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
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl YtDlp {
    pub fn new(tool: Tool) -> Self {
        YtDlp { tool, cache: Mutex::new(HashMap::new()), ytmusic: true }
    }

    /// Use only yt-dlp, never YouTube Music's web API (tests, or if it breaks).
    pub fn without_ytmusic(mut self) -> Self {
        self.ytmusic = false;
        self
    }

    fn run(&self, extra: &[String]) -> Result<String> {
        let mut cmd = Command::new(&self.tool.program);
        cmd.args(self.tool.base_args()).args(extra);
        for (k, v) in &self.tool.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
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
        let out = self.run(&[
            "--no-playlist".into(),
            "-f".into(),
            q.format().into(),
            "-J".into(),
            format!("https://www.youtube.com/watch?v={id}"),
        ])?;
        let s = parse::stream_from_info(&out)?;
        self.cache.lock().unwrap().insert(key, s.clone());
        Ok(s)
    }

    /// Downloads a song's audio into `dir` for offline playback. Returns the
    /// file path and its size.
    pub fn download(&self, id: &str, dir: &std::path::Path, q: Quality) -> Result<(std::path::PathBuf, u64)> {
        std::fs::create_dir_all(dir)?;
        let template = dir.join(format!("{id}.%(ext)s"));
        let out = self.run(&[
            "--no-playlist".into(),
            "--no-part".into(),
            "-f".into(),
            q.format().into(),
            "-o".into(),
            template.to_string_lossy().into_owned(),
            "--print".into(),
            "after_move:filepath".into(),
            "--no-simulate".into(),
            format!("https://www.youtube.com/watch?v={id}"),
        ])?;
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
