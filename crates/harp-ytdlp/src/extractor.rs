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

/// Stop using a cached URL this long before it expires.
const EXPIRY_MARGIN_SECS: u64 = 300;

pub struct YtDlp {
    tool: Tool,
    cache: Mutex<HashMap<String, Stream>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl YtDlp {
    pub fn new(tool: Tool) -> Self {
        YtDlp { tool, cache: Mutex::new(HashMap::new()) }
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

    /// Search YouTube. (YouTube Music's search page gives yt-dlp only ids and
    /// titles in flat mode: no artist, channel or length. Plain `ytsearch` has
    /// all three, and `tidy` pulls "Artist - Title" apart.)
    pub fn search_songs(&self, query: &str, limit: usize) -> Result<Vec<Song>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        self.flat(&format!("ytsearch{limit}:{q}"), limit)
    }

    /// The best audio stream for a video, cached until shortly before it expires.
    pub fn resolve(&self, id: &str) -> Result<Stream> {
        if let Some(s) = self.cache.lock().unwrap().get(id) {
            if s.expires_at.map_or(true, |e| e > now() + EXPIRY_MARGIN_SECS) {
                return Ok(s.clone());
            }
        }
        let out = self.run(&[
            "--no-playlist".into(),
            "-f".into(),
            "bestaudio[ext=m4a]/bestaudio/best".into(),
            "-J".into(),
            format!("https://www.youtube.com/watch?v={id}"),
        ])?;
        let s = parse::stream_from_info(&out)?;
        self.cache.lock().unwrap().insert(id.to_string(), s.clone());
        Ok(s)
    }

    pub fn forget(&self, id: &str) {
        self.cache.lock().unwrap().remove(id);
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

    /// YouTube's mix for a video ("RD" + id playlist) is its watch-next list.
    fn related(&self, song_id: &str, limit: usize) -> harp_core::Result<Vec<Song>> {
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
        let y = YtDlp::new(fake(&tmp("search")));
        let v = y.search_songs("daft punk", 5).unwrap();
        assert_eq!(v[0].title, "Get Lucky");
    }

    #[test]
    fn resolve_is_cached_until_expiry() {
        let d = tmp("resolve");
        let y = YtDlp::new(fake(&d));
        let a = y.resolve("5NV6Rdv1a3I").unwrap();
        // The fixture's expiry is far in the future, so removing the tool must not matter.
        std::fs::remove_file(d.join("yt-dlp")).unwrap();
        assert_eq!(y.resolve("5NV6Rdv1a3I").unwrap(), a);
        y.forget("5NV6Rdv1a3I");
        assert!(y.resolve("5NV6Rdv1a3I").is_err());
    }

    #[test]
    fn related_drops_the_seed() {
        let y = YtDlp::new(fake(&tmp("rel")));
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
