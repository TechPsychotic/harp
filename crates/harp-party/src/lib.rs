//! Listening together (roadmap M7), peer to peer over iroh, the way Warptron's
//! party mode does it: no accounts and no servers of our own (iroh's public
//! relays only help two devices find each other and pass traffic when a
//! direct path can't be made).
//!
//! One person **hosts**; friends **join** with the host's invite link. The host's
//! player is the truth: it sends its state (which song, playing or paused, where)
//! whenever it changes and every few seconds while playing, and each guest's
//! player follows. Every device streams the song itself, so only a few hundred
//! bytes cross the connection. A playing position is moved on by half the
//! connection's round trip on arrival, so everyone hears the same moment.
//!
//! Guests can steer: play, pause, skip, seek and suggest songs. Those go to the
//! host, whose player acts and then tells everyone.
//!
//! Wire format: one bidirectional QUIC stream per guest, newline-separated JSON.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr, EndpointId, RelayUrl,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
};

pub mod sync;

pub const ALPN: &[u8] = b"harp/party/1";

/// While playing, the host repeats its state this often (corrects drift and
/// lost messages).
pub const HEARTBEAT: Duration = Duration::from_secs(4);

/// A song, as much as a guest needs to find and play it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<u32>,
}

/// Where the host's playback is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayState {
    pub track: Option<Track>,
    pub playing: bool,
    pub position_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Msg {
    Hello { name: String },
    Welcome { host: String, state: PlayState, members: Vec<String> },
    State { state: PlayState },
    Members { names: Vec<String> },
    /// "play", "pause", "next", "previous" or "seek" (with `position_ms`).
    Control { action: String, position_ms: u64 },
    Request { track: Track },
    Bye,
}

/// What happens in a party, for the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Host: someone joined / left.
    GuestJoined { name: String },
    GuestLeft { name: String },
    /// Host: a guest pressed something.
    Control { from: String, action: String, position_ms: u64 },
    /// Host: a guest suggests a song.
    Request { from: String, track: Track },
    /// Guest: we're in.
    Connected { host: String },
    /// Guest: play this, like this (position already corrected for the trip).
    State(PlayState),
    /// Everyone in the party (host first).
    Members(Vec<String>),
    /// Guest: the party ended or the connection dropped.
    Ended { reason: String },
}

/// `harp://party/<host id>?relay=<home relay>&name=<host's name>`. The id alone
/// is enough to reach the host; the relay makes the first connection quicker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    pub id: EndpointId,
    pub relay: Option<RelayUrl>,
    pub name: String,
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
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Invite {
    pub fn to_link(&self) -> String {
        let mut s = format!("harp://party/{}", self.id);
        let mut q = vec![];
        if let Some(r) = &self.relay {
            q.push(format!("relay={}", enc(r.as_str())));
        }
        if !self.name.is_empty() {
            q.push(format!("name={}", enc(&self.name)));
        }
        if !q.is_empty() {
            s.push('?');
            s.push_str(&q.join("&"));
        }
        s
    }

    /// Reads an invite link (or a bare host id).
    pub fn parse(text: &str) -> Result<Invite> {
        let text = text.trim();
        if let Ok(id) = text.parse::<EndpointId>() {
            return Ok(Invite { id, relay: None, name: String::new() });
        }
        let rest = text.strip_prefix("harp://party/").context("not a Harp party link")?;
        let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
        let id = id.trim_end_matches('/').parse::<EndpointId>().map_err(|_| anyhow!("the party link is damaged"))?;
        let mut inv = Invite { id, relay: None, name: String::new() };
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                // A bad relay only loses the shortcut; the id still works.
                "relay" => inv.relay = dec(v).parse().ok(),
                "name" => inv.name = dec(v).chars().take(64).collect(),
                _ => {}
            }
        }
        Ok(inv)
    }

    fn addr(&self) -> EndpointAddr {
        let a = EndpointAddr::new(self.id);
        match &self.relay {
            Some(r) => a.with_relay_url(r.clone()),
            None => a,
        }
    }
}

struct Guest {
    id: u64,
    name: String,
    tx: mpsc::UnboundedSender<Msg>,
}

struct Inner {
    name: Mutex<String>,
    events: mpsc::UnboundedSender<Event>,
    hosting: AtomicBool,
    /// Host: the last state and when it was set (a playing position moves on).
    state: Mutex<(PlayState, Instant)>,
    guests: Mutex<Vec<Guest>>,
    next_guest: AtomicU64,
    /// Guest: the link to the host.
    link: Mutex<Option<(Connection, mpsc::UnboundedSender<Msg>)>>,
}

impl Inner {
    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    fn now_state(&self) -> PlayState {
        let (mut s, at) = self.state.lock().unwrap().clone();
        if s.playing {
            s.position_ms += at.elapsed().as_millis() as u64;
        }
        s
    }

    fn members(&self) -> Vec<String> {
        let mut v = vec![self.name.lock().unwrap().clone()];
        v.extend(self.guests.lock().unwrap().iter().map(|g| g.name.clone()));
        v
    }

    fn broadcast(&self, make: impl Fn() -> Msg) {
        self.guests.lock().unwrap().retain(|g| g.tx.send(make()).is_ok());
    }

    fn members_changed(&self) {
        let names = self.members();
        self.emit(Event::Members(names.clone()));
        self.broadcast(|| Msg::Members { names: names.clone() });
    }
}

/// Writes messages to a stream, one JSON line each, until the channel closes.
fn spawn_writer(mut send: SendStream) -> mpsc::UnboundedSender<Msg> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            let bye = matches!(m, Msg::Bye);
            let Ok(mut line) = serde_json::to_vec(&m) else { continue };
            line.push(b'\n');
            if send.write_all(&line).await.is_err() {
                break;
            }
            if bye {
                break;
            }
        }
        let _ = send.finish();
    });
    tx
}

/// Half the round trip of the connection's current path.
fn half_rtt(conn: &Connection) -> Duration {
    conn.paths().iter().find(|p| p.is_selected()).map(|p| p.rtt() / 2).unwrap_or_default()
}

/// Lines are capped so a misbehaving peer can't make us buffer without end.
const MAX_LINE: usize = 64 * 1024;

async fn read_msg(lines: &mut BufReader<RecvStream>, buf: &mut String) -> Result<Option<Msg>> {
    buf.clear();
    let n = lines.read_line(buf).await?;
    if n == 0 {
        return Ok(None);
    }
    ensure!(buf.len() <= MAX_LINE, "message too long");
    Ok(Some(serde_json::from_str(buf.trim_end())?))
}

#[derive(Clone)]
struct Handler(Arc<Inner>);

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("party")
    }
}

impl ProtocolHandler for Handler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let _ = serve_guest(self.0.clone(), conn).await;
        Ok(())
    }
}

/// Host: one guest, from hello to goodbye.
async fn serve_guest(inner: Arc<Inner>, conn: Connection) -> Result<()> {
    ensure!(inner.hosting.load(Ordering::SeqCst), "not hosting");
    let (send, recv) = conn.accept_bi().await?;
    let mut lines = BufReader::new(recv);
    let mut buf = String::new();
    let name = match tokio::time::timeout(Duration::from_secs(10), read_msg(&mut lines, &mut buf)).await?? {
        Some(Msg::Hello { name }) => {
            let n: String = name.trim().chars().take(40).collect();
            if n.is_empty() { "Guest".to_string() } else { n }
        }
        _ => bail!("no hello"),
    };
    let id = inner.next_guest.fetch_add(1, Ordering::SeqCst);
    let tx = spawn_writer(send);
    let host = inner.name.lock().unwrap().clone();
    inner.guests.lock().unwrap().push(Guest { id, name: name.clone(), tx: tx.clone() });
    let _ = tx.send(Msg::Welcome { host, state: inner.now_state(), members: inner.members() });
    inner.emit(Event::GuestJoined { name: name.clone() });
    inner.members_changed();
    loop {
        match read_msg(&mut lines, &mut buf).await {
            Ok(Some(Msg::Control { action, position_ms })) => inner.emit(Event::Control { from: name.clone(), action, position_ms }),
            Ok(Some(Msg::Request { track })) => inner.emit(Event::Request { from: name.clone(), track }),
            Ok(Some(Msg::Bye)) | Ok(None) | Err(_) => break,
            Ok(Some(_)) => {}
        }
    }
    inner.guests.lock().unwrap().retain(|g| g.id != id);
    if inner.hosting.load(Ordering::SeqCst) {
        inner.emit(Event::GuestLeft { name });
        inner.members_changed();
    }
    Ok(())
}

/// A party on this device: host one, or join one.
pub struct Party {
    endpoint: Endpoint,
    local_only: bool,
    router: Mutex<Option<Router>>,
    inner: Arc<Inner>,
}

impl Party {
    /// Starts the network side. `local_only`: no relays or address lookup (tests).
    pub async fn bind(name: &str, local_only: bool) -> Result<(Party, mpsc::UnboundedReceiver<Event>)> {
        let endpoint = if local_only {
            Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled).bind().await?
        } else {
            Endpoint::builder(presets::N0).bind().await?
        };
        let (events, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            name: Mutex::new(name.trim().chars().take(40).collect()),
            events,
            hosting: AtomicBool::new(false),
            state: Mutex::new((PlayState::default(), Instant::now())),
            guests: Mutex::new(vec![]),
            next_guest: AtomicU64::new(1),
            link: Mutex::new(None),
        });
        Ok((Party { endpoint, local_only, router: Mutex::new(None), inner }, rx))
    }

    pub fn set_name(&self, name: &str) {
        *self.inner.name.lock().unwrap() = name.trim().chars().take(40).collect();
    }

    pub fn is_hosting(&self) -> bool {
        self.inner.hosting.load(Ordering::SeqCst)
    }

    pub fn is_guest(&self) -> bool {
        self.inner.link.lock().unwrap().is_some()
    }

    /// The address others connect to (tests use it directly).
    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Starts hosting. Returns the invite link to share.
    pub async fn host(&self) -> Result<String> {
        ensure!(!self.is_guest(), "leave the party you're in first");
        if !self.is_hosting() {
            self.inner.hosting.store(true, Ordering::SeqCst);
            let router = Router::builder(self.endpoint.clone()).accept(ALPN, Handler(self.inner.clone())).spawn();
            *self.router.lock().unwrap() = Some(router);
            // Heartbeat: while playing, the state again every few seconds.
            let weak = Arc::downgrade(&self.inner);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(HEARTBEAT).await;
                    let Some(inner) = weak.upgrade() else { return };
                    if !inner.hosting.load(Ordering::SeqCst) {
                        return;
                    }
                    let s = inner.now_state();
                    if s.playing {
                        inner.broadcast(|| Msg::State { state: s.clone() });
                    }
                }
            });
            self.inner.members_changed();
        }
        // Wait (briefly) to know our home relay, so the link carries it.
        if !self.local_only {
            let _ = tokio::time::timeout(Duration::from_secs(8), self.endpoint.online()).await;
        }
        let relay = self.endpoint.addr().relay_urls().next().cloned();
        Ok(Invite { id: self.endpoint.id(), relay, name: self.inner.name.lock().unwrap().clone() }.to_link())
    }

    /// Host: our player changed (or a guest's request was carried out).
    pub fn publish(&self, state: PlayState) {
        *self.inner.state.lock().unwrap() = (state.clone(), Instant::now());
        if self.is_hosting() {
            self.inner.broadcast(|| Msg::State { state: state.clone() });
        }
    }

    /// Joins a party from its invite link.
    pub async fn join(&self, link: &str) -> Result<()> {
        ensure!(!self.is_hosting(), "stop hosting first");
        let inv = Invite::parse(link)?;
        self.join_addr(inv.addr()).await
    }

    /// Joins at a known address (tests).
    pub async fn join_addr(&self, addr: EndpointAddr) -> Result<()> {
        self.leave().await;
        let conn = tokio::time::timeout(Duration::from_secs(20), self.endpoint.connect(addr, ALPN))
            .await
            .context("the host didn't answer in time")??;
        let (send, recv) = conn.open_bi().await?;
        let tx = spawn_writer(send);
        let _ = tx.send(Msg::Hello { name: self.inner.name.lock().unwrap().clone() });
        *self.inner.link.lock().unwrap() = Some((conn.clone(), tx));
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(recv);
            let mut buf = String::new();
            let reason = loop {
                match read_msg(&mut lines, &mut buf).await {
                    Ok(Some(msg)) => {
                        let fix = |mut s: PlayState| {
                            if s.playing {
                                s.position_ms += half_rtt(&conn).as_millis() as u64;
                            }
                            s
                        };
                        match msg {
                            Msg::Welcome { host, state, members } => {
                                inner.emit(Event::Connected { host });
                                inner.emit(Event::Members(members));
                                inner.emit(Event::State(fix(state)));
                            }
                            Msg::State { state } => inner.emit(Event::State(fix(state))),
                            Msg::Members { names } => inner.emit(Event::Members(names)),
                            Msg::Bye => break "The host ended the party".to_string(),
                            _ => {}
                        }
                    }
                    Ok(None) => break "The host left".to_string(),
                    Err(e) => break format!("Lost the connection ({e})"),
                }
            };
            // Only report the end if this is still our link (not a newer join).
            let mine = inner.link.lock().unwrap().as_ref().is_some_and(|(c, _)| c.stable_id() == conn.stable_id());
            if mine {
                *inner.link.lock().unwrap() = None;
                inner.emit(Event::Ended { reason });
            }
        });
        Ok(())
    }

    /// Guest: play, pause, next, previous, or seek (to `position_ms`).
    pub fn control(&self, action: &str, position_ms: u64) -> Result<()> {
        let link = self.inner.link.lock().unwrap();
        let (_, tx) = link.as_ref().context("not in a party")?;
        tx.send(Msg::Control { action: action.into(), position_ms }).map_err(|_| anyhow!("the connection closed"))
    }

    /// Guest: suggest a song to the host.
    pub fn request(&self, track: Track) -> Result<()> {
        let link = self.inner.link.lock().unwrap();
        let (_, tx) = link.as_ref().context("not in a party")?;
        tx.send(Msg::Request { track }).map_err(|_| anyhow!("the connection closed"))
    }

    /// Leaves (guest) or ends (host) the party.
    pub async fn leave(&self) {
        let link = self.inner.link.lock().unwrap().take();
        if let Some((conn, tx)) = link {
            let _ = tx.send(Msg::Bye);
            tokio::time::sleep(Duration::from_millis(150)).await;
            conn.close(0u32.into(), b"left");
        }
        if self.inner.hosting.swap(false, Ordering::SeqCst) {
            self.inner.broadcast(|| Msg::Bye);
            self.inner.guests.lock().unwrap().clear();
            let router = self.router.lock().unwrap().take();
            tokio::time::sleep(Duration::from_millis(150)).await;
            if let Some(r) = router {
                let _ = r.shutdown().await;
            }
        }
    }

    pub async fn close(self) {
        self.leave().await;
        self.endpoint.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track { id: id.into(), title: format!("Song {id}"), artist: "A".into(), album: None, thumbnail: None, duration_secs: Some(200) }
    }

    async fn next(rx: &mut mpsc::UnboundedReceiver<Event>, want: impl Fn(&Event) -> bool) -> Event {
        loop {
            let e = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("event in time").expect("channel open");
            if want(&e) {
                return e;
            }
        }
    }

    #[test]
    fn invite_links_round_trip() {
        let id = iroh::SecretKey::generate().public();
        let inv = Invite { id, relay: Some("https://euc1-1.relay.n0.iroh-canary.iroh.link./".parse().unwrap()), name: "Vansh's phone & co".into() };
        let link = inv.to_link();
        assert!(link.starts_with(&format!("harp://party/{id}?relay=")), "{link}");
        assert_eq!(Invite::parse(&link).unwrap(), inv);
        assert_eq!(Invite::parse(&format!("  {id} ")).unwrap().id, id);
        assert!(Invite::parse("https://example.com").is_err());
        assert!(Invite::parse("harp://party/not-an-id").is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn host_and_guest_stay_in_sync() {
        let (host, mut hrx) = Party::bind("Host", true).await.unwrap();
        let (guest, mut grx) = Party::bind("Guest", true).await.unwrap();
        host.host().await.unwrap();
        host.publish(PlayState { track: Some(track("a")), playing: true, position_ms: 10_000 });

        guest.join_addr(host.addr()).await.unwrap();
        assert_eq!(next(&mut grx, |e| matches!(e, Event::Connected { .. })).await, Event::Connected { host: "Host".into() });
        // Welcome carries the host's state, moved on by the time that passed.
        let Event::State(s) = next(&mut grx, |e| matches!(e, Event::State(_))).await else { unreachable!() };
        assert_eq!(s.track.unwrap().id, "a");
        assert!(s.playing && s.position_ms >= 10_000 && s.position_ms < 15_000, "{}", s.position_ms);
        assert_eq!(next(&mut hrx, |e| matches!(e, Event::GuestJoined { .. })).await, Event::GuestJoined { name: "Guest".into() });
        let Event::Members(m) = next(&mut grx, |e| matches!(e, Event::Members(m) if m.len() == 2)).await else { unreachable!() };
        assert_eq!(m, ["Host", "Guest"]);

        // The host skips: the guest hears about it.
        host.publish(PlayState { track: Some(track("b")), playing: true, position_ms: 0 });
        let Event::State(s) = next(&mut grx, |e| matches!(e, Event::State(s) if s.track.as_ref().is_some_and(|t| t.id == "b"))).await else { unreachable!() };
        assert!(s.position_ms < 2_000);

        // The guest steers and suggests; the host hears it.
        guest.control("pause", 0).unwrap();
        assert_eq!(
            next(&mut hrx, |e| matches!(e, Event::Control { .. })).await,
            Event::Control { from: "Guest".into(), action: "pause".into(), position_ms: 0 }
        );
        guest.request(track("c")).unwrap();
        assert_eq!(next(&mut hrx, |e| matches!(e, Event::Request { .. })).await, Event::Request { from: "Guest".into(), track: track("c") });

        // The host ends it: the guest is told.
        host.leave().await;
        assert!(matches!(next(&mut grx, |e| matches!(e, Event::Ended { .. })).await, Event::Ended { .. }));
        assert!(!guest.is_guest());
        host.close().await;
        guest.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_guest_leaving_is_noticed() {
        let (host, mut hrx) = Party::bind("Host", true).await.unwrap();
        let (guest, _grx) = Party::bind("Ana", true).await.unwrap();
        host.host().await.unwrap();
        guest.join_addr(host.addr()).await.unwrap();
        next(&mut hrx, |e| matches!(e, Event::GuestJoined { .. })).await;
        guest.leave().await;
        assert_eq!(next(&mut hrx, |e| matches!(e, Event::GuestLeft { .. })).await, Event::GuestLeft { name: "Ana".into() });
        let Event::Members(m) = next(&mut hrx, |e| matches!(e, Event::Members(_))).await else { unreachable!() };
        assert_eq!(m, ["Host"]);
        host.close().await;
        guest.close().await;
    }

    /// Over the internet, joining with only the invite link (as people will).
    /// `cargo test -p harp-party -- --ignored live_party`
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn live_party_over_the_internet() {
        let (host, _hrx) = Party::bind("Host", false).await.unwrap();
        let (guest, mut grx) = Party::bind("Guest", false).await.unwrap();
        let link = host.host().await.unwrap();
        eprintln!("link: {link}");
        assert!(link.contains("relay="), "the link carries a relay");
        host.publish(PlayState { track: Some(track("live")), playing: true, position_ms: 5_000 });
        let t = Instant::now();
        guest.join(&link).await.unwrap();
        let Event::State(s) = next(&mut grx, |e| matches!(e, Event::State(_))).await else { unreachable!() };
        eprintln!("joined and in sync in {} ms", t.elapsed().as_millis());
        assert_eq!(s.track.unwrap().id, "live");
        host.close().await;
        guest.close().await;
    }
}
