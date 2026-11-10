//! Syncing your library between your own devices (roadmap M7), over iroh.
//!
//! One device **offers** (shows a link), the other **joins** with it. Each sends
//! the other a backup of its library, and each then *adds* what it's missing
//! (likes, playlists, history, follows…), so both end up with everything and
//! neither loses anything. The bytes are the app's backup format; this module
//! only carries them.
//!
//! Wire: the joiner opens one stream, sends its backup (8-byte length, then the
//! bytes) and finishes; the offerer answers with its own the same way.

use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use iroh::{
    endpoint::{presets, Connection},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr,
};
use tokio::sync::{mpsc, Mutex};

use crate::Invite;

pub const ALPN: &[u8] = b"harp/sync/1";

/// Libraries are a few MB; this stops a bad peer from making us buffer without end.
const MAX_BYTES: u64 = 256 * 1024 * 1024;

async fn send_blob(send: &mut iroh::endpoint::SendStream, data: &[u8]) -> Result<()> {
    send.write_all(&(data.len() as u64).to_be_bytes()).await?;
    send.write_all(data).await?;
    Ok(())
}

async fn recv_blob(recv: &mut iroh::endpoint::RecvStream) -> Result<Vec<u8>> {
    let mut len = [0u8; 8];
    recv.read_exact(&mut len).await?;
    let n = u64::from_be_bytes(len);
    ensure!(n <= MAX_BYTES, "the other library is too big ({n} bytes)");
    let mut buf = vec![0u8; n as usize];
    recv.read_exact(&mut buf).await?;
    Ok(buf)
}

#[derive(Clone)]
struct Handler {
    ours: std::sync::Arc<Vec<u8>>,
    got: mpsc::Sender<Result<Vec<u8>>>,
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("sync")
    }
}

impl ProtocolHandler for Handler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let r = async {
            let (mut send, mut recv) = conn.accept_bi().await?;
            let theirs = recv_blob(&mut recv).await?;
            send_blob(&mut send, &self.ours).await?;
            send.finish()?;
            // Let the bytes reach them before the connection goes.
            let _ = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;
            Ok::<_, anyhow::Error>(theirs)
        }
        .await;
        let _ = self.got.send(r).await;
        Ok(())
    }
}

/// A sync offer: show [`Offer::link`], then [`Offer::wait`] for the other device.
pub struct Offer {
    endpoint: Endpoint,
    router: Router,
    got: Mutex<mpsc::Receiver<Result<Vec<u8>>>>,
    cancel: mpsc::Sender<Result<Vec<u8>>>,
    pub link: String,
}

impl Offer {
    /// Starts an offer with this device's library (`ours`). `local_only`: tests.
    pub async fn start(ours: Vec<u8>, name: &str, local_only: bool) -> Result<Offer> {
        let endpoint = bind(local_only).await?;
        let (tx, rx) = mpsc::channel(1);
        let router = Router::builder(endpoint.clone()).accept(ALPN, Handler { ours: std::sync::Arc::new(ours), got: tx.clone() }).spawn();
        if !local_only {
            let _ = tokio::time::timeout(Duration::from_secs(8), endpoint.online()).await;
        }
        let relay = endpoint.addr().relay_urls().next().cloned();
        let link = Invite { id: endpoint.id(), relay, name: name.into() }.to_link().replacen("harp://party/", "harp://sync/", 1);
        Ok(Offer { endpoint, router, got: Mutex::new(rx), cancel: tx, link })
    }

    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Waits (up to `timeout`) for the other device; returns its library.
    pub async fn wait(&self, timeout: Duration) -> Result<Vec<u8>> {
        let mut rx = self.got.lock().await;
        let r = tokio::time::timeout(timeout, rx.recv()).await.context("no device joined in time")?;
        r.context("the offer was closed")?
    }

    /// Stops a `wait` in progress (it returns an error).
    pub fn cancel(&self) {
        let _ = self.cancel.try_send(Err(anyhow::anyhow!("cancelled")));
    }

    pub async fn close(self) {
        let _ = self.router.shutdown().await;
        self.endpoint.close().await;
    }
}

async fn bind(local_only: bool) -> Result<Endpoint> {
    Ok(if local_only {
        Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled).bind().await?
    } else {
        Endpoint::builder(presets::N0).bind().await?
    })
}

/// Joins an offer: sends our library, returns theirs.
pub async fn join(link: &str, ours: &[u8]) -> Result<Vec<u8>> {
    let link = link.trim();
    let Some(rest) = link.strip_prefix("harp://sync/") else {
        if link.starts_with("harp://party/") {
            bail!("that's a party link; this needs a sync link (Settings › Sync on the other device)");
        }
        bail!("not a Harp sync link");
    };
    let inv = Invite::parse(&format!("harp://party/{rest}"))?;
    join_addr(inv_addr(&inv), ours, false).await
}

fn inv_addr(inv: &Invite) -> EndpointAddr {
    let a = EndpointAddr::new(inv.id);
    match &inv.relay {
        Some(r) => a.with_relay_url(r.clone()),
        None => a,
    }
}

/// Joins at a known address (tests).
pub async fn join_addr(addr: EndpointAddr, ours: &[u8], local_only: bool) -> Result<Vec<u8>> {
    let endpoint = bind(local_only).await?;
    let r = async {
        let conn = tokio::time::timeout(Duration::from_secs(20), endpoint.connect(addr, ALPN)).await.context("the other device didn't answer in time")??;
        let (mut send, mut recv) = conn.open_bi().await?;
        send_blob(&mut send, ours).await?;
        send.finish()?;
        let theirs = recv_blob(&mut recv).await?;
        conn.close(0u32.into(), b"done");
        Ok::<_, anyhow::Error>(theirs)
    }
    .await;
    endpoint.close().await;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn both_sides_get_the_other_library() {
        let a: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let b = b"the other library".to_vec();
        let offer = Offer::start(a.clone(), "Laptop", true).await.unwrap();
        assert!(offer.link.starts_with("harp://sync/"));
        let addr = offer.addr();
        let joiner = tokio::spawn({
            let b = b.clone();
            async move { join_addr(addr, &b, true).await }
        });
        let got_b = offer.wait(Duration::from_secs(10)).await.unwrap();
        let got_a = joiner.await.unwrap().unwrap();
        assert_eq!(got_b, b);
        assert_eq!(got_a, a, "a bigger library arrives whole");
        offer.close().await;
    }

    #[tokio::test]
    async fn waiting_can_be_cancelled() {
        let offer = std::sync::Arc::new(Offer::start(vec![1], "x", true).await.unwrap());
        let o = offer.clone();
        let w = tokio::spawn(async move { o.wait(Duration::from_secs(60)).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        offer.cancel();
        assert!(w.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn a_party_link_is_refused_with_a_hint() {
        let id = iroh::SecretKey::generate().public();
        let e = join(&format!("harp://party/{id}"), b"x").await.unwrap_err();
        assert!(e.to_string().contains("party link"), "{e}");
    }
}
