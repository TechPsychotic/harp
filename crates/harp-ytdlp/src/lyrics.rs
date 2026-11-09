//! Lyrics from LRCLIB (https://lrclib.net): free, no account, no key. Synced
//! lyrics come as LRC text.

use crate::{Error, Result};
use harp_core::lyrics::{from_texts, Lyrics};
use serde_json::Value;
use std::time::Duration;

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

fn get(url: &str) -> Result<Option<Value>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .header("User-Agent", "Harp (music player; https://github.com/TechPsychotic/harp)")
        .call()
        .map_err(|e| Error::Download(e.to_string()))?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(Error::Download(format!("lrclib: HTTP {}", resp.status())));
    }
    let text = resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))?;
    serde_json::from_str(&text).map(Some).map_err(|e| Error::Parse(e.to_string()))
}

/// First artist of "A, B & C", and a title without "(feat. ...)" or "(From ...)" noise.
fn clean(title: &str, artist: &str) -> (String, String) {
    let (t, _) = harp_core::tidy::tidy(title, "");
    let t = t.split(" (feat").next().unwrap_or(&t).split(" (From").next().unwrap_or(&t).trim().to_string();
    let a = artist.split([',', '&']).next().unwrap_or(artist).trim().to_string();
    (t, a)
}

/// Picks the best entry: synced beats plain, and a length close to the song's beats a far one.
pub fn pick(v: &Value, duration: Option<u32>) -> Option<Lyrics> {
    let entries: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![v],
        _ => vec![],
    };
    let mut best: Option<(i64, &Value)> = None;
    for e in entries {
        if e.get("instrumental").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let synced = e.get("syncedLyrics").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
        let plain = e.get("plainLyrics").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
        if !synced && !plain {
            continue;
        }
        let mut score = if synced { 1000 } else { 0 };
        if let (Some(want), Some(have)) = (duration, e.get("duration").and_then(Value::as_f64)) {
            let d = (want as f64 - have).abs();
            score -= if d <= 3.0 { 0 } else if d <= 8.0 { 200 } else { 700 + d as i64 };
        }
        if best.map_or(true, |(s, _)| score > s) {
            best = Some((score, e));
        }
    }
    let (_, e) = best?;
    let l = from_texts(
        e.get("syncedLyrics").and_then(Value::as_str),
        e.get("plainLyrics").and_then(Value::as_str),
        "LRCLIB",
    );
    (!l.is_empty()).then_some(l)
}

/// Looks lyrics up. `Ok(None)` means none exist (worth remembering); `Err` is a failed request.
pub fn fetch(title: &str, artist: &str, album: Option<&str>, duration: Option<u32>) -> Result<Option<Lyrics>> {
    let (t, a) = clean(title, artist);
    // Exact lookup first (needs the length).
    if let Some(d) = duration {
        let mut url = format!("https://lrclib.net/api/get?track_name={}&artist_name={}&duration={d}", enc(&t), enc(&a));
        if let Some(al) = album.filter(|x| !x.is_empty()) {
            url.push_str(&format!("&album_name={}", enc(al)));
        }
        if let Some(v) = get(&url)? {
            if let Some(l) = pick(&v, duration) {
                return Ok(Some(l));
            }
        }
    }
    let url = format!("https://lrclib.net/api/search?track_name={}&artist_name={}", enc(&t), enc(&a));
    Ok(get(&url)?.and_then(|v| pick(&v, duration)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefers_synced_and_a_matching_length() {
        let v = json!([
            {"duration": 300.0, "syncedLyrics": "[00:01.00]long version"},
            {"duration": 248.0, "syncedLyrics": null, "plainLyrics": "plain only"},
            {"duration": 249.0, "syncedLyrics": "[00:02.00]right one"},
            {"instrumental": true, "duration": 248.0, "syncedLyrics": "[00:00.00]x"}
        ]);
        let l = pick(&v, Some(248)).unwrap();
        assert!(l.synced);
        assert_eq!(l.lines[0].text, "right one");
        assert_eq!(l.source, "LRCLIB");
    }

    #[test]
    fn plain_when_nothing_is_synced_and_none_when_empty() {
        let v = json!({"duration": 200.0, "syncedLyrics": null, "plainLyrics": "a\nb"});
        let l = pick(&v, Some(200)).unwrap();
        assert!(!l.synced && l.lines.len() == 2);
        assert!(pick(&json!({"instrumental": true}), None).is_none());
        assert!(pick(&json!([]), None).is_none());
        assert!(pick(&json!("x"), None).is_none());
    }

    #[test]
    fn cleans_titles_and_artists_for_lookup() {
        assert_eq!(clean("Get Lucky (feat. Pharrell Williams and Nile Rodgers)", "Daft Punk, Pharrell Williams & Nile Rodgers"), ("Get Lucky".into(), "Daft Punk".into()));
        assert_eq!(clean("Tum Hi Ho (From \"Aashiqui 2\")", "Arijit Singh"), ("Tum Hi Ho".into(), "Arijit Singh".into()));
        assert_eq!(enc("a b&c"), "a%20b%26c");
    }

    /// `cargo test -p harp-ytdlp -- --ignored live_lyrics`
    #[test]
    #[ignore]
    fn live_lyrics() {
        let l = fetch("Get Lucky", "Daft Punk", Some("Random Access Memories"), Some(248)).unwrap().unwrap();
        assert!(l.synced && l.lines.len() > 20);
    }
}
