//! Lyrics from several free, keyless sources, best first:
//!
//! 1. **LRCLIB** (https://lrclib.net), synced: the open default.
//! 2. **NetEase Cloud Music**, synced: strong for Asian and Indian songs (often
//!    already in English letters for Hindi).
//! 3. LRCLIB plain text.
//! 4. **YouTube Music**'s own lyrics (licensed from LyricFind), plain text, often
//!    in the song's original script.

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

/// Looks lyrics up in every source, best first (see the top of this file).
/// `Ok(None)` means no source has any (worth remembering); `Err` means every
/// source failed to answer.
pub fn fetch(video_id: &str, title: &str, artist: &str, album: Option<&str>, duration: Option<u32>) -> Result<Option<Lyrics>> {
    // Synced lyrics end the search; plain ones are kept in case nothing better turns up.
    fn note(r: Result<Option<Lyrics>>, plain: &mut Option<Lyrics>, failed: &mut Option<Error>) -> Option<Lyrics> {
        match r {
            Ok(Some(l)) if l.synced => return Some(l),
            Ok(Some(l)) => {
                plain.get_or_insert(l);
            }
            Ok(None) => {}
            Err(e) => {
                failed.get_or_insert(e);
            }
        }
        None
    }
    let (mut plain, mut failed) = (None, None);
    if let Some(l) = note(lrclib(title, artist, album, duration), &mut plain, &mut failed) {
        return Ok(Some(l));
    }
    if let Some(l) = note(netease(title, artist, duration), &mut plain, &mut failed) {
        return Ok(Some(l));
    }
    if plain.is_none() && !video_id.is_empty() && !harp_core::local::is_local(video_id) {
        let yt = crate::ytmusic::lyrics(video_id).map(|o| {
            o.map(|(text, credit)| {
                let src = if credit.is_empty() { "YouTube Music".to_string() } else { format!("YouTube Music ({credit})") };
                from_texts(None, Some(&text), &src)
            })
            .filter(|l| !l.is_empty())
        });
        note(yt, &mut plain, &mut failed);
    }
    match (plain, failed) {
        (Some(l), _) => Ok(Some(l)),
        // Nothing found, but a source couldn't be asked: try again later rather
        // than remembering "no lyrics".
        (None, Some(e)) => Err(e),
        (None, None) => Ok(None),
    }
}

/// LRCLIB alone.
pub fn lrclib(title: &str, artist: &str, album: Option<&str>, duration: Option<u32>) -> Result<Option<Lyrics>> {
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

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The NetEase song that matches: same title (or one containing the other),
/// one of its artists in ours, and a length within 6 s when we know ours.
pub fn netease_pick(v: &Value, title: &str, artist: &str, duration: Option<u32>) -> Option<i64> {
    let (t, a) = (norm(title), norm(artist));
    if t.is_empty() {
        return None;
    }
    let mut best: Option<(i64, i64)> = None;
    for s in v.pointer("/result/songs")?.as_array()? {
        let name = norm(s.get("name").and_then(Value::as_str).unwrap_or(""));
        if name.is_empty() || !(name == t || name.contains(&t) || t.contains(&name)) {
            continue;
        }
        let artist_ok = s.get("artists").and_then(Value::as_array).is_some_and(|l| {
            l.iter().filter_map(|x| x.get("name").and_then(Value::as_str)).map(norm).any(|n| !n.is_empty() && (a.contains(&n) || n.contains(&a)))
        });
        if !artist_ok {
            continue;
        }
        let mut score = if name == t { 100 } else { 50 };
        if let (Some(want), Some(ms)) = (duration, s.get("duration").and_then(Value::as_i64)) {
            let d = (want as i64 - ms / 1000).abs();
            if d > 6 {
                continue;
            }
            score -= d;
        }
        let id = s.get("id").and_then(Value::as_i64)?;
        if best.map_or(true, |(b, _)| score > b) {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}

/// Synced lyrics from a NetEase lyric response, without the credit lines
/// ("作词 : …", "作曲 : …") it puts first, and nothing for instrumentals.
pub fn netease_lrc(v: &Value) -> Option<Lyrics> {
    let raw = v.pointer("/lrc/lyric").and_then(Value::as_str)?;
    if raw.contains("纯音乐") {
        return None;
    }
    let cjk = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
    let kept: Vec<&str> = raw
        .lines()
        .filter(|l| {
            let text = l.rsplit(']').next().unwrap_or("");
            !((text.contains(':') || text.contains('：')) && text.chars().any(cjk))
        })
        .collect();
    let l = from_texts(Some(&kept.join("\n")), None, "NetEase");
    (l.synced && l.lines.iter().any(|x| !x.text.trim().is_empty())).then_some(l)
}

/// NetEase Cloud Music: search, then the matching song's lyrics.
pub fn netease(title: &str, artist: &str, duration: Option<u32>) -> Result<Option<Lyrics>> {
    let (t, a) = clean(title, artist);
    let url = format!("https://music.163.com/api/search/get?s={}&type=1&limit=8", enc(&format!("{t} {a}")));
    let Some(v) = get(&url)? else { return Ok(None) };
    let Some(id) = netease_pick(&v, &t, &a, duration) else { return Ok(None) };
    Ok(get(&format!("https://music.163.com/api/song/lyric?id={id}&lv=1"))?.and_then(|v| netease_lrc(&v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn netease_match_and_credits() {
        let v: Value = serde_json::from_str(include_str!("../tests/fixtures/netease_search.json")).unwrap();
        assert_eq!(netease_pick(&v, "Tum Hi Ho", "Arijit Singh", Some(262)), Some(1872590764));
        assert_eq!(netease_pick(&v, "Tum Hi Ho", "Arijit Singh", Some(200)), None, "length too far");
        assert_eq!(netease_pick(&v, "Tum Hi Ho", "Someone Else", None), None);
        let l = netease_lrc(&serde_json::from_str(include_str!("../tests/fixtures/netease_lyric.json")).unwrap()).unwrap();
        assert!(l.synced);
        assert_eq!(l.source, "NetEase");
        assert!(l.lines.iter().all(|x| !x.text.contains("作曲")), "credit lines dropped");
        assert_eq!(l.lines.iter().find(|x| !x.text.is_empty()).unwrap().text, "Hum tere bin ab reh nahi sakte");
        assert!(netease_lrc(&json!({"lrc": {"lyric": "[00:00.00]纯音乐，请欣赏"}})).is_none());
    }

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
        let l = fetch("", "Get Lucky", "Daft Punk", Some("Random Access Memories"), Some(248)).unwrap().unwrap();
        assert!(l.synced && l.lines.len() > 20);
        // Each source on its own, on a Hindi song.
        let n = netease("Tum Hi Ho", "Arijit Singh", Some(262)).unwrap().unwrap();
        assert!(n.synced && n.lines.len() > 10, "netease");
        let (text, _) = crate::ytmusic::lyrics("Umqb9KENgmk").unwrap().unwrap();
        assert!(text.contains("तुम ही हो"), "youtube music");
    }
}
