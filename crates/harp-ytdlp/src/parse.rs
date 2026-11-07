//! Parsing yt-dlp's `-J` output.

use crate::{Error, Result};
use harp_core::{tidy::tidy, Song};
use serde_json::Value;
use std::collections::HashMap;

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty()).map(String::from)
}

/// One flat-playlist entry -> a Song (None when it has no id, or is a channel/playlist).
pub fn song_from_entry(e: &Value) -> Option<Song> {
    let id = s(e, "id")?;
    // Video ids are 11 characters; channel (UC...) and playlist (PL...) ids are longer.
    if id.len() != 11 {
        return None;
    }
    let raw_title = s(e, "title")?;
    let uploader = s(e, "artist")
        .or_else(|| e.get("artists").and_then(|a| a.get(0)).and_then(Value::as_str).map(String::from))
        .or_else(|| s(e, "channel"))
        .or_else(|| s(e, "uploader"))
        .unwrap_or_default();
    let (title, artist) = if s(e, "artist").is_some() || e.get("artists").is_some() {
        (harp_core::tidy::tidy(&raw_title, "").0, uploader.clone())
    } else {
        tidy(&raw_title, &uploader)
    };
    let thumbnail = s(e, "thumbnail").or_else(|| {
        e.get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|a| a.last())
            .and_then(|t| s(t, "url"))
    });
    Some(Song {
        id,
        title,
        artist: if artist.is_empty() { "Unknown".into() } else { artist },
        artist_id: s(e, "channel_id").or_else(|| s(e, "uploader_id")),
        album: s(e, "album"),
        duration_secs: e.get("duration").and_then(Value::as_f64).map(|d| d.round() as u32),
        thumbnail,
    })
}

/// `-J --flat-playlist` output -> songs.
pub fn songs_from_playlist(json: &str) -> Result<Vec<Song>> {
    let v: Value = serde_json::from_str(json).map_err(|e| Error::Parse(e.to_string()))?;
    let entries = v.get("entries").and_then(Value::as_array).ok_or_else(|| Error::Parse("no entries".into()))?;
    let mut seen = std::collections::HashSet::new();
    Ok(entries
        .iter()
        .filter_map(song_from_entry)
        .filter(|s| seen.insert(s.id.clone()))
        .collect())
}

/// A resolved audio stream.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamInfo {
    pub url: String,
    pub ext: Option<String>,
    pub abr: Option<f64>,
    pub headers: HashMap<String, String>,
    /// Unix time the URL stops working, from its `expire=` parameter.
    pub expires_at: Option<u64>,
    pub duration_secs: Option<u32>,
}

/// The `expire` query parameter of a googlevideo URL.
pub fn expiry_of(url: &str) -> Option<u64> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.strip_prefix("expire=")).and_then(|v| v.parse().ok())
}

/// `-J -f bestaudio` output for one video -> its stream.
pub fn stream_from_info(json: &str) -> Result<StreamInfo> {
    let v: Value = serde_json::from_str(json).map_err(|e| Error::Parse(e.to_string()))?;
    let url = s(&v, "url").ok_or_else(|| Error::Parse("no stream url".into()))?;
    let headers = v
        .get("http_headers")
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default();
    Ok(StreamInfo {
        expires_at: expiry_of(&url),
        ext: s(&v, "ext"),
        abr: v.get("abr").and_then(Value::as_f64),
        duration_secs: v.get("duration").and_then(Value::as_f64).map(|d| d.round() as u32),
        headers,
        url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../tests/fixtures/search.json");
    const INFO: &str = include_str!("../tests/fixtures/info.json");

    #[test]
    fn search_entries_become_tidy_songs() {
        let songs = songs_from_playlist(SEARCH).unwrap();
        assert_eq!(songs.len(), 3, "channel entry dropped, duplicate dropped");
        assert_eq!(songs[0].title, "Get Lucky");
        assert_eq!(songs[0].artist, "Daft Punk");
        assert_eq!(songs[0].duration_secs, Some(248));
        assert_eq!(songs[0].artist_id.as_deref(), Some("UC_x5XG1OV2P6uZZ5FSM9Ttw"));
        assert!(songs[0].thumbnail.as_deref().unwrap().starts_with("https://"));
        // Topic channel falls back to uploader name
        assert_eq!(songs[1].artist, "Daft Punk");
        // YouTube Music entry with explicit artist/album
        assert_eq!(songs[2].artist, "Pharrell Williams");
        assert_eq!(songs[2].album.as_deref(), Some("G I R L"));
    }

    #[test]
    fn real_ytsearch_shapes_keep_the_artist() {
        // Shapes taken from real `yt-dlp --flat-playlist -J ytsearch3:...` output.
        let json = r#"{"entries":[
          {"id":"5NV6Rdv1a3I","title":"Daft Punk - Get Lucky (Official Audio) ft. Pharrell Williams, Nile Rodgers","channel":"Daft Punk","channel_id":"UC_kRDKYrUlrbtrSiyu5Tflg","duration":249},
          {"id":"kTJczUoc26U","title":"The Kid LAROI, Justin Bieber - STAY (Official Video)","channel":"The Kid LAROI.","duration":158},
          {"id":"AAAAAAAAAAA","title":"Blinding Lights","channel":"The Weeknd - Topic","duration":200}]}"#;
        let s = songs_from_playlist(json).unwrap();
        assert_eq!(s[0].artist, "Daft Punk");
        assert_eq!(s[1].artist, "The Kid LAROI, Justin Bieber");
        assert_eq!(s[1].title, "STAY");
        assert_eq!(s[2].artist, "The Weeknd");
    }

    #[test]
    fn stream_parsed_with_expiry() {
        let st = stream_from_info(INFO).unwrap();
        assert_eq!(st.ext.as_deref(), Some("m4a"));
        assert_eq!(st.expires_at, Some(4102444800));
        assert_eq!(st.abr, Some(129.5));
        assert!(st.headers.contains_key("User-Agent"));
    }

    #[test]
    fn expiry_parsing() {
        assert_eq!(expiry_of("https://x/y?a=1&expire=42&b=2"), Some(42));
        assert_eq!(expiry_of("https://x/y"), None);
    }

    #[test]
    fn rejects_garbage() {
        assert!(songs_from_playlist("nope").is_err());
        assert!(stream_from_info("{}").is_err());
    }
}
