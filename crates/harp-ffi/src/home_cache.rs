//! The saved home-page recommendations (kept out of `api`, which is what the
//! bridge generator scans).

use harp_core::Song;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct HomeCache {
    pub ts: u64,
    pub recommended: Vec<Song>,
    pub similar_to: Option<Song>,
    pub similar: Vec<Song>,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub const HOME_TTL_SECS: u64 = 3 * 3600;
