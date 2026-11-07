use serde::{Deserialize, Serialize};

/// A song: what it is, plus the YouTube id it plays from (more sources later).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// Video id on the source.
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Channel / artist id when the source gives one.
    pub artist_id: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub thumbnail: Option<String>,
}

impl Song {
    /// Stable key for taste scoring: the artist id, else the lowercased name.
    pub fn artist_key(&self) -> String {
        match &self.artist_id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => self.artist.trim().to_lowercase(),
        }
    }

    pub fn thumbnail_or_default(&self) -> String {
        self.thumbnail
            .clone()
            .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", self.id))
    }
}

#[cfg(test)]
pub(crate) fn song(id: &str, title: &str, artist: &str) -> Song {
    Song {
        id: id.into(),
        title: title.into(),
        artist: artist.into(),
        artist_id: Some(format!("ch-{}", artist.to_lowercase())),
        album: None,
        duration_secs: Some(200),
        thumbnail: None,
    }
}
