//! The play cache: songs fetched ahead of time (or while playing) into a
//! folder with a size limit, so skipping, replaying and going back are instant,
//! and the next songs still play if the connection or the screen goes to sleep.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A finished cached file for a song id (`<id>.<ext>`, never a `.part`).
pub fn find(dir: &Path, id: &str) -> Option<PathBuf> {
    let prefix = format!("{id}.");
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(&prefix) && !n.ends_with(".part") && !n.ends_with(".ytdl"))
            && p.metadata().map(|m| m.len() > 0).unwrap_or(false)
    })
}

/// Marks a file as just used, so it is the last to be trimmed (access times are
/// unreliable on many systems, so the modified time stands in for "last used").
pub fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::options().write(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

pub fn total_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|d| d.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

/// Deletes the least recently used files until the folder fits in `max_bytes`.
/// `keep` ids (what is playing or about to) are never deleted. Returns how many were removed.
pub fn trim(dir: &Path, max_bytes: u64, keep: &[String]) -> usize {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    m.is_file().then(|| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    let mut removed = 0;
    for (_, size, path) in files {
        if total <= max_bytes {
            break;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if keep.iter().any(|k| name.starts_with(&format!("{k}."))) {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            removed += 1;
        }
    }
    removed
}

/// Deletes every finished cached song except the `keep` ids (what just played,
/// what plays now and what's coming). Files still being fetched are left alone.
pub fn retain(dir: &Path, keep: &[String]) -> usize {
    let Ok(d) = std::fs::read_dir(dir) else { return 0 };
    let mut removed = 0;
    for e in d.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".part") || name.ends_with(".ytdl") || keep.iter().any(|k| name.starts_with(&format!("{k}."))) {
            continue;
        }
        if e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn clear(dir: &Path) {
    if let Ok(d) = std::fs::read_dir(dir) {
        for e in d.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tmp(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("harp-cache-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(d: &Path, name: &str, bytes: usize, age_secs: u64) {
        let p = d.join(name);
        std::fs::write(&p, vec![1u8; bytes]).unwrap();
        let t = SystemTime::now() - Duration::from_secs(age_secs);
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn finds_finished_files_only() {
        let d = tmp("find");
        write(&d, "abc.m4a", 10, 0);
        write(&d, "def.m4a.part", 10, 0);
        write(&d, "empty.m4a", 0, 0);
        assert_eq!(find(&d, "abc").unwrap().file_name().unwrap(), "abc.m4a");
        assert!(find(&d, "def").is_none());
        assert!(find(&d, "empty").is_none());
        assert!(find(&d, "ab").is_none(), "prefix must end at the dot");
        assert_eq!(total_bytes(&d), 20);
    }

    #[test]
    fn using_a_file_protects_it_from_trimming() {
        let d = tmp("touch");
        write(&d, "a.m4a", 100, 500);
        write(&d, "b.m4a", 100, 400);
        touch(&d.join("a.m4a"));
        assert_eq!(trim(&d, 100, &[]), 1);
        assert!(find(&d, "a").is_some() && find(&d, "b").is_none(), "b was now the least recently used");
    }

    #[test]
    fn trims_oldest_first_and_keeps_what_is_needed() {
        let d = tmp("trim");
        write(&d, "old.m4a", 100, 300);
        write(&d, "mid.m4a", 100, 200);
        write(&d, "new.m4a", 100, 100);
        assert_eq!(trim(&d, 250, &[]), 1);
        assert!(find(&d, "old").is_none() && find(&d, "mid").is_some());
        let d = tmp("trim2");
        write(&d, "old.m4a", 100, 300);
        write(&d, "new.m4a", 100, 100);
        assert_eq!(trim(&d, 100, &["old".to_string()]), 1, "old is kept, so new goes");
        assert!(find(&d, "old").is_some() && find(&d, "new").is_none());
        clear(&d);
        assert_eq!(total_bytes(&d), 0);
    }

    #[test]
    fn retain_keeps_only_what_is_needed() {
        let d = tmp("retain");
        write(&d, "prev.m4a", 10, 0);
        write(&d, "cur.webm", 10, 0);
        write(&d, "gone.m4a", 10, 0);
        write(&d, "fetching.m4a.part", 10, 0);
        assert_eq!(retain(&d, &["prev".into(), "cur".into(), "next".into()]), 1);
        assert!(find(&d, "gone").is_none() && find(&d, "prev").is_some() && find(&d, "cur").is_some());
        assert!(d.join("fetching.m4a.part").exists(), "a fetch in progress is not touched");
    }
}
