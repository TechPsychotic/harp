//! Sharing a playlist with friends, peer to peer over iroh (like Sync): the
//! sharer shows a `harp://playlist/…` link; whoever opens it gets a copy of the
//! playlist in their own Harp. The sharer serves it to everyone who asks while
//! sharing is on. The bytes are the app's playlist format; this only carries them.

use std::{sync::Arc, time::Duration};

use anyhow::{bail, ensure, Context, Result};
use iroh::{
    endpoint::{presets, Connection},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr,
};

use crate::Invite;

pub const ALPN: &[u8] = b"harp/playlist/1";
const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
struct Handler(Arc<Vec<u8>>);

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("share")
    }
}

impl ProtocolHandler for Handler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let r = async {
            let mut send = conn.open_uni().await?;
            send.write_all(&(self.0.len() as u64).to_be_bytes()).await?;
            send.write_all(&self.0).await?;
            send.finish()?;
            let _ = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let _ = r;
        Ok(())
    }
}

/// A playlist being shared. Dropping (or [`Share::close`]) stops sharing.
pub struct Share {
    endpoint: Endpoint,
    router: Router,
    pub link: String,
}

impl Share {
    /// Starts sharing `data` (a playlist) named `name`. `local_only`: tests.
    pub async fn start(data: Vec<u8>, name: &str, local_only: bool) -> Result<Share> {
        ensure!(data.len() as u64 <= MAX_BYTES, "playlist too big to share");
        let endpoint = bind(local_only).await?;
        let router = Router::builder(endpoint.clone()).accept(ALPN, Handler(Arc::new(data))).spawn();
        if !local_only {
            let _ = tokio::time::timeout(Duration::from_secs(8), endpoint.online()).await;
        }
        let relay = endpoint.addr().relay_urls().next().cloned();
        let link = Invite { id: endpoint.id(), relay, name: name.into() }.to_link().replacen("harp://party/", "harp://playlist/", 1);
        Ok(Share { endpoint, router, link })
    }

    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
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

/// Fetches a shared playlist from its link.
pub async fn fetch(link: &str) -> Result<Vec<u8>> {
    let link = link.trim();
    let Some(rest) = link.strip_prefix("harp://playlist/") else { bail!("not a Harp playlist link") };
    let inv = Invite::parse(&format!("harp://party/{rest}"))?;
    let mut addr = EndpointAddr::new(inv.id);
    if let Some(r) = inv.relay {
        addr = addr.with_relay_url(r);
    }
    fetch_addr(addr, false).await
}

/// Fetches from a known address (tests).
pub async fn fetch_addr(addr: EndpointAddr, local_only: bool) -> Result<Vec<u8>> {
    let endpoint = bind(local_only).await?;
    let r = async {
        let conn = tokio::time::timeout(Duration::from_secs(20), endpoint.connect(addr, ALPN))
            .await
            .context("the person sharing isn't reachable (sharing may have stopped)")??;
        let mut recv = conn.accept_uni().await?;
        let mut len = [0u8; 8];
        recv.read_exact(&mut len).await?;
        let n = u64::from_be_bytes(len);
        ensure!(n <= MAX_BYTES, "playlist too big");
        let mut buf = vec![0u8; n as usize];
        recv.read_exact(&mut buf).await?;
        conn.close(0u32.into(), b"thanks");
        Ok::<_, anyhow::Error>(buf)
    }
    .await;
    endpoint.close().await;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn several_friends_can_fetch_it() {
        let data = br#"{"name":"Road trip","songs":[]}"#.to_vec();
        let share = Share::start(data.clone(), "Road trip", true).await.unwrap();
        assert!(share.link.starts_with("harp://playlist/"));
        for _ in 0..3 {
            assert_eq!(fetch_addr(share.addr(), true).await.unwrap(), data);
        }
        share.close().await;
    }

    #[tokio::test]
    async fn other_links_are_refused() {
        assert!(fetch("harp://sync/abc").await.is_err());
    }
}
