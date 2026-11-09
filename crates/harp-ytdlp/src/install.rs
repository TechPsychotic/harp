//! Keeps yt-dlp installed and fresh: downloads the official release asset and
//! checks it against the release's `SHA2-256SUMS` before using it.
//! (Verifying the sums file's own GPG signature is a TODO; today it relies on
//! HTTPS from github.com.)

use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Which release asset to use on this platform. `zipapp` is the Python zipapp
/// (used on Android, where Python is bundled).
pub fn asset_name(os: &str, arch: &str, zipapp: bool) -> &'static str {
    if zipapp {
        return "yt-dlp";
    }
    match (os, arch) {
        ("windows", "aarch64") => "yt-dlp_arm64.exe",
        ("windows", _) => "yt-dlp.exe",
        ("macos", _) => "yt-dlp_macos",
        ("linux", "aarch64") => "yt-dlp_linux_aarch64",
        _ => "yt-dlp_linux",
    }
}

/// Finds `asset`'s hash in a `SHA2-256SUMS` file ("<hex>  <name>").
pub fn find_sum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (h, n) = l.split_once(char::is_whitespace)?;
        (n.trim().trim_start_matches('*') == asset).then(|| h.to_lowercase())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn get(url: &str) -> Result<Vec<u8>> {
    // A time limit, so a GitHub outage cannot hang the app's first start.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| Error::Download(e.to_string()))?;
    let mut buf = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(80 << 20)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Download(e.to_string()))?;
    Ok(buf)
}

fn is_fresh(p: &Path) -> bool {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map_or(false, |age| age < MAX_AGE)
}

/// Makes sure `dir/<asset>` exists and is less than a week old, downloading
/// and verifying it if not. If a refresh fails, an older copy is still used.
/// `force` skips the age check ("update now").
pub fn ensure(dir: &Path, asset: &str, force: bool) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(asset);
    if dest.exists() && !force && is_fresh(&dest) {
        return Ok(dest);
    }
    match download(&dest, asset) {
        Ok(()) => Ok(dest),
        Err(_) if dest.exists() => Ok(dest),
        Err(e) => Err(e),
    }
}

fn download(dest: &Path, asset: &str) -> Result<()> {
    let sums = String::from_utf8(get(&format!("{BASE}/SHA2-256SUMS"))?).map_err(|e| Error::Download(e.to_string()))?;
    let want = find_sum(&sums, asset).ok_or_else(|| Error::Download(format!("no checksum for {asset}")))?;
    let data = get(&format!("{BASE}/{asset}"))?;
    if sha256_hex(&data) != want {
        return Err(Error::Download(format!("{asset} does not match its published SHA-256")));
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_per_platform() {
        assert_eq!(asset_name("linux", "x86_64", false), "yt-dlp_linux");
        assert_eq!(asset_name("linux", "aarch64", false), "yt-dlp_linux_aarch64");
        assert_eq!(asset_name("windows", "x86_64", false), "yt-dlp.exe");
        assert_eq!(asset_name("macos", "aarch64", false), "yt-dlp_macos");
        assert_eq!(asset_name("android", "aarch64", true), "yt-dlp");
    }

    #[test]
    fn sums_lookup_is_exact() {
        let sums = "AAAA  yt-dlp_linux_aarch64\nbbbb  yt-dlp_linux\ncccc *yt-dlp.exe\n";
        assert_eq!(find_sum(sums, "yt-dlp_linux").as_deref(), Some("bbbb"));
        assert_eq!(find_sum(sums, "yt-dlp_linux_aarch64").as_deref(), Some("aaaa"));
        assert_eq!(find_sum(sums, "yt-dlp.exe").as_deref(), Some("cccc"));
        assert_eq!(find_sum(sums, "yt-dlp"), None);
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
