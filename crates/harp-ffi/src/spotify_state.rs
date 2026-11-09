//! The Spotify sign-in in progress (kept out of `api`, which the bridge generator scans).

use harp_ytdlp::spotify;
use std::sync::Mutex;

pub struct PendingSignIn {
    pub client_id: String,
    pub verifier: String,
    pub state: String,
    pub callback: spotify::Callback,
}

pub static SPOTIFY_PENDING: Mutex<Option<PendingSignIn>> = Mutex::new(None);
pub static SPOTIFY_CANCEL: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = Mutex::new(None);
