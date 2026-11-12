//! Harp's own stream finder: one request to the source's player API with a
//! client that returns plain audio links (no player code to download or run,
//! no Python). Used first everywhere, and the only way on iOS, where apps can't
//! start other programs such as yt-dlp. yt-dlp stays the fallback elsewhere.

use crate::parse::StreamInfo;
use crate::extractor::{is_bot_check, Quality};
use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ureq::config::IpFamily;

const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

fn body(id: &str) -> Value {
    json!({
        "context": {"client": {
            "clientName": "VISIONOS", "clientVersion": "1.02",
            "deviceMake": "Apple", "deviceModel": "RealityDevice17,1",
            "userAgent": UA, "osName": "visionOS", "osVersion": "26.5.23O471",
            "hl": "en", "gl": "US"
        }},
        "videoId": id,
        "playbackContext": {"contentPlaybackContext": {"html5Preference": "HTML5_PREF_WANTS"}},
        "contentCheckOk": true,
        "racyCheckOk": true
    })
}

/// 0: either address family; 4 or 6: the one that worked after the other was refused.
static FAMILY: AtomicU8 = AtomicU8::new(0);

fn agent(family: u8, timeout: Duration) -> ureq::Agent {
    let f = match family {
        4 => IpFamily::Ipv4Only,
        6 => IpFamily::Ipv6Only,
        _ => IpFamily::Any,
    };
    ureq::Agent::config_builder().timeout_global(Some(timeout)).ip_family(f).build().into()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn player(id: &str, family: u8) -> Result<Value> {
    let mut r = agent(family, Duration::from_secs(20))
        .post(PLAYER)
        .header("Content-Type", "application/json")
        .header("User-Agent", UA)
        .header("X-YouTube-Client-Name", "101")
        .header("X-YouTube-Client-Version", "1.02")
        .header("Origin", "https://www.youtube.com")
        .send(body(id).to_string())
        .map_err(|e| Error::Download(e.to_string()))?;
    let text = r.body_mut().with_config().limit(4 * 1024 * 1024).read_to_string().map_err(|e| Error::Download(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()))
}

/// Itags by preference for each quality (AAC 140/139, Opus 251/250/249).
fn wanted(q: Quality) -> &'static [u64] {
    match q {
        Quality::High => &[251, 140, 250, 249, 139],
        Quality::Medium => &[140, 250, 251, 249, 139],
        Quality::Low => &[249, 139, 250, 140, 251],
    }
}

/// The best audio stream in a player response, or why there is none.
pub fn pick(resp: &Value, q: Quality) -> Result<StreamInfo> {
    let ps = &resp["playabilityStatus"];
    let status = ps["status"].as_str().unwrap_or("");
    if status != "OK" {
        let reason = ps["reason"].as_str().or_else(|| ps["messages"][0].as_str()).unwrap_or("not playable");
        return Err(Error::Failed(format!("{status}: {reason}")));
    }
    let sd = &resp["streamingData"];
    let formats: Vec<&Value> = sd["adaptiveFormats"]
        .as_array()
        .map(|a| a.iter().filter(|f| f["mimeType"].as_str().is_some_and(|m| m.starts_with("audio/")) && f["url"].is_string()).collect())
        .unwrap_or_default();
    let chosen = wanted(q)
        .iter()
        .find_map(|it| formats.iter().find(|f| f["itag"].as_u64() == Some(*it)))
        .or_else(|| formats.first())
        .ok_or_else(|| Error::Failed("no audio stream in the answer".into()))?;
    let url = chosen["url"].as_str().unwrap_or_default().to_string();
    let mime = chosen["mimeType"].as_str().unwrap_or("");
    let ext = if mime.contains("webm") { "webm" } else { "m4a" };
    let expires_at = sd["expiresInSeconds"].as_str().and_then(|s| s.parse::<u64>().ok()).map(|s| now() + s).or_else(|| crate::parse::expiry_of(&url));
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), UA.to_string());
    Ok(StreamInfo {
        url,
        ext: Some(ext.into()),
        abr: chosen["bitrate"].as_f64().map(|b| b / 1000.0),
        headers,
        expires_at,
        duration_secs: resp["videoDetails"]["lengthSeconds"].as_str().and_then(|s| s.parse().ok()),
    })
}

/// A stream for a song. If the source refuses this address, asks again over
/// the other address family, and keeps using the one that works.
pub fn resolve(id: &str, q: Quality) -> Result<StreamInfo> {
    let fam = FAMILY.load(Ordering::Relaxed);
    let first = player(id, fam).and_then(|r| pick(&r, q));
    match first {
        Err(e) if is_bot_check(&e.to_string()) => {
            for other in [6u8, 4u8] {
                if other == fam {
                    continue;
                }
                if let Ok(s) = player(id, other).and_then(|r| pick(&r, q)) {
                    FAMILY.store(other, Ordering::Relaxed);
                    return Ok(s);
                }
            }
            Err(e)
        }
        r => r,
    }
}

/// Downloads a song's audio into `dir` (in pieces, as the source prefers).
/// Returns the file and its size.
pub fn download(id: &str, dir: &Path, q: Quality) -> Result<(PathBuf, u64)> {
    let s = resolve(id, q)?;
    std::fs::create_dir_all(dir)?;
    let ext = s.ext.clone().unwrap_or_else(|| "m4a".into());
    let path = dir.join(format!("{id}.{ext}"));
    let part = dir.join(format!("{id}.{ext}.part"));
    let agent = agent(FAMILY.load(Ordering::Relaxed), Duration::from_secs(60));
    let mut file = std::fs::File::create(&part)?;
    const PIECE: u64 = 4 * 1024 * 1024;
    let mut at: u64 = 0;
    loop {
        let mut r = agent
            .get(&s.url)
            .header("User-Agent", UA)
            .header("Range", &format!("bytes={}-{}", at, at + PIECE - 1))
            .call()
            .map_err(|e| Error::Download(e.to_string()))?;
        let total = r
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok());
        let mut buf = Vec::with_capacity(PIECE as usize);
        r.body_mut().with_config().limit(PIECE + 1024).reader().read_to_end(&mut buf).map_err(|e| Error::Download(e.to_string()))?;
        if buf.is_empty() {
            break;
        }
        file.write_all(&buf)?;
        at += buf.len() as u64;
        match total {
            Some(t) if at < t => continue,
            _ => break,
        }
    }
    file.flush()?;
    drop(file);
    if at == 0 {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Download("the download was empty".into()));
    }
    std::fs::rename(&part, &path)?;
    Ok((path, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: &str) -> Value {
        json!({
            "playabilityStatus": {"status": status, "reason": "Sign in to confirm you’re not a bot"},
            "streamingData": {"expiresInSeconds": "21540", "adaptiveFormats": [
                {"itag": 137, "mimeType": "video/mp4", "url": "https://v/137"},
                {"itag": 140, "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 130677, "url": "https://a/140?expire=1"},
                {"itag": 251, "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 136544, "url": "https://a/251"},
                {"itag": 249, "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 49496, "url": "https://a/249"},
                {"itag": 250, "mimeType": "audio/webm; codecs=\"opus\"", "signatureCipher": "s=…"}
            ]},
            "videoDetails": {"lengthSeconds": "213"}
        })
    }

    #[test]
    fn picks_audio_by_quality() {
        let a = answer("OK");
        let hi = pick(&a, Quality::High).unwrap();
        assert_eq!(hi.url, "https://a/251");
        assert_eq!(hi.ext.as_deref(), Some("webm"));
        assert_eq!(hi.duration_secs, Some(213));
        assert!(hi.expires_at.unwrap() > now() + 20_000);
        assert_eq!(pick(&a, Quality::Medium).unwrap().url, "https://a/140?expire=1");
        assert_eq!(pick(&a, Quality::Low).unwrap().url, "https://a/249");
        assert!(hi.headers.contains_key("User-Agent"));
    }

    #[test]
    fn refusals_carry_the_reason() {
        let e = pick(&answer("LOGIN_REQUIRED"), Quality::High).unwrap_err().to_string();
        assert!(e.contains("LOGIN_REQUIRED") && is_bot_check(&e), "{e}");
        let none = pick(&json!({"playabilityStatus": {"status": "OK"}, "streamingData": {}}), Quality::High);
        assert!(none.is_err());
    }

    /// `cargo test -p harp-ytdlp -- --ignored live_native`
    #[test]
    #[ignore]
    fn live_native() {
        let s = resolve("dQw4w9WgXcQ", Quality::High).unwrap();
        assert!(s.url.starts_with("https://"));
        let dir = std::env::temp_dir().join("harp-native-live");
        let (p, n) = download("dQw4w9WgXcQ", &dir, Quality::Medium).unwrap();
        eprintln!("{} ({n} bytes)", p.display());
        assert!(n > 1_000_000);
    }
}
