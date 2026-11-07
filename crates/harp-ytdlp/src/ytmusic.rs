//! YouTube Music's public web API (the same calls the `ytmusicapi` library
//! makes, with no sign-in and no key), in Rust so it works on every platform
//! without Python. It gives what yt-dlp's flat listings lack: artists, album,
//! length and cover art, and the same "song radio" YouTube Music plays next.
//!
//! The JSON is deeply nested and changes shape now and then, so parsing finds
//! renderers by name instead of by path.

use crate::{Error, Result};
use harp_core::Song;
use serde_json::{json, Value};
use std::time::Duration;

const BASE: &str = "https://music.youtube.com/youtubei/v1";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";
/// The "Songs" filter of YouTube Music's search page.
const SONGS_FILTER: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";

fn context() -> Value {
    json!({"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20241127.01.00", "hl": "en", "gl": "US"}})
}

fn post(endpoint: &str, mut body: Value) -> Result<Value> {
    body["context"] = context();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut resp = agent
        .post(&format!("{BASE}/{endpoint}?prettyPrint=false"))
        .header("Content-Type", "application/json")
        .header("Origin", "https://music.youtube.com")
        .header("User-Agent", UA)
        .send(body.to_string())
        .map_err(|e| Error::Download(e.to_string()))?;
    let text = resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()))
}

/// Songs matching a query.
pub fn search(query: &str, limit: usize) -> Result<Vec<Song>> {
    let v = post("search", json!({"query": query, "params": SONGS_FILTER}))?;
    let mut songs = parse_search(&v);
    songs.truncate(limit);
    Ok(songs)
}

/// The song radio for a video: what YouTube Music plays after it.
pub fn radio(video_id: &str, limit: usize) -> Result<Vec<Song>> {
    let v = post(
        "next",
        json!({
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        }),
    )?;
    let mut songs = parse_radio(&v);
    songs.retain(|s| s.id != video_id);
    songs.truncate(limit);
    Ok(songs)
}

/// Every value stored under `key`, anywhere in the tree.
fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == key {
                    out.push(x);
                } else {
                    find_all(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, out)),
        _ => {}
    }
}

fn text(v: &Value) -> Option<&str> {
    v.get("text").and_then(Value::as_str)
}

/// "6:10" or "1:02:03" -> seconds.
fn parse_clock(s: &str) -> Option<u32> {
    let mut total = 0u32;
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    for p in parts {
        total = total.checked_mul(60)?.checked_add(p.trim().parse::<u32>().ok()?)?;
    }
    Some(total)
}

struct Byline {
    artist: String,
    artist_id: Option<String>,
    album: Option<String>,
    secs: Option<u32>,
}

/// Parses "Artist, Artist & Artist • Album • 6:10" (runs, split on " • ").
fn byline(runs: &[Value]) -> Byline {
    let mut groups: Vec<Vec<&Value>> = vec![vec![]];
    for r in runs {
        if text(r) == Some(" • ") {
            groups.push(vec![]);
        } else {
            groups.last_mut().unwrap().push(r);
        }
    }
    let join = |g: &Vec<&Value>| g.iter().filter_map(|r| text(r)).collect::<String>();
    let artist = groups.first().map(join).unwrap_or_default();
    let artist_id = groups
        .first()
        .and_then(|g| g.iter().find_map(|r| r.pointer("/navigationEndpoint/browseEndpoint/browseId")))
        .and_then(Value::as_str)
        .map(String::from);
    // The last group that looks like a clock is the length; an album is a
    // group linking to an MPRE... page.
    let secs = groups.iter().skip(1).rev().find_map(|g| parse_clock(&join(g)));
    let album = groups.iter().skip(1).find_map(|g| {
        let is_album = g.iter().any(|r| {
            r.pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(Value::as_str)
                .map_or(false, |b| b.starts_with("MPRE"))
        });
        is_album.then(|| join(g))
    });
    Byline { artist, artist_id, album, secs }
}

/// A larger version of a Google image URL (they end in `=w60-h60-...`).
fn big_cover(url: &str) -> String {
    match url.rsplit_once('=') {
        Some((base, size)) if size.starts_with('w') && size.contains("-h") => format!("{base}=w544-h544-l90-rj"),
        _ => url.to_string(),
    }
}

fn thumb(v: &Value) -> Option<String> {
    let mut found = vec![];
    find_all(v, "thumbnails", &mut found);
    found
        .first()
        .and_then(|t| t.as_array())
        .and_then(|a| a.last())
        .and_then(|t| t.get("url"))
        .and_then(Value::as_str)
        .map(big_cover)
}

pub fn parse_search(v: &Value) -> Vec<Song> {
    let mut items = vec![];
    find_all(v, "musicResponsiveListItemRenderer", &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter_map(|it| {
            let id = it.pointer("/playlistItemData/videoId").and_then(Value::as_str)?;
            let cols = it.get("flexColumns")?.as_array()?;
            let col = |i: usize| {
                cols.get(i)
                    .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
                    .and_then(Value::as_array)
            };
            let title = col(0)?.first().and_then(text)?.to_string();
            let b = byline(col(1)?);
            if b.artist.is_empty() || !seen.insert(id.to_string()) {
                return None;
            }
            Some(Song {
                id: id.to_string(),
                title,
                artist: b.artist,
                artist_id: b.artist_id,
                album: b.album,
                duration_secs: b.secs,
                thumbnail: thumb(it),
            })
        })
        .collect()
}

pub fn parse_radio(v: &Value) -> Vec<Song> {
    let mut items = vec![];
    find_all(v, "playlistPanelVideoRenderer", &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter_map(|it| {
            let id = it.get("videoId").and_then(Value::as_str)?;
            let title = it.pointer("/title/runs/0/text").and_then(Value::as_str)?.to_string();
            let runs = it.pointer("/longBylineText/runs").and_then(Value::as_array)?;
            let b = byline(runs);
            let secs = it
                .pointer("/lengthText/runs/0/text")
                .and_then(Value::as_str)
                .and_then(parse_clock)
                .or(b.secs);
            if b.artist.is_empty() || !seen.insert(id.to_string()) {
                return None;
            }
            Some(Song {
                id: id.to_string(),
                title,
                artist: b.artist,
                artist_id: b.artist_id,
                album: b.album,
                duration_secs: secs,
                thumbnail: thumb(it),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn search_has_artists_album_length_and_cover() {
        let songs = parse_search(&fixture(include_str!("../tests/fixtures/ytm_search.json")));
        assert_eq!(songs.len(), 5, "the row without a video id is skipped");
        let s = &songs[0];
        assert_eq!(s.id, "4D7u5KF7SP8");
        assert_eq!(s.title, "Get Lucky (feat. Pharrell Williams and Nile Rodgers)");
        assert_eq!(s.artist, "Daft Punk, Pharrell Williams & Nile Rodgers");
        assert_eq!(s.artist_id.as_deref(), Some("UCRr1xG_2WIDs18a6cIiCxeA"));
        assert_eq!(s.album.as_deref(), Some("Random Access Memories"));
        assert_eq!(s.duration_secs, Some(370));
        assert!(s.thumbnail.as_deref().unwrap().contains("w544-h544"));
        assert_eq!(songs[2].artist, "Los del Fuego");
    }

    #[test]
    fn radio_parses_and_skips_the_seed() {
        let songs = parse_radio(&fixture(include_str!("../tests/fixtures/ytm_next.json")));
        assert_eq!(songs.len(), 5);
        assert_eq!(songs[1].artist, "The Weeknd");
        assert_eq!(songs[1].album.as_deref(), Some("Starboy"));
        assert_eq!(songs[1].duration_secs, Some(270));
    }

    #[test]
    fn clocks_and_covers() {
        assert_eq!(parse_clock("6:10"), Some(370));
        assert_eq!(parse_clock("1:02:03"), Some(3723));
        assert_eq!(parse_clock("1.9B plays"), None);
        assert_eq!(parse_clock("2024"), None);
        assert_eq!(big_cover("https://x/y=w60-h60-l90-rj"), "https://x/y=w544-h544-l90-rj");
        assert_eq!(big_cover("https://i.ytimg.com/vi/a/hq.jpg"), "https://i.ytimg.com/vi/a/hq.jpg");
    }

    #[test]
    fn unknown_shapes_give_nothing_not_a_panic() {
        assert!(parse_search(&json!({"contents": [1, 2, {"a": null}]})).is_empty());
        assert!(parse_radio(&json!("nope")).is_empty());
    }
}

/// Live check against the real service. Run with
/// `cargo test -p harp-ytdlp -- --ignored live`.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_search_and_radio() {
        let s = search("daft punk get lucky", 5).unwrap();
        assert!(s.iter().any(|x| x.artist.contains("Daft Punk")), "{s:?}");
        let r = radio(&s[0].id, 10).unwrap();
        assert!(!r.is_empty());
    }
}
