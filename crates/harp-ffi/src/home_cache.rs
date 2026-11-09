//! The saved home-page recommendations (kept out of `api`, which is what the
//! bridge generator scans).

use harp_core::Song;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct HomeCache {
    pub ts: u64,
    pub recommended: Vec<Song>,
    pub similar_to: Option<Song>,
    pub similar: Vec<Song>,
    /// Artists you play most, with their pictures (older caches have none).
    #[serde(default)]
    pub artists: Vec<CachedCollection>,
    /// Albums by those artists.
    #[serde(default)]
    pub albums: Vec<CachedCollection>,
}

/// An artist or album as the home page keeps it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct CachedCollection {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
    pub kind: String,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub const HOME_TTL_SECS: u64 = 3 * 3600;
