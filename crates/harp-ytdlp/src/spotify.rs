//! A Spotify connection, like Exportify: you sign in to your own Spotify account in
//! the browser, and Harp reads *all* of your playlists and Liked Songs, with no cap
//! (a public playlist page stops at 100 songs).
//!
//! * Sign-in is OAuth with PKCE, so there is no client secret and nothing of ours in
//!   the middle. You use your own free Spotify developer app (a Client ID).
//! * The browser sends the sign-in result to a tiny server on this device
//!   (`127.0.0.1`), which is the only place the code goes.
//! * Tokens stay on this device. Harp only reads (playlist and library read scopes).

use crate::{Error, Result};
use harp_core::import::{ImportTrack, ImportedPlaylist};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// The address to register in your Spotify app as a Redirect URI.
pub const REDIRECT_PORT: u16 = 43821;
pub fn redirect_uri() -> String {
    format!("http://127.0.0.1:{REDIRECT_PORT}/callback")
}

pub const SCOPES: &str = "playlist-read-private playlist-read-collaborative user-library-read";

// --- PKCE ---

pub fn b64url(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut o = String::new();
    for c in input.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        if c.len() > 1 {
            o.push(T[(n >> 6) as usize & 63] as char);
        }
        if c.len() > 2 {
            o.push(T[n as usize & 63] as char);
        }
    }
    o
}

pub fn challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        // No /dev/urandom (Windows): mix the clock, the process and a counter.
        let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1) ^ (std::process::id() as u128) << 64;
        for x in b.iter_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *x = (seed >> 33) as u8;
        }
    }
    b
}

pub fn new_verifier() -> String {
    b64url(&random_bytes(48)) // 64 characters of the allowed set
}

pub fn new_state() -> String {
    b64url(&random_bytes(16))
}

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

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn authorize_url(client_id: &str, challenge: &str, state: &str) -> String {
    format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge_method=S256&code_challenge={}&state={}",
        enc(client_id),
        enc(&redirect_uri()),
        enc(SCOPES),
        challenge,
        state
    )
}

// --- the loopback callback ---

pub struct Callback {
    listener: TcpListener,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Callback {
    pub fn listen_on(port: u16) -> Result<Callback> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| Error::Download(format!("cannot listen on 127.0.0.1:{port} for Spotify's reply ({e}); is another Harp sign-in open?")))?;
        listener.set_nonblocking(true)?;
        Ok(Callback { listener, cancel: Default::default() })
    }

    pub fn listen() -> Result<Callback> {
        Self::listen_on(REDIRECT_PORT)
    }

    /// A handle that makes a waiting `wait_for_code` give up (the user pressed Cancel).
    pub fn canceller(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.cancel.clone()
    }

    /// Waits for the browser to arrive with `?code=...&state=...` and returns the code.
    pub fn wait_for_code(&self, expected_state: &str, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        loop {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(Error::Download("sign-in cancelled".into()));
            }
            if start.elapsed() > timeout {
                return Err(Error::Download("timed out waiting for the Spotify sign-in".into()));
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = handle_callback(stream, expected_state) {
                        return result;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(e.into()),
            }
        }
    }
}

fn page(title: &str, body: &str) -> String {
    format!("<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Harp</title><body style='font-family:sans-serif;background:#111;color:#eee;display:grid;place-items:center;height:100vh;margin:0;text-align:center'><div><h1>{title}</h1><p>{body}</p></div>")
}

fn respond(stream: &mut TcpStream, status: &str, html: String) {
    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}", html.len());
}

/// `None`: not the callback (a favicon request, a stray probe): keep waiting.
fn handle_callback(mut stream: TcpStream, expected_state: &str) -> Option<Result<String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    BufReader::new(stream.try_clone().ok()?).read_line(&mut line).ok()?;
    let target = line.split_whitespace().nth(1)?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        respond(&mut stream, "404 Not Found", page("Not here", ""));
        return None;
    }
    let get = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(dec);
    if let Some(err) = get("error") {
        respond(&mut stream, "200 OK", page("Not connected", "Spotify said no. You can close this tab and try again in Harp."));
        return Some(Err(Error::Download(format!("Spotify sign-in was cancelled ({err})"))));
    }
    if get("state").as_deref() != Some(expected_state) {
        respond(&mut stream, "400 Bad Request", page("Wrong sign-in", "This reply was not for this Harp sign-in. Start again in Harp."));
        return None; // someone else's request: ignore it and keep waiting
    }
    match get("code") {
        Some(code) => {
            respond(&mut stream, "200 OK", page("Connected", "You can close this tab and go back to Harp."));
            Some(Ok(code))
        }
        None => {
            respond(&mut stream, "400 Bad Request", page("No code", "Spotify's reply had no code."));
            None
        }
    }
}

// --- the API ---

#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub access: String,
    pub refresh: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
}

impl Tokens {
    pub fn to_json(&self) -> String {
        serde_json::json!({"access": self.access, "refresh": self.refresh, "expires_at": self.expires_at}).to_string()
    }
    pub fn from_json(s: &str) -> Option<Tokens> {
        let v: Value = serde_json::from_str(s).ok()?;
        Some(Tokens {
            access: v.get("access")?.as_str()?.into(),
            refresh: v.get("refresh").and_then(Value::as_str).map(String::from),
            expires_at: v.get("expires_at")?.as_u64()?,
        })
    }
    /// Refresh a minute early.
    pub fn is_fresh(&self, now: u64) -> bool {
        self.expires_at > now + 60
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistInfo {
    /// Playlist id, or "liked" for Liked Songs.
    pub id: String,
    pub name: String,
    pub total: u32,
    pub owner: String,
}

pub struct Api {
    pub accounts: String,
    pub api: String,
}

impl Default for Api {
    fn default() -> Self {
        Api { accounts: "https://accounts.spotify.com".into(), api: "https://api.spotify.com/v1".into() }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(25))).http_status_as_error(false).build().into()
}

impl Api {
    fn token_request(&self, form: &[(&str, &str)]) -> Result<Tokens> {
        let mut resp = agent()
            .post(&format!("{}/api/token", self.accounts))
            .send_form(form.iter().copied())
            .map_err(|e| Error::Download(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()))?;
        if status != 200 {
            let why = v.get("error_description").or_else(|| v.get("error")).and_then(Value::as_str).unwrap_or("unknown error");
            return Err(Error::Download(format!("Spotify refused the sign-in: {why}")));
        }
        Ok(Tokens {
            access: v.get("access_token").and_then(Value::as_str).ok_or_else(|| Error::Parse("no access token".into()))?.into(),
            refresh: v.get("refresh_token").and_then(Value::as_str).map(String::from),
            expires_at: now() + v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600),
        })
    }

    pub fn exchange(&self, client_id: &str, code: &str, verifier: &str) -> Result<Tokens> {
        self.token_request(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri()),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ])
    }

    /// A new access token; Spotify may or may not send a new refresh token.
    pub fn refresh(&self, client_id: &str, refresh: &str) -> Result<Tokens> {
        let mut t = self.token_request(&[("grant_type", "refresh_token"), ("refresh_token", refresh), ("client_id", client_id)])?;
        if t.refresh.is_none() {
            t.refresh = Some(refresh.into());
        }
        Ok(t)
    }

    /// GET with the bearer token, waiting out rate limits (429) and retrying server errors.
    fn get(&self, token: &str, url: &str) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let mut resp = agent()
                .get(url)
                .header("Authorization", &format!("Bearer {token}"))
                .call()
                .map_err(|e| Error::Download(e.to_string()))?;
            let status = resp.status().as_u16();
            match status {
                200 => {
                    let text = resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))?;
                    return serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()));
                }
                429 if attempt < 6 => {
                    let wait = resp.headers().get("retry-after").and_then(|h| h.to_str().ok()).and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(2);
                    attempt += 1;
                    std::thread::sleep(Duration::from_secs(wait.clamp(1, 20)));
                }
                500..=599 if attempt < 3 => {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(700 * attempt));
                }
                401 => return Err(Error::Download("Spotify sign-in expired: connect again".into())),
                403 => return Err(Error::Download("Spotify did not allow reading that (403). Playlists you do not own or follow are not readable with a personal app.".into())),
                404 | 410 => return Err(Error::Download("not found".into())),
                s => return Err(Error::Download(format!("Spotify answered HTTP {s}"))),
            }
        }
    }

    /// Every page of a list, following `next` until there is none.
    fn all_pages(&self, token: &str, first: &str, mut take: impl FnMut(&Value)) -> Result<()> {
        let mut url = first.to_string();
        let mut pages = 0;
        loop {
            let v = self.get(token, &url)?;
            take(&v);
            pages += 1;
            match v.get("next").and_then(Value::as_str) {
                Some(n) if !n.is_empty() && pages < 500 => url = n.to_string(),
                _ => return Ok(()),
            }
        }
    }

    /// Your playlists (the ones you own or follow), with Liked Songs first.
    pub fn playlists(&self, token: &str) -> Result<Vec<PlaylistInfo>> {
        let mut out = Vec::new();
        // "Liked Songs" is a library, not a playlist: ask for its size.
        if let Ok(v) = self.get(token, &format!("{}/me/tracks?limit=1", self.api)) {
            out.push(PlaylistInfo {
                id: "liked".into(),
                name: "Liked Songs".into(),
                total: v.get("total").and_then(Value::as_u64).unwrap_or(0) as u32,
                owner: "you".into(),
            });
        }
        self.all_pages(token, &format!("{}/me/playlists?limit=50", self.api), |v| {
            for p in v.get("items").and_then(Value::as_array).into_iter().flatten() {
                let Some(id) = p.get("id").and_then(Value::as_str) else { continue };
                // The size lives under "tracks" (older) or "items" (newer API).
                let total = p.pointer("/tracks/total").or_else(|| p.pointer("/items/total")).and_then(Value::as_u64).unwrap_or(0) as u32;
                out.push(PlaylistInfo {
                    id: id.into(),
                    name: p.get("name").and_then(Value::as_str).unwrap_or("Playlist").into(),
                    total,
                    owner: p.pointer("/owner/display_name").and_then(Value::as_str).unwrap_or("").into(),
                });
            }
        })?;
        Ok(out)
    }

    /// All the songs of a playlist (id "liked" for Liked Songs), however many.
    pub fn tracks(&self, token: &str, id: &str, name: &str) -> Result<ImportedPlaylist> {
        let read = |first: &str| -> Result<Vec<ImportTrack>> {
            let mut out = Vec::new();
            self.all_pages(token, first, |v| {
                for item in v.get("items").and_then(Value::as_array).into_iter().flatten() {
                    if let Some(t) = parse_track(item) {
                        out.push(t);
                    }
                }
            })?;
            Ok(out)
        };
        let tracks = if id == "liked" {
            read(&format!("{}/me/tracks?limit=50", self.api))?
        } else {
            // Newer API versions call it /items, older ones /tracks: try one, then the other.
            match read(&format!("{}/playlists/{}/items?limit=100", self.api, enc(id))) {
                Ok(v) => v,
                Err(_) => read(&format!("{}/playlists/{}/tracks?limit=100", self.api, enc(id)))?,
            }
        };
        Ok(ImportedPlaylist { name: name.into(), tracks })
    }
}

/// One entry of a playlist page: `{"track": {...}}` (older) or `{"item": {...}}` (newer).
pub fn parse_track(entry: &Value) -> Option<ImportTrack> {
    let t = entry.get("track").or_else(|| entry.get("item")).or(Some(entry)).filter(|t| t.get("name").is_some())?;
    if t.get("is_local").and_then(Value::as_bool) == Some(true) || matches!(t.get("type").and_then(Value::as_str), Some("episode")) {
        return None;
    }
    let title = t.get("name")?.as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    let artist = t
        .get("artists")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("name").and_then(Value::as_str)).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    Some(ImportTrack {
        title: title.into(),
        artist,
        album: t.pointer("/album/name").and_then(Value::as_str).filter(|a| !a.is_empty()).map(String::from),
        duration_secs: t.get("duration_ms").and_then(Value::as_u64).map(|ms| (ms / 1000) as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pkce_matches_the_rfc_7636_example() {
        // RFC 7636, appendix B.
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let v = new_verifier();
        assert!(v.len() >= 43 && v.len() <= 128 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(new_verifier(), v);
        assert_eq!(b64url(b"\xfb\xff"), "-_8");
    }

    #[test]
    fn authorize_url_has_everything_spotify_needs() {
        let u = authorize_url("my client", "CHAL", "STATE");
        assert!(u.starts_with("https://accounts.spotify.com/authorize?"));
        for part in ["client_id=my%20client", "response_type=code", "redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback", "code_challenge_method=S256", "code_challenge=CHAL", "state=STATE", "playlist-read-private"] {
            assert!(u.contains(part), "{part} in {u}");
        }
        assert_eq!(dec("a%20b+c%2Fd"), "a b c/d");
    }

    /// Sends one request to the sign-in listener and returns its answer.
    fn request(port: u16, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        r
    }

    #[test]
    fn callback_accepts_the_right_state_and_ignores_the_wrong_one() {
        // The listener waits in the background; requests are sent one at a time, each
        // answered before the next (no race between them).
        let cb = Callback::listen_on(0).unwrap();
        let port = cb.listener.local_addr().unwrap().port();
        let waiter = std::thread::spawn(move || cb.wait_for_code("STATE", Duration::from_secs(20)));
        assert!(request(port, "/callback?code=EVIL&state=WRONG").contains("Wrong sign-in"));
        assert!(request(port, "/favicon.ico").contains("404"));
        assert!(request(port, "/callback?code=GOOD%2Fcode&state=STATE").contains("Connected"));
        assert_eq!(waiter.join().unwrap().unwrap(), "GOOD/code");

        // The user pressing "cancel" at Spotify is reported, not hung on.
        let cb2 = Callback::listen_on(0).unwrap();
        let p2 = cb2.listener.local_addr().unwrap().port();
        let w2 = std::thread::spawn(move || cb2.wait_for_code("S", Duration::from_secs(20)));
        assert!(request(p2, "/callback?error=access_denied&state=S").contains("Not connected"));
        assert!(w2.join().unwrap().unwrap_err().to_string().contains("cancelled"));

        assert!(Callback::listen_on(0).unwrap().wait_for_code("S", Duration::from_millis(300)).unwrap_err().to_string().contains("timed out"));
        // Cancel from another thread ends the wait at once.
        let cb3 = Callback::listen_on(0).unwrap();
        let stop = cb3.canceller();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        let began = Instant::now();
        assert!(cb3.wait_for_code("S", Duration::from_secs(30)).unwrap_err().to_string().contains("cancelled"));
        assert!(began.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn tracks_in_both_api_shapes_and_things_to_skip() {
        let old = json!({"track": {"name": "Hello", "artists": [{"name": "Adele"}], "album": {"name": "25"}, "duration_ms": 295000}});
        let new = json!({"item": {"name": "Hello", "artists": [{"name": "Adele"}, {"name": "X"}], "album": {"name": "25"}, "duration_ms": 295000}});
        assert_eq!(parse_track(&old).unwrap().artist, "Adele");
        assert_eq!(parse_track(&new).unwrap().artist, "Adele, X");
        assert_eq!(parse_track(&old).unwrap().duration_secs, Some(295));
        assert!(parse_track(&json!({"track": null})).is_none(), "removed songs come back as null");
        assert!(parse_track(&json!({"track": {"name": "My file", "is_local": true}})).is_none());
        assert!(parse_track(&json!({"track": {"name": "Ep 1", "type": "episode"}})).is_none());
    }

    #[test]
    fn tokens_roundtrip() {
        let t = Tokens { access: "a".into(), refresh: Some("r".into()), expires_at: 1000 };
        assert_eq!(Tokens::from_json(&t.to_json()), Some(t.clone()));
        assert!(t.is_fresh(900) && !t.is_fresh(950));
        assert_eq!(Tokens::from_json("nope"), None);
    }

    // --- a fake Spotify on a local port, to test the paging and the rate limit ---

    type Handler = dyn Fn(&str, &str) -> (u16, String, Vec<(String, String)>) + Send + Sync;

    fn serve(handler: Arc<Handler>) -> String {
        let ln = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", ln.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for s in ln.incoming().flatten() {
                let h = handler.clone();
                std::thread::spawn(move || {
                    let mut rd = BufReader::new(s.try_clone().unwrap());
                    let mut line = String::new();
                    rd.read_line(&mut line).unwrap();
                    let (method, target) = { let mut p = line.split_whitespace(); (p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string()) };
                    let (mut auth, mut len) = (String::new(), 0usize);
                    loop {
                        let mut l = String::new();
                        if rd.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" { break; }
                        let low = l.to_lowercase();
                        if let Some(v) = low.strip_prefix("authorization:") { auth = v.trim().to_string(); }
                        if let Some(v) = low.strip_prefix("content-length:") { len = v.trim().parse().unwrap_or(0); }
                    }
                    let mut body = vec![0u8; len];
                    rd.read_exact(&mut body).unwrap();
                    let key = if method == "POST" { format!("POST {target} {}", String::from_utf8_lossy(&body)) } else { target };
                    let (status, text, headers) = h(&key, &auth);
                    let mut s = s;
                    let extra: String = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
                    let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
                });
            }
        });
        base
    }

    fn track_item(i: usize) -> Value {
        json!({"track": {"name": format!("Song {i}"), "artists": [{"name": "Artist"}], "album": {"name": "Album"}, "duration_ms": 200000 + i}})
    }

    #[test]
    fn reads_a_playlist_of_250_songs_past_the_100_limit_and_waits_out_a_rate_limit() {
        let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let b2 = base.clone();
        let limited = Arc::new(Mutex::new(false));
        let handler: Arc<Handler> = Arc::new(move |key, auth| {
            if auth != "bearer tok" {
                return (401, "{}".into(), vec![]);
            }
            let base = b2.lock().unwrap().clone();
            // A page of up to 100 songs, with a `next` link until all 250 are given.
            if let Some(rest) = key.strip_prefix("/v1/playlists/P1/items") {
                let offset: usize = rest.split("offset=").nth(1).and_then(|s| s.split('&').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
                // The second page is rate limited once.
                if offset == 100 && !std::mem::replace(&mut *limited.lock().unwrap(), true) {
                    return (429, "{}".into(), vec![("Retry-After".into(), "1".into())]);
                }
                let end = (offset + 100).min(250);
                let items: Vec<Value> = (offset..end).map(track_item).collect();
                let next = if end < 250 { json!(format!("{base}/v1/playlists/P1/items?limit=100&offset={end}")) } else { Value::Null };
                return (200, json!({"items": items, "next": next, "total": 250}).to_string(), vec![]);
            }
            (404, "{}".into(), vec![])
        });
        *base.lock().unwrap() = serve(handler);
        let api = Api { accounts: String::new(), api: format!("{}/v1", base.lock().unwrap()) };
        let started = Instant::now();
        let pl = api.tracks("tok", "P1", "Road").unwrap();
        assert_eq!(pl.tracks.len(), 250, "every song, not the first 100");
        assert_eq!(pl.tracks[249].title, "Song 249");
        assert!(started.elapsed() >= Duration::from_secs(1), "it waited for Retry-After");
        assert!(api.tracks("wrong", "P1", "x").unwrap_err().to_string().contains("expired"));
    }

    #[test]
    fn falls_back_to_the_older_tracks_endpoint_and_lists_playlists_with_liked_songs() {
        let handler: Arc<Handler> = Arc::new(|key, _| {
            if key.starts_with("/v1/playlists/OLD/items") {
                return (404, "{}".into(), vec![]);
            }
            if key.starts_with("/v1/playlists/OLD/tracks") {
                return (200, json!({"items": [track_item(1), track_item(2), json!({"track": null})], "next": null}).to_string(), vec![]);
            }
            if key.starts_with("/v1/me/tracks?limit=1") {
                return (200, json!({"total": 777, "items": []}).to_string(), vec![]);
            }
            if key.starts_with("/v1/me/playlists") {
                return (200, json!({"items": [
                    {"id": "A", "name": "Gym", "tracks": {"total": 40}, "owner": {"display_name": "me"}},
                    {"id": "B", "name": "New shape", "items": {"total": 12}, "owner": {"display_name": "me"}}], "next": null}).to_string(), vec![]);
            }
            (404, "{}".into(), vec![])
        });
        let base = serve(handler);
        let api = Api { accounts: String::new(), api: format!("{base}/v1") };
        assert_eq!(api.tracks("t", "OLD", "Old").unwrap().tracks.len(), 2);
        let pls = api.playlists("t").unwrap();
        assert_eq!((pls[0].id.as_str(), pls[0].total), ("liked", 777));
        assert_eq!((pls[1].name.as_str(), pls[1].total), ("Gym", 40));
        assert_eq!(pls[2].total, 12, "the newer `items` size is read too");
    }

    #[test]
    fn token_exchange_and_refresh_against_a_fake_accounts_server() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let s2 = seen.clone();
        let handler: Arc<Handler> = Arc::new(move |key, _| {
            s2.lock().unwrap().push(key.to_string());
            if key.contains("grant_type=authorization_code") {
                return (200, json!({"access_token": "AT", "refresh_token": "RT", "expires_in": 3600}).to_string(), vec![]);
            }
            if key.contains("refresh_token=BAD") {
                return (400, json!({"error": "invalid_grant", "error_description": "Refresh token revoked"}).to_string(), vec![]);
            }
            // A refresh that does not send a new refresh token keeps the old one.
            (200, json!({"access_token": "AT2", "expires_in": 3600}).to_string(), vec![])
        });
        let base = serve(handler);
        let api = Api { accounts: base, api: String::new() };
        let t = api.exchange("CID", "CODE", "VERIFIER").unwrap();
        assert_eq!((t.access.as_str(), t.refresh.as_deref()), ("AT", Some("RT")));
        let posted = seen.lock().unwrap()[0].clone();
        for part in ["code=CODE", "client_id=CID", "code_verifier=VERIFIER", "redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback"] {
            assert!(posted.contains(part), "{part} in {posted}");
        }
        let r = api.refresh("CID", "RT").unwrap();
        assert_eq!((r.access.as_str(), r.refresh.as_deref()), ("AT2", Some("RT")));
        assert!(api.refresh("CID", "BAD").unwrap_err().to_string().contains("revoked"));
    }
}
