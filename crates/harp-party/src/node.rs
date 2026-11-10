//! This device on the network, with a lasting identity (its key is kept by the
//! app), for the things that need to find it again later:
//!
//! * **Pairing and sync** between your own devices. The first sync carries a
//!   one-time code; after it, both devices remember each other and sync by
//!   themselves whenever both are online. Only paired devices are let in.
//! * **Shared playlists that stay up to date.** A playlist shared from here has
//!   a lasting link (`harp://playlist/<this device>?p=<token>`); friends who
//!   saved it fetch the current version whenever they open Harp.
//!
//! Bytes in and out (the library backup, the playlist) come from the app
//! through [`Hooks`]; this module only carries them.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub const SYNC_ALPN: &[u8] = b"harp/sync/2";
pub const PLAYLIST_ALPN: &[u8] = b"harp/playlist/2";
const MAX_BYTES: u64 = 256 * 1024 * 1024;

/// What the app provides.
pub struct Hooks {
    /// This device's library (a backup).
    pub library: Box<dyn Fn() -> Result<Vec<u8>> + Send + Sync>,
    /// Adds another device's library to ours.
    pub merge: Box<dyn Fn(Vec<u8>) -> Result<()> + Send + Sync>,
    /// A shared playlist by its token (None: not shared, or not any more).
    pub playlist: Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>,
    /// A device was paired (remember it: id, its name).
    pub paired: Box<dyn Fn(EndpointId, String) + Send + Sync>,
    /// Whether automatic syncing is on here (pairing with a code always works).
    pub auto_sync: Box<dyn Fn() -> bool + Send + Sync>,
}

/// A link: `harp://<kind>/<id>?relay=…&name=…&<more>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub kind: String,
    pub id: EndpointId,
    pub relay: Option<RelayUrl>,
    pub params: HashMap<String, String>,
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Link {
    pub fn to_string(&self) -> String {
        let mut q: Vec<String> = vec![];
        if let Some(r) = &self.relay {
            q.push(format!("relay={}", enc(r.as_str())));
        }
        let mut keys: Vec<_> = self.params.keys().collect();
        keys.sort();
        for k in keys {
            q.push(format!("{k}={}", enc(&self.params[k])));
        }
        format!("harp://{}/{}{}{}", self.kind, self.id, if q.is_empty() { "" } else { "?" }, q.join("&"))
    }

    pub fn parse(text: &str) -> Result<Link> {
        let rest = text.trim().strip_prefix("harp://").context("not a Harp link")?;
        let (kind, rest) = rest.split_once('/').context("not a Harp link")?;
        let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
        let id = id.trim_end_matches('/').parse::<EndpointId>().map_err(|_| anyhow!("the link is damaged"))?;
        let mut link = Link { kind: kind.into(), id, relay: None, params: HashMap::new() };
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if k == "relay" {
                link.relay = dec(v).parse().ok();
            } else {
                link.params.insert(k.into(), dec(v));
            }
        }
        Ok(link)
    }

    pub fn addr(&self) -> EndpointAddr {
        let a = EndpointAddr::new(self.id);
        match &self.relay {
            Some(r) => a.with_relay_url(r.clone()),
            None => a,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SyncHello {
    name: String,
    /// The one-time code from a pairing link (absent between paired devices).
    code: Option<String>,
}

struct State {
    hooks: Hooks,
    name: Mutex<String>,
    paired: Mutex<HashSet<EndpointId>>,
    /// An open pairing offer: its code, and who to tell when it's used.
    pairing: Mutex<Option<(String, mpsc::Sender<Result<String>>)>>,
}

async fn send_blob(send: &mut SendStream, data: &[u8]) -> Result<()> {
    send.write_all(&(data.len() as u64).to_be_bytes()).await?;
    send.write_all(data).await?;
    Ok(())
}

async fn recv_blob(recv: &mut RecvStream) -> Result<Vec<u8>> {
    let mut len = [0u8; 8];
    recv.read_exact(&mut len).await?;
    let n = u64::from_be_bytes(len);
    ensure!(n <= MAX_BYTES, "too big ({n} bytes)");
    let mut buf = vec![0u8; n as usize];
    recv.read_exact(&mut buf).await?;
    Ok(buf)
}

#[derive(Clone)]
struct SyncHandler(Arc<State>);
#[derive(Clone)]
struct PlaylistHandler(Arc<State>);

impl std::fmt::Debug for SyncHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("sync")
    }
}
impl std::fmt::Debug for PlaylistHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("playlist")
    }
}

impl ProtocolHandler for SyncHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let st = self.0.clone();
        let peer = conn.remote_id();
        let r = async {
            let (mut send, mut recv) = conn.accept_bi().await?;
            let hello: SyncHello = serde_json::from_slice(&recv_blob(&mut recv).await?)?;
            // Paired devices, or someone holding the open pairing code. Nobody else.
            let via_code = {
                let p = st.pairing.lock().unwrap();
                matches!((&*p, &hello.code), (Some((c, _)), Some(h)) if c == h)
            };
            let known = st.paired.lock().unwrap().contains(&peer);
            if known && !via_code && !(st.hooks.auto_sync)() {
                send_blob(&mut send, b"off").await?;
                send.finish()?;
                bail!("automatic sync is off on this device");
            }
            if !known && !via_code {
                send_blob(&mut send, b"no").await?;
                send.finish()?;
                bail!("not a paired device");
            }
            let my_name = st.name.lock().unwrap().clone();
            send_blob(&mut send, my_name.as_bytes()).await?;
            let theirs = recv_blob(&mut recv).await?;
            let ours = (st.hooks.library)()?;
            send_blob(&mut send, &ours).await?;
            send.finish()?;
            let _ = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;
            (st.hooks.merge)(theirs)?;
            if !known {
                st.paired.lock().unwrap().insert(peer);
                (st.hooks.paired)(peer, hello.name.clone());
            }
            Ok::<_, anyhow::Error>((via_code, hello.name))
        }
        .await;
        // Tell a waiting pairing offer how it went.
        if let Ok((true, _)) | Err(_) = &r {
            let waiting = st.pairing.lock().unwrap().take();
            match (waiting, r) {
                (Some((_, tx)), Ok((_, name))) => {
                    let _ = tx.try_send(Ok(name));
                }
                (Some((code, tx)), Err(e)) => {
                    // A stranger's failed attempt doesn't end the offer.
                    if e.to_string().contains("not a paired device") {
                        *st.pairing.lock().unwrap() = Some((code, tx));
                    } else {
                        let _ = tx.try_send(Err(e));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl ProtocolHandler for PlaylistHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let st = self.0.clone();
        let _ = async {
            let (mut send, mut recv) = conn.accept_bi().await?;
            let token = String::from_utf8(recv_blob(&mut recv).await?)?;
            let data = (st.hooks.playlist)(&token).unwrap_or_default();
            send_blob(&mut send, &data).await?;
            send.finish()?;
            let _ = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        Ok(())
    }
}

/// This device on the network.
pub struct Node {
    endpoint: Endpoint,
    router: Router,
    state: Arc<State>,
    local_only: bool,
}

impl Node {
    /// Starts with this device's lasting key (32 bytes), its name, the devices
    /// already paired, and the app's hooks.
    pub async fn start(key: [u8; 32], name: &str, paired: Vec<EndpointId>, hooks: Hooks, local_only: bool) -> Result<Node> {
        let secret = SecretKey::from_bytes(&key);
        let endpoint = if local_only {
            Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled).secret_key(secret).bind().await?
        } else {
            Endpoint::builder(presets::N0).secret_key(secret).bind().await?
        };
        let state = Arc::new(State {
            hooks,
            name: Mutex::new(name.into()),
            paired: Mutex::new(paired.into_iter().collect()),
            pairing: Mutex::new(None),
        });
        let router = Router::builder(endpoint.clone())
            .accept(SYNC_ALPN, SyncHandler(state.clone()))
            .accept(PLAYLIST_ALPN, PlaylistHandler(state.clone()))
            .spawn();
        Ok(Node { endpoint, router, state, local_only })
    }

    pub fn id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    pub fn set_name(&self, name: &str) {
        *self.state.name.lock().unwrap() = name.into();
    }

    pub fn unpair(&self, id: EndpointId) {
        self.state.paired.lock().unwrap().remove(&id);
    }

    async fn relay(&self) -> Option<RelayUrl> {
        if !self.local_only {
            let _ = tokio::time::timeout(Duration::from_secs(8), self.endpoint.online()).await;
        }
        self.endpoint.addr().relay_urls().next().cloned()
    }

    async fn link(&self, kind: &str, params: &[(&str, &str)]) -> String {
        let mut p: HashMap<String, String> = params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        p.insert("name".into(), self.state.name.lock().unwrap().clone());
        Link { kind: kind.into(), id: self.id(), relay: self.relay().await, params: p }.to_string()
    }

    /// A pairing link with a fresh one-time code. Then [`Node::wait_pairing`].
    pub async fn offer_pairing(&self, code: &str) -> (String, mpsc::Receiver<Result<String>>) {
        let (tx, rx) = mpsc::channel(1);
        *self.state.pairing.lock().unwrap() = Some((code.into(), tx));
        (self.link("sync", &[("code", code)]).await, rx)
    }

    pub fn cancel_pairing(&self) {
        if let Some((_, tx)) = self.state.pairing.lock().unwrap().take() {
            let _ = tx.try_send(Err(anyhow!("cancelled")));
        }
    }

    /// The lasting link to a shared playlist.
    pub async fn playlist_link(&self, token: &str, name: &str) -> String {
        let mut l = Link::parse(&self.link("playlist", &[("p", token)]).await).expect("own link");
        l.params.insert("name".into(), name.into());
        l.to_string()
    }

    /// Syncs with a device: joining with a pairing link (`code`), or a paired
    /// device by address. Merges its library; returns its name.
    pub async fn sync_with(&self, addr: EndpointAddr, code: Option<String>) -> Result<String> {
        let peer = addr.id;
        let conn = tokio::time::timeout(Duration::from_secs(20), self.endpoint.connect(addr, SYNC_ALPN))
            .await
            .context("the other device didn't answer (is Harp open on it?)")??;
        let (mut send, mut recv) = conn.open_bi().await?;
        let my_name = self.state.name.lock().unwrap().clone();
        let hello = SyncHello { name: my_name, code };
        send_blob(&mut send, &serde_json::to_vec(&hello)?).await?;
        let first = recv_blob(&mut recv).await?;
        ensure!(first != b"no", "that device doesn't know this one (pair them first, or the code expired)");
        ensure!(first != b"off", "automatic sync is turned off on that device");
        let their_name = String::from_utf8_lossy(&first).into_owned();
        send_blob(&mut send, &(self.state.hooks.library)()?).await?;
        send.finish()?;
        let theirs = recv_blob(&mut recv).await?;
        conn.close(0u32.into(), b"done");
        (self.state.hooks.merge)(theirs)?;
        if self.state.paired.lock().unwrap().insert(peer) {
            (self.state.hooks.paired)(peer, their_name.clone());
        }
        Ok(their_name)
    }

    /// Fetches a shared playlist (current version) from its link.
    pub async fn fetch_playlist(&self, link: &Link) -> Result<Vec<u8>> {
        let token = link.params.get("p").context("the link has no playlist in it")?;
        let conn = tokio::time::timeout(Duration::from_secs(20), self.endpoint.connect(link.addr(), PLAYLIST_ALPN))
            .await
            .context("the person sharing isn't reachable right now")??;
        let (mut send, mut recv) = conn.open_bi().await?;
        send_blob(&mut send, token.as_bytes()).await?;
        send.finish()?;
        let data = recv_blob(&mut recv).await?;
        conn.close(0u32.into(), b"thanks");
        ensure!(!data.is_empty(), "this playlist isn't shared any more");
        Ok(data)
    }

    pub async fn close(self) {
        let _ = self.router.shutdown().await;
        self.endpoint.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        lib: Arc<Mutex<Vec<String>>>,
        paired: Arc<Mutex<Vec<String>>>,
        shared: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    fn hooks(f: &Fake) -> Hooks {
        let (a, b, c, d) = (f.lib.clone(), f.lib.clone(), f.shared.clone(), f.paired.clone());
        Hooks {
            library: Box::new(move || Ok(a.lock().unwrap().join(",").into_bytes())),
            merge: Box::new(move |bytes| {
                let mut l = b.lock().unwrap();
                for x in String::from_utf8(bytes)?.split(',').filter(|x| !x.is_empty()) {
                    if !l.contains(&x.to_string()) {
                        l.push(x.into());
                    }
                }
                Ok(())
            }),
            playlist: Box::new(move |t| c.lock().unwrap().get(t).cloned()),
            paired: Box::new(move |_, name| d.lock().unwrap().push(name)),
            auto_sync: Box::new(|| true),
        }
    }

    fn fake(items: &[&str]) -> Fake {
        Fake { lib: Arc::new(Mutex::new(items.iter().map(|s| s.to_string()).collect())), paired: Arc::default(), shared: Arc::default() }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pair_once_then_sync_by_themselves_and_strangers_stay_out() {
        let (pf, lf, sf) = (fake(&["a"]), fake(&["b"]), fake(&["x"]));
        let phone = Node::start([1; 32], "Phone", vec![], hooks(&pf), true).await.unwrap();
        let laptop = Node::start([2; 32], "Laptop", vec![], hooks(&lf), true).await.unwrap();
        let stranger = Node::start([3; 32], "Stranger", vec![], hooks(&sf), true).await.unwrap();

        // The key gives a lasting identity.
        let again = Node::start([1; 32], "Phone", vec![], hooks(&fake(&[])), true).await.unwrap();
        assert_eq!(again.id(), phone.id());
        again.close().await;

        // A stranger without a code is refused.
        assert!(stranger.sync_with(phone.addr(), None).await.is_err());
        assert_eq!(*pf.lib.lock().unwrap(), ["a"]);

        // Pairing: the laptop uses the phone's one-time code.
        let (link, mut wait) = phone.offer_pairing("1234").await;
        assert!(link.starts_with("harp://sync/") && link.contains("code=1234"));
        let parsed = Link::parse(&link).unwrap();
        assert_eq!(laptop.sync_with(phone.addr(), parsed.params.get("code").cloned()).await.unwrap(), "Phone");
        assert_eq!(wait.recv().await.unwrap().unwrap(), "Laptop");
        assert_eq!(*lf.lib.lock().unwrap(), ["b", "a"]);
        assert_eq!(*pf.lib.lock().unwrap(), ["a", "b"]);
        assert_eq!(*pf.paired.lock().unwrap(), ["Laptop"]);
        assert_eq!(*lf.paired.lock().unwrap(), ["Phone"]);

        // Later, by themselves (no code): new things go across.
        pf.lib.lock().unwrap().push("c".into());
        laptop.sync_with(phone.addr(), None).await.unwrap();
        assert!(lf.lib.lock().unwrap().contains(&"c".to_string()));
        // The code was single-use: the stranger still can't get in with it.
        assert!(stranger.sync_with(phone.addr(), Some("1234".into())).await.is_err());

        for n in [phone, laptop, stranger] {
            n.close().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shared_playlists_stay_current() {
        let sharer_f = fake(&[]);
        sharer_f.shared.lock().unwrap().insert("tok".into(), b"v1".to_vec());
        let sharer = Node::start([4; 32], "Ana", vec![], hooks(&sharer_f), true).await.unwrap();
        let friend = Node::start([5; 32], "Ben", vec![], hooks(&fake(&[])), true).await.unwrap();
        let link = sharer.playlist_link("tok", "Road trip").await;
        let mut l = Link::parse(&link).unwrap();
        assert_eq!(l.params.get("name").map(String::as_str), Some("Road trip"));
        l.relay = None;
        // Local test: reach it at its known address.
        let fetch = |l: &Link| {
            let mut l = l.clone();
            l.id = sharer.id();
            l
        };
        let mut addr_link = fetch(&l);
        addr_link.relay = None;
        let conn_addr = sharer.addr();
        // Same id, direct address.
        let got = {
            let mut ll = addr_link.clone();
            ll.id = conn_addr.id;
            friend_fetch(&friend, &ll, conn_addr.clone()).await.unwrap()
        };
        assert_eq!(got, b"v1");
        sharer_f.shared.lock().unwrap().insert("tok".into(), b"v2".to_vec());
        assert_eq!(friend_fetch(&friend, &addr_link, conn_addr.clone()).await.unwrap(), b"v2", "the friend gets the new version");
        sharer_f.shared.lock().unwrap().remove("tok");
        assert!(friend_fetch(&friend, &addr_link, conn_addr).await.is_err(), "stopped sharing");
        sharer.close().await;
        friend.close().await;
    }

    /// fetch_playlist, but at a direct address (no relay in local tests).
    async fn friend_fetch(n: &Node, l: &Link, addr: EndpointAddr) -> Result<Vec<u8>> {
        let token = l.params.get("p").unwrap();
        let conn = n.endpoint.connect(addr, PLAYLIST_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;
        send_blob(&mut send, token.as_bytes()).await?;
        send.finish()?;
        let data = recv_blob(&mut recv).await?;
        ensure!(!data.is_empty(), "not shared");
        Ok(data)
    }

    #[test]
    fn links_round_trip() {
        let id = SecretKey::from_bytes(&[9; 32]).public();
        let mut params = HashMap::new();
        params.insert("p".to_string(), "abc".to_string());
        params.insert("name".to_string(), "Ana's list & more".to_string());
        let l = Link { kind: "playlist".into(), id, relay: Some("https://aps1-1.relay.n0.iroh.link./".parse().unwrap()), params };
        assert_eq!(Link::parse(&l.to_string()).unwrap(), l);
        assert!(Link::parse("https://example.com").is_err());
    }
}
