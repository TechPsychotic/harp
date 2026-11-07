//! A small on-disk cache for cover art, so thumbnails load instantly, can be
//! precached ahead of time, and still show offline.

use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_BYTES: u64 = 3 << 20;

/// Where a URL's image lives in the cache.
pub fn path_for(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{}.img", &hex::encode(Sha256::digest(url.as_bytes()))[..24]))
}

/// Returns the cached file for `url`, downloading it first if needed.
pub fn fetch(dir: &Path, url: &str) -> Result<PathBuf> {
    let p = path_for(dir, url);
    if p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(p);
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(Error::Download("not an http(s) url".into()));
    }
    std::fs::create_dir_all(dir)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| Error::Download(e.to_string()))?;
    let mut buf = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Download(e.to_string()))?;
    if buf.is_empty() {
        return Err(Error::Download("empty image".into()));
    }
    let tmp = p.with_extension("part");
    std::fs::write(&tmp, &buf)?;
    std::fs::rename(&tmp, &p)?;
    Ok(p)
}

/// Total size of the cache in bytes.
pub fn size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|d| d.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

/// Empties the cache.
pub fn clear(dir: &Path) {
    if let Ok(d) = std::fs::read_dir(dir) {
        for e in d.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// GET a page as text (used for public playlist pages).
pub fn get_text(url: &str) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0")
        .call()
        .map_err(|e| Error::Download(e.to_string()))?;
    resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_distinct_names_and_cache_hit() {
        let d = std::env::temp_dir().join(format!("harp-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (path_for(&d, "https://x/a.jpg"), path_for(&d, "https://x/b.jpg"));
        assert_eq!(a, path_for(&d, "https://x/a.jpg"));
        assert_ne!(a, b);
        std::fs::write(&a, b"img").unwrap();
        // A cached file is returned without any network access.
        assert_eq!(fetch(&d, "https://x/a.jpg").unwrap(), a);
        assert_eq!(size(&d), 3);
        assert!(fetch(&d, "file:///etc/passwd").is_err());
        clear(&d);
        assert_eq!(size(&d), 0);
    }
}
