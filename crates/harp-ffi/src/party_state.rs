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
