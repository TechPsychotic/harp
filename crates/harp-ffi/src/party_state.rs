//! The party's runtime and connection (kept out of `api`, which the bridge
//! generator scans).

use std::sync::{Mutex, OnceLock};

use harp_party::{Event, Party};
use tokio::runtime::Runtime;

/// A small runtime for the network side (iroh needs tokio).
pub fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("harp-party").enable_all().build().expect("tokio runtime"))
}

/// The party (made on first use: it opens the network endpoint).
pub static PARTY: Mutex<Option<std::sync::Arc<Party>>> = Mutex::new(None);

/// Where events go (the app's listener), set by `party_events`.
pub static SINK: Mutex<Option<Box<dyn Fn(Event) + Send>>> = Mutex::new(None);

/// The party, starting it if needed; its events are forwarded to the sink.
pub fn party(name: &str) -> anyhow::Result<std::sync::Arc<Party>> {
    if let Some(p) = PARTY.lock().unwrap().as_ref() {
        p.set_name(name);
        return Ok(p.clone());
    }
    let (p, mut rx) = rt().block_on(Party::bind(name, false))?;
    let p = std::sync::Arc::new(p);
    *PARTY.lock().unwrap() = Some(p.clone());
    rt().spawn(async move {
        while let Some(e) = rx.recv().await {
            if let Some(f) = SINK.lock().unwrap().as_ref() {
                f(e);
            }
        }
    });
    Ok(p)
}

// --- this device's lasting network node (pairing, auto sync, shared playlists) ---

use harp_party::node::{Hooks, Node};

pub static NODE: Mutex<Option<std::sync::Arc<Node>>> = Mutex::new(None);
/// What the last sync added (the merge happens inside the node's hook).
pub static LAST_MERGE: Mutex<Option<crate::api::engine::MergeInfoView>> = Mutex::new(None);
/// A pairing offer waiting for the other device.
pub static PAIRING: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<anyhow::Result<String>>>> = tokio::sync::Mutex::const_new(None);

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Paired {
    pub id: String,
    pub name: String,
}

/// token → (playlist id, link)
pub type Shares = std::collections::HashMap<String, (i64, String)>;

pub fn kv_get(k: &str) -> Option<String> {
    crate::api::engine::setting_get(k.into()).ok().flatten()
}
pub fn kv_set(k: &str, v: &str) {
    let _ = crate::api::engine::setting_set(k.into(), v.into());
}

pub fn paired() -> Vec<Paired> {
    kv_get("paired").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}
pub fn set_paired(v: &[Paired]) {
    kv_set("paired", &serde_json::to_string(v).unwrap_or_default());
}
pub fn shares() -> Shares {
    kv_get("shares").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}
pub fn set_shares(s: &Shares) {
    kv_set("shares", &serde_json::to_string(s).unwrap_or_default());
}

pub fn device_name() -> String {
    kv_get("party_name").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| if cfg!(target_os = "android") { "My phone".into() } else { "My computer".into() })
}

/// The node, started on first use (it opens the network endpoint).
pub fn node() -> anyhow::Result<std::sync::Arc<Node>> {
    if let Some(n) = NODE.lock().unwrap().as_ref() {
        n.set_name(&device_name());
        return Ok(n.clone());
    }
    // This device's lasting key.
    let key: [u8; 32] = match kv_get("node_key").and_then(|h| (0..32).map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok()).collect::<Option<Vec<u8>>>()) {
        Some(v) if v.len() == 32 => v.try_into().unwrap(),
        _ => {
            let k = iroh_key();
            kv_set("node_key", &k.iter().map(|b| format!("{b:02x}")).collect::<String>());
            k
        }
    };
    let ids: Vec<_> = paired().iter().filter_map(|p| p.id.parse().ok()).collect();
    let hooks = Hooks {
        library: Box::new(|| crate::api::engine::library_backup().map_err(|e| anyhow::anyhow!(e))),
        merge: Box::new(|bytes| {
            let m = crate::api::engine::library_restore(bytes).map_err(|e| anyhow::anyhow!(e))?;
            *LAST_MERGE.lock().unwrap() = Some(m);
            Ok(())
        }),
        playlist: Box::new(|token| {
            let (id, _) = shares().get(token)?.clone();
            let (name, songs) = crate::api::engine::playlist_contents(id).ok()?;
            serde_json::to_vec(&crate::api::party::SharedPlaylist { name, songs }).ok()
        }),
        auto_sync: Box::new(|| kv_get("auto_sync").as_deref() != Some("0")),
        paired: Box::new(|id, name| {
            let mut v = paired();
            v.retain(|p| p.id != id.to_string());
            v.push(Paired { id: id.to_string(), name });
            set_paired(&v);
        }),
    };
    let n = std::sync::Arc::new(rt().block_on(Node::start(key, &device_name(), ids, hooks, false))?);
    *NODE.lock().unwrap() = Some(n.clone());
    Ok(n)
}

fn iroh_key() -> [u8; 32] {
    iroh::SecretKey::generate().to_bytes()
}

/// A short random token (for pairing codes and share links).
pub fn token() -> String {
    iroh::SecretKey::generate().public().to_string()[..16].to_string()
}
