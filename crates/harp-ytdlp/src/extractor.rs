use crate::config::Tool;
use crate::parse::{self, StreamInfo};
use crate::{Error, Result};
use harp_core::recommend::CandidateSource;
use harp_core::Song;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type Stream = StreamInfo;

/// Audio quality for streaming and downloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    /// About 48 to 64 kbps: saves mobile data.
    Low,
    /// About 128 kbps.
    Medium,
    /// The best the source has (usually 128 to 160 kbps AAC / Opus).
    High,
}

impl Quality {
    pub fn parse(s: &str) -> Quality {
        match s {
            "low" => Quality::Low,
            "medium" => Quality::Medium,
            _ => Quality::High,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "high",
        }
    }

    /// yt-dlp format selector. Falls back to worse/better so something always plays.
    pub fn format(self) -> &'static str {
        match self {
            Quality::Low => "worstaudio[abr>=40]/bestaudio[abr<=70]/worstaudio/bestaudio",
            Quality::Medium => "bestaudio[abr<=130]/bestaudio[abr<=160]/bestaudio",
            Quality::High => "bestaudio[ext=m4a]/bestaudio/best",
        }
    }
}

/// Stop using a cached URL this long before it expires.
const EXPIRY_MARGIN_SECS: u64 = 300;

pub struct YtDlp {
    tool: Tool,
    cache: Mutex<HashMap<(String, Quality), Stream>>,
    ytmusic: bool,
    light_client: bool,
    /// Desktop: `--force-ipv4` / `--force-ipv6` once one family got refused.
    family: Mutex<Option<&'static str>>,
    /// Find streams with Harp's own finder first (see `native`).
    native: bool,
    /// False where yt-dlp can't run (iOS): only the built-in finder and the
    /// catalogue API are used.
    has_tool: bool,
}

/// iOS: yt-dlp runs inside the app's own Python (apps there can't start other
/// programs) and answers on a loopback port, guarded by a token.
static SERVER: Mutex<Option<(u16, String)>> = Mutex::new(None);

/// Points yt-dlp runs at the in-app Python (port, token).
pub fn set_server(port: u16, token: String) {
    *SERVER.lock().unwrap() = Some((port, token));
}

/// Runs yt-dlp with `args` in the in-app Python: one JSON line out
/// (`{"token", "args"}`), one back (`{"code", "stdout", "stderr"}`).
fn run_in_app(port: u16, token: &str, args: &[String], timeout: std::time::Duration) -> Result<String> {
    use std::io::{BufRead, BufReader, Write};
    // Python may still be starting: try for a few seconds.
    let started = Instant::now();
    let mut stream = loop {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(e) if started.elapsed() < std::time::Duration::from_secs(8) => {
                let _ = e;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Err(Error::Failed(format!("yt-dlp inside the app isn't answering: {e}"))),
        }
    };
    stream.set_read_timeout(Some(timeout))?;
    let req = serde_json::json!({"token": token, "args": args});
    stream.write_all(format!("{req}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| {
        if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) { Error::Timeout } else { Error::Spawn(e) }
    })?;
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| Error::Parse(e.to_string()))?;
    let stdout = v["stdout"].as_str().unwrap_or_default().to_string();
    if v["code"].as_i64().unwrap_or(1) != 0 {
        let stderr = v["stderr"].as_str().unwrap_or_default();
        let msg = stderr.lines().rev().find(|l| l.contains("ERROR")).unwrap_or(stderr.trim()).trim();
        return Err(Error::Failed(msg.chars().take(300).collect()));
    }
    Ok(stdout)
}

const LIGHT_CLIENT: &str = "youtube:player_client=visionos;player_skip=webpage,configs";

/// YouTube refusing an address for asking too much.
pub fn is_bot_check(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("not a bot") || m.contains("confirm you") || m.contains("http error 429") || m.contains("too many requests")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Errors that come from the network rather than from the video.
pub(crate) fn is_transient(msg: &str) -> bool {
    let m = msg.to_lowercase();
    ["tunnel connection failed", "502", "503", "504", "timed out", "temporary failure", "connection reset", "connection refused", "remote end closed", "unable to download api page"]
        .iter()
        .any(|k| m.contains(k))
}

impl YtDlp {
    pub fn new(tool: Tool) -> Self {
        YtDlp { tool, cache: Mutex::new(HashMap::new()), ytmusic: true, light_client: true, family: Mutex::new(None), native: !cfg!(test), has_tool: true }
    }

    /// Use only yt-dlp, never YouTube Music's web API (tests, or if it breaks).
    pub fn without_ytmusic(mut self) -> Self {
        self.ytmusic = false;
        self
    }

    /// Always let yt-dlp choose YouTube's clients (tests with a fake yt-dlp).
    pub fn without_light_client(mut self) -> Self {
        self.light_client = false;
        self
    }

    /// Without yt-dlp (iOS): streams and downloads with the built-in finder,
    /// search and radio with the catalogue API.
    pub fn native_only() -> Self {
        let mut y = YtDlp::new(Tool::binary(""));
        y.native = true;
        y.has_tool = false;
        y
    }

    /// yt-dlp can run here: as a program, or (iOS) inside the app's own Python.
    fn can_run(&self) -> bool {
        self.has_tool || SERVER.lock().unwrap().is_some()
    }

    fn run(&self, extra: &[String]) -> Result<String> {
        let server = SERVER.lock().unwrap().clone();
        if let Some((port, token)) = server {
            let mut args = self.tool.base_args();
            if let Some(f) = *self.family.lock().unwrap() {
                args.push(f.to_string());
            }
            args.extend(extra.iter().cloned());
            return run_in_app(port, &token, &args, self.tool.timeout);
        }
        if !self.has_tool {
            return Err(Error::Failed("not available on this device".into()));
        }
        let mut cmd = Command::new(&self.tool.program);
        cmd.args(self.tool.base_args());
        if let Some(f) = *self.family.lock().unwrap() {
            cmd.arg(f);
        }
        cmd.args(extra);
        for (k, v) in &self.tool.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // "Text file busy" happens when the program was written a moment ago while
        // another thread forked; it clears within milliseconds.
        let mut child = {
            let mut tries = 0;
            loop {
                match cmd.spawn() {
                    Ok(c) => break c,
                    Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 8 => {
                        tries += 1;
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        };
        // Read both pipes on threads so a full pipe never blocks the child.
        let mut out = child.stdout.take().expect("piped");
        let mut err = child.stderr.take().expect("piped");
        let t_out = std::thread::spawn(move || {
            let mut b = String::new();
            let _ = out.read_to_string(&mut b);
            b
        });
        let t_err = std::thread::spawn(move || {
            let mut b = String::new();
            let _ = err.read_to_string(&mut b);
            b
        });
        let start = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if start.elapsed() > self.tool.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Timeout);
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        let stdout = t_out.join().unwrap_or_default();
        let stderr = t_err.join().unwrap_or_default();
        if !status.success() {
            let msg = stderr.lines().rev().find(|l| l.contains("ERROR")).unwrap_or(stderr.trim()).trim();
            return Err(Error::Failed(msg.chars().take(300).collect()));
        }
        Ok(stdout)
    }

    fn flat(&self, target: &str, n: usize) -> Result<Vec<Song>> {
        let out = self.run(&[
            "--flat-playlist".into(),
            "-J".into(),
            "--playlist-end".into(),
            n.to_string(),
            target.into(),
        ])?;
        parse::songs_from_playlist(&out)
    }

    /// Search songs: YouTube Music's own search first (artist, album, length,
    /// cover art), then yt-dlp's plain YouTube search if that fails or is empty.
    /// (YouTube Music's page in yt-dlp's flat mode has only ids and titles, and
    /// plain `ytsearch` has a channel name rather than the artist.)
    pub fn search_songs(&self, query: &str, limit: usize) -> Result<Vec<Song>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        if self.ytmusic {
            if let Ok(v) = crate::ytmusic::search(q, limit) {
                if !v.is_empty() {
                    return Ok(v);
                }
            }
        }
        self.flat(&format!("ytsearch{limit}:{q}"), limit)
    }

    /// An audio stream for a video at the chosen quality, cached until shortly
    /// before it expires.
    pub fn resolve(&self, id: &str, q: Quality) -> Result<Stream> {
        let key = (id.to_string(), q);
        if let Some(s) = self.cache.lock().unwrap().get(&key) {
            if s.expires_at.map_or(true, |e| e > now() + EXPIRY_MARGIN_SECS) {
                return Ok(s.clone());
            }
        }
        // Harp's own finder first (one request, no Python); yt-dlp if it can't.
        if self.native {
            match crate::native::resolve(id, q) {
                Ok(s) => {
                    self.cache.lock().unwrap().insert(key, s.clone());
                    return Ok(s);
                }
                Err(e) if !self.can_run() => return Err(e),
                Err(_) => {}
            }
        }
        let url = format!("https://www.youtube.com/watch?v={id}");
        let out = self.with_clients(|client| {
            let mut a = vec!["--no-playlist".to_string()];
            a.extend(client.iter().map(|x| x.to_string()));
            a.extend(["-f".into(), q.format().into(), "-J".into(), url.clone()]);
            self.run_retrying(&a)
        })?;
        let s = parse::stream_from_info(&out)?;
        self.cache.lock().unwrap().insert(key, s.clone());
        Ok(s)
    }

    /// Downloads a song's audio into `dir` for offline playback. Returns the
    /// file path and its size.
    pub fn download(&self, id: &str, dir: &std::path::Path, q: Quality) -> Result<(std::path::PathBuf, u64)> {
        if self.native {
            match crate::native::download(id, dir, q) {
                Ok(r) => return Ok(r),
                Err(e) if !self.can_run() => return Err(e),
                Err(_) => {}
            }
        }
        std::fs::create_dir_all(dir)?;
        let template = dir.join(format!("{id}.%(ext)s"));
        let url = format!("https://www.youtube.com/watch?v={id}");
        let out = self.with_clients(|client| {
            let mut a = vec!["--no-playlist".to_string(), "--no-part".into()];
            a.extend(client.iter().map(|x| x.to_string()));
            a.extend([
                "-f".into(),
                q.format().into(),
                "-o".into(),
                template.to_string_lossy().into_owned(),
                "--print".into(),
                "after_move:filepath".into(),
                "--no-simulate".into(),
                url.clone(),
            ]);
            self.run_retrying(&a)
        })?;
        let path = out
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| Error::Failed("yt-dlp did not report a file".into()))?;
        let bytes = std::fs::metadata(&path)?.len();
        Ok((path, bytes))
    }

    /// A flaky connection (a dropped tunnel, a 502, a timeout) is worth another try.
    fn run_retrying(&self, args: &[String]) -> Result<String> {
        let mut attempt = 0;
        loop {
            match self.run(args) {
                Err(Error::Failed(m)) if attempt < 2 && is_transient(&m) => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(600 * attempt as u64));
                }
                r => return r,
            }
        }
    }

    /// Runs `f` with YouTube's lightest client first, then yt-dlp's own choice.
    ///
    /// The `visionos` client returns plain audio links from one request, with no
    /// web page and no player code to download and solve (slow with QuickJS on a
    /// phone). yt-dlp's default instead asks three clients, after the page and the
    /// player code: four or five requests a song, which is what gets an address
    /// "confirm you're not a bot" sooner. If the light client has nothing, the
    /// default runs as before.
    fn with_clients(&self, f: impl Fn(&[&str]) -> Result<String>) -> Result<String> {
        let attempt = || -> Result<String> {
            if !self.light_client {
                return f(&[]);
            }
            match f(&["--extractor-args", LIGHT_CLIENT]) {
                Ok(o) => Ok(o),
                Err(Error::Timeout) => Err(Error::Timeout),
                Err(first) => f(&[]).map_err(|e| if is_bot_check(&e.to_string()) || !is_bot_check(&first.to_string()) { e } else { first }),
            }
        };
        match attempt() {
            Err(e) if is_bot_check(&e.to_string()) => {
                // YouTube refused this address. Try the other family (IPv6
                // instead of a carrier's shared IPv4, or the reverse) and keep
                // it if it works.
                self.switch_family();
                attempt().map_err(|second| {
                    self.switch_family();
                    if is_bot_check(&second.to_string()) { e } else { second }
                })
            }
            r => r,
        }
    }

    /// Flips which address family YouTube is asked from.
    fn switch_family(&self) {
        if self.tool.proxy.is_some() {
            crate::proxy::set_prefer_v6(!crate::proxy::prefer_v6());
        } else {
            let mut f = self.family.lock().unwrap();
            *f = match *f {
                Some("--force-ipv6") => Some("--force-ipv4"),
                _ => Some("--force-ipv6"),
            };
        }
    }

    pub fn forget(&self, id: &str) {
        self.cache.lock().unwrap().retain(|(i, _), _| i != id);
    }

    /// Songs in a playlist link.
    pub fn playlist(&self, url: &str, limit: usize) -> Result<Vec<Song>> {
        self.flat(url, limit)
    }

    pub fn version(&self) -> Result<String> {
        Ok(self.run(&["--version".into()])?.trim().to_string())
    }
}

impl CandidateSource for YtDlp {
    fn search(&self, query: &str, limit: usize) -> harp_core::Result<Vec<Song>> {
        self.search_songs(query, limit).map_err(|e| harp_core::Error::Source(e.to_string()))
    }

    /// YouTube Music's song radio, else YouTube's mix for a video ("RD" + id playlist) is its watch-next list.
    fn related(&self, song_id: &str, limit: usize) -> harp_core::Result<Vec<Song>> {
        if self.ytmusic {
            if let Ok(v) = crate::ytmusic::radio(song_id, limit) {
                if !v.is_empty() {
                    return Ok(v);
                }
            }
        }
        let url = format!("https://www.youtube.com/watch?v={song_id}&list=RD{song_id}");
        let mut v = self.flat(&url, limit + 1).map_err(|e| harp_core::Error::Source(e.to_string()))?;
        v.retain(|s| s.id != song_id);
        v.truncate(limit);
        Ok(v)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in yt-dlp: a shell script printing fixtures by what it is asked.
    fn fake(dir: &std::path::Path) -> Tool {
        let p = dir.join("yt-dlp");
        let search = include_str!("../tests/fixtures/search.json").replace('\n', "");
        let info = include_str!("../tests/fixtures/info.json").replace('\n', "");
        let script = format!(
            "#!/bin/sh\ncase \"$*\" in\n*--version*) echo 2099.01.01;;\n*ytsearch*|*list=RD*) cat <<'EOF'\n{search}\nEOF\n;;\n*watch?v=*) cat <<'EOF'\n{info}\nEOF\n;;\n*) echo 'ERROR: unexpected' >&2; exit 2;;\nesac\n"
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        Tool::binary(p)
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("harp-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn search_uses_youtube_search() {
        let y = YtDlp::new(fake(&tmp("search"))).without_ytmusic();
        let v = y.search_songs("daft punk", 5).unwrap();
        assert_eq!(v[0].title, "Get Lucky");
    }

    #[test]
    fn resolve_is_cached_until_expiry() {
        let d = tmp("resolve");
        let y = YtDlp::new(fake(&d));
        let a = y.resolve("5NV6Rdv1a3I", Quality::High).unwrap();
        // The fixture's expiry is far in the future, so removing the tool must not matter.
        std::fs::remove_file(d.join("yt-dlp")).unwrap();
        assert_eq!(y.resolve("5NV6Rdv1a3I", Quality::High).unwrap(), a);
        y.forget("5NV6Rdv1a3I");
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_err());
    }

    #[test]
    fn network_errors_are_retried_but_video_errors_are_not() {
        assert!(is_transient("ERROR: [youtube] x: Unable to download API page: <urlopen error Tunnel connection failed: 502 Bad Gateway>"));
        assert!(is_transient("The read operation timed out"));
        assert!(!is_transient("ERROR: Video unavailable. This video is private"));
        // A tool that fails once with a 502 and then works: resolve succeeds.
        let d = tmp("retry");
        let tool = d.join("yt-dlp");
        let info = include_str!("../tests/fixtures/info.json").replace('\n', "");
        std::fs::write(
            &tool,
            format!("#!/bin/sh\nif [ ! -f \"{d}/seen\" ]; then touch \"{d}/seen\"; echo 'ERROR: Tunnel connection failed: 502 Bad Gateway' >&2; exit 1; fi\ncat <<'EOF'\n{info}\nEOF\n", d = d.display()),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let y = YtDlp::new(Tool::binary(tool)).without_ytmusic();
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_ok());
    }

    #[test]
    fn light_client_first_then_ytdlps_choice() {
        // A tool that refuses the light client (logging each call): the default still plays.
        let d = tmp("light");
        let tool = d.join("yt-dlp");
        let info = include_str!("../tests/fixtures/info.json").replace('\n', "");
        std::fs::write(
            &tool,
            format!(
                "#!/bin/sh\necho \"$*\" >> \"{d}/calls\"\ncase \"$*\" in *visionos*) echo 'ERROR: [youtube] x: Sign in to confirm you’re not a bot' >&2; exit 1;; esac\ncat <<'EOF'\n{info}\nEOF\n",
                d = d.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let y = YtDlp::new(Tool::binary(tool)).without_ytmusic();
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_ok());
        let calls = std::fs::read_to_string(d.join("calls")).unwrap();
        let lines: Vec<&str> = calls.lines().collect();
        assert_eq!(lines.len(), 2, "{calls}");
        assert!(lines[0].contains("player_client=visionos") && lines[0].contains("--cache-dir").eq(&false));
        assert!(!lines[1].contains("visionos"));
        assert!(is_bot_check("ERROR: Sign in to confirm you're not a bot"));
        assert!(!is_bot_check("ERROR: Video unavailable"));
    }

    #[test]
    fn refused_address_switches_family_and_keeps_it() {
        // YouTube refuses unless asked over IPv6: Harp switches and stays switched.
        let d = tmp("family");
        let tool = d.join("yt-dlp");
        let info = include_str!("../tests/fixtures/info.json").replace('\n', "");
        std::fs::write(
            &tool,
            format!(
                "#!/bin/sh\necho \"$*\" >> \"{d}/calls\"\ncase \"$*\" in *--force-ipv6*) ;; *) echo 'ERROR: [youtube] x: Sign in to confirm you’re not a bot' >&2; exit 1;; esac\ncat <<'EOF'\n{info}\nEOF\n",
                d = d.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let y = YtDlp::new(Tool::binary(tool)).without_ytmusic();
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_ok());
        y.forget("5NV6Rdv1a3I");
        let before = std::fs::read_to_string(d.join("calls")).unwrap().lines().count();
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_ok());
        let calls = std::fs::read_to_string(d.join("calls")).unwrap();
        assert_eq!(calls.lines().count(), before + 1, "the working family is remembered: {calls}");
        assert!(calls.lines().last().unwrap().contains("--force-ipv6"));
    }

    #[test]
    fn runs_through_the_in_app_python() {
        use std::io::{BufRead, BufReader, Write};
        // A stand-in for the app's Python: checks the token, echoes the args.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (s, _) = l.accept().unwrap();
                let mut line = String::new();
                BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                let ok = v["token"] == "t0k";
                let reply = if ok {
                    serde_json::json!({"code": 0, "stdout": v["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect::<Vec<_>>().join(" "), "stderr": ""})
                } else {
                    serde_json::json!({"code": 1, "stdout": "", "stderr": "WARNING: x\nERROR: [youtube] y: Sign in to confirm you're not a bot"})
                };
                (&s).write_all(format!("{reply}\n").as_bytes()).unwrap();
            }
        });
        let out = run_in_app(port, "t0k", &["--version".into()], std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(out, "--version");
        let err = run_in_app(port, "wrong", &["-J".into()], std::time::Duration::from_secs(5)).unwrap_err().to_string();
        assert!(err.contains("ERROR") && is_bot_check(&err), "{err}");
    }

    #[test]
    fn cache_dir_replaces_no_cache() {
        let mut t = Tool::binary("/bin/yt-dlp");
        assert!(t.base_args().contains(&"--no-cache-dir".to_string()));
        t.cache_dir = Some("/data/ytc".into());
        let a = t.base_args();
        assert!(!a.contains(&"--no-cache-dir".to_string()));
        assert!(a.windows(2).any(|w| w[0] == "--cache-dir" && w[1] == "/data/ytc"));
    }

    #[test]
    fn qualities_parse_and_select_formats() {
        assert_eq!(Quality::parse("low"), Quality::Low);
        assert_eq!(Quality::parse("medium"), Quality::Medium);
        assert_eq!(Quality::parse("anything"), Quality::High);
        assert!(Quality::Low.format().starts_with("worstaudio"));
        assert!(Quality::Medium.format().contains("abr<=130"));
        assert_eq!(Quality::High.as_str(), "high");
    }

    #[test]
    fn each_quality_is_cached_separately() {
        let d = tmp("qcache");
        let y = YtDlp::new(fake(&d)).without_ytmusic();
        y.resolve("5NV6Rdv1a3I", Quality::High).unwrap();
        std::fs::remove_file(d.join("yt-dlp")).unwrap();
        assert!(y.resolve("5NV6Rdv1a3I", Quality::High).is_ok());
        assert!(y.resolve("5NV6Rdv1a3I", Quality::Low).is_err(), "not cached at another quality");
    }

    #[test]
    fn download_reports_the_file() {
        let d = tmp("dl");
        let tool = d.join("yt-dlp");
        let out = d.join("music");
        // A stand-in that writes the file it is asked for and prints its path.
        std::fs::write(
            &tool,
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in *%\\(ext\\)s) T=\"$a\";; esac; done\nF=$(echo \"$T\" | sed 's/%(ext)s/m4a/')\nprintf 12345 > \"$F\"\necho \"$F\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let y = YtDlp::new(Tool::binary(tool));
        let (p, n) = y.download("abc", &out, Quality::Medium).unwrap();
        assert!(p.ends_with("abc.m4a"));
        assert_eq!(n, 5);
    }

    #[test]
    fn related_drops_the_seed() {
        let y = YtDlp::new(fake(&tmp("rel"))).without_ytmusic();
        let v = y.related("5NV6Rdv1a3I", 10).unwrap();
        assert!(v.iter().all(|s| s.id != "5NV6Rdv1a3I"));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn failure_message_and_timeout() {
        let d = tmp("fail");
        let mut t = fake(&d);
        let y = YtDlp::new(t.clone());
        assert_eq!(y.version().unwrap(), "2099.01.01");
        assert!(matches!(y.flat("https://example.com/other", 1), Err(Error::Failed(m)) if m.contains("unexpected")));
        let sleeper = d.join("sleep.sh");
        std::fs::write(&sleeper, "#!/bin/sh\nsleep 5\n").unwrap();
        std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o755)).unwrap();
        t.program = sleeper;
        t.timeout = std::time::Duration::from_millis(200);
        assert!(matches!(YtDlp::new(t).version(), Err(Error::Timeout)));
    }

}

#[cfg(test)]
mod live {
    use super::*;

    /// `HARP_YTDLP=/path/to/yt-dlp cargo test -p harp-ytdlp -- --ignored live_light_client`
    #[test]
    #[ignore]
    fn live_light_client() {
        let mut t = Tool::binary(std::env::var("HARP_YTDLP").expect("HARP_YTDLP"));
        let dir = std::env::temp_dir().join("harp-live-light");
        t.cache_dir = Some(dir.join("ytc"));
        let y = YtDlp::new(t).without_ytmusic();
        let s = y.resolve("dQw4w9WgXcQ", Quality::High).unwrap();
        assert!(s.url.starts_with("https://"));
        let (p, bytes) = y.download("dQw4w9WgXcQ", &dir, Quality::High).unwrap();
        eprintln!("{} ({bytes} bytes)", p.display());
        assert!(bytes > 500_000);
    }
}
