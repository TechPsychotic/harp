use std::path::PathBuf;
use std::time::Duration;

/// How to launch yt-dlp.
#[derive(Clone, Debug)]
pub struct Tool {
    /// The program: the yt-dlp binary, or Python.
    pub program: PathBuf,
    /// Arguments before yt-dlp's own (the zipapp path when `program` is Python).
    pub prefix_args: Vec<String>,
    /// Environment for the child (PYTHONHOME, SSL_CERT_FILE, PATH...).
    pub env: Vec<(String, String)>,
    /// `--proxy` URL, e.g. the loopback proxy on Android.
    pub proxy: Option<String>,
    /// A JavaScript runtime for YouTube's player challenges: `quickjs:/path/to/qjs`.
    pub js_runtime: Option<String>,
    /// yt-dlp's own cache (YouTube's player code and solved challenges), kept
    /// between songs so each one doesn't fetch and solve them again. None: no cache.
    pub cache_dir: Option<PathBuf>,
    pub timeout: Duration,
}

impl Tool {
    /// A plain yt-dlp executable (desktop).
    pub fn binary(path: impl Into<PathBuf>) -> Self {
        Tool {
            program: path.into(),
            prefix_args: vec![],
            env: vec![],
            proxy: None,
            js_runtime: None,
            cache_dir: None,
            timeout: Duration::from_secs(60),
        }
    }

    /// yt-dlp as a zipapp run by `python` (Android).
    pub fn zipapp(python: impl Into<PathBuf>, zipapp: impl Into<PathBuf>) -> Self {
        let mut t = Tool::binary(python);
        t.prefix_args = vec![zipapp.into().to_string_lossy().into_owned()];
        t
    }

    pub(crate) fn base_args(&self) -> Vec<String> {
        let mut a = self.prefix_args.clone();
        a.extend(["--ignore-config", "--no-warnings", "--encoding", "utf-8", "--socket-timeout", "30"].map(String::from));
        match &self.cache_dir {
            Some(d) => {
                a.push("--cache-dir".into());
                a.push(d.to_string_lossy().into_owned());
            }
            None => a.push("--no-cache-dir".into()),
        }
        if let Some(js) = &self.js_runtime {
            a.push("--js-runtimes".into());
            a.push(js.clone());
        }
        if let Some(p) = &self.proxy {
            a.push("--proxy".into());
            a.push(p.clone());
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zipapp_args_come_first() {
        let mut t = Tool::zipapp("/lib/python3", "/files/yt-dlp");
        t.proxy = Some("http://u:p@127.0.0.1:1".into());
        t.js_runtime = Some("quickjs:/lib/qjs".into());
        let a = t.base_args();
        assert_eq!(a[0], "/files/yt-dlp");
        assert!(a.windows(2).any(|w| w == ["--proxy", "http://u:p@127.0.0.1:1"]));
        assert!(a.windows(2).any(|w| w == ["--js-runtimes", "quickjs:/lib/qjs"]));
        assert!(a.contains(&"--ignore-config".to_string()));
    }
}
