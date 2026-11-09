//! Your own music files as a source: folders you pick are scanned for audio
//! files, their tags (title, artist, album, length, cover) are read, and they
//! play like any other song. Ids look like `local:<hash of the path>`.

use crate::error::Result;
use crate::model::Song;
use crate::taste::Store;
use lofty::prelude::*;
use rusqlite::params;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Audio files we read.
pub const EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "opus", "wav", "wma", "alac", "aiff", "aif", "webm", "mka"];

/// Ids of local songs start with this.
pub const PREFIX: &str = "local:";

pub fn is_local(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// A stable id for a file path (FNV-1a, 64-bit).
pub fn id_for(path: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.to_string_lossy().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{PREFIX}{h:016x}")
}

/// Every audio file under these folders (hidden folders skipped; at most
/// `limit` files, so a mistaken pick of `/` can't run forever).
pub fn scan(dirs: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, u32)> = dirs.iter().map(|d| (d.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if depth < 12 {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| EXTENSIONS.contains(&x.to_ascii_lowercase().as_str())) {
                out.push(p);
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}

/// A file as a song, from its tags; the file name and folder fill in what the
/// tags leave out. The embedded cover, if any, is written into `covers`.
pub fn read(path: &Path, covers: Option<&Path>) -> Song {
    let id = id_for(path);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut song = Song { id: id.clone(), title: stem.clone(), artist: String::new(), artist_id: None, album: None, duration_secs: None, thumbnail: None };
    if let Ok(f) = lofty::read_from_path(path) {
        let secs = f.properties().duration().as_secs();
        if secs > 0 {
            song.duration_secs = Some(secs as u32);
        }
        if let Some(tag) = f.primary_tag().or_else(|| f.first_tag()) {
            if let Some(t) = tag.title().filter(|t| !t.trim().is_empty()) {
                song.title = t.trim().to_string();
            }
            if let Some(a) = tag.artist().filter(|a| !a.trim().is_empty()) {
                song.artist = a.trim().to_string();
            }
            song.album = tag.album().map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
            if let (Some(dir), Some(pic)) = (covers, tag.pictures().first()) {
                let ext = match pic.mime_type() {
                    Some(lofty::picture::MimeType::Png) => "png",
                    _ => "jpg",
                };
                let out = dir.join(format!("{}.{ext}", &id[PREFIX.len()..]));
                if out.is_file() || (std::fs::create_dir_all(dir).is_ok() && std::fs::write(&out, pic.data()).is_ok()) {
                    song.thumbnail = Some(out.to_string_lossy().into_owned());
                }
            }
        }
    }
    // "Artist - Title.mp3" with no tags.
    if song.artist.is_empty() {
        if let Some((a, t)) = stem.split_once(" - ") {
            if song.title == stem {
                song.title = t.trim().to_string();
            }
            song.artist = a.trim().to_string();
        }
    }
    if song.artist.is_empty() {
        song.artist = "Unknown artist".into();
    }
    song
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Store {
    /// The folders you added.
    pub fn local_folders(&self) -> Result<Vec<String>> {
        Ok(self.kv_get("local_dirs")?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
    }

    pub fn set_local_folders(&self, dirs: &[String]) -> Result<()> {
        self.kv_set("local_dirs", &serde_json::to_string(dirs).unwrap_or_default())
    }

    /// Rescans the folders: new and changed files are read, unchanged ones kept,
    /// missing ones dropped. Returns how many songs there are now.
    pub fn local_rescan(&self, covers: Option<&Path>) -> Result<u32> {
        let dirs: Vec<PathBuf> = self.local_folders()?.into_iter().map(PathBuf::from).collect();
        let files = scan(&dirs, 50_000);
        let mut known: HashMap<String, (i64, String)> = HashMap::new();
        {
            let mut st = self.conn.prepare("SELECT path, mtime, song_json FROM local_files")?;
            for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?.flatten() {
                known.insert(r.0, (r.1, r.2));
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM local_files", [])?;
        for p in &files {
            let key = p.to_string_lossy().into_owned();
            let m = mtime(p);
            let json = match known.get(&key) {
                Some((km, j)) if *km == m => j.clone(),
                _ => crate::model::to_json(&read(p, covers)),
            };
            tx.execute(
                "INSERT OR REPLACE INTO local_files(id,path,mtime,song_json) VALUES(?,?,?,?)",
                params![id_for(p), key, m, json],
            )?;
        }
        tx.commit()?;
        Ok(files.len() as u32)
    }

    /// Your local songs, by artist, album, then title.
    pub fn local_songs(&self) -> Result<Vec<Song>> {
        let mut st = self.conn.prepare("SELECT song_json FROM local_files")?;
        let mut v: Vec<Song> = st.query_map([], |r| r.get::<_, String>(0))?.flatten().filter_map(|j| serde_json::from_str(&j).ok()).collect();
        v.sort_by_cached_key(|s| (s.artist.to_lowercase(), s.album.clone().unwrap_or_default().to_lowercase(), s.title.to_lowercase()));
        Ok(v)
    }

    /// The file behind a local song id.
    pub fn local_path(&self, id: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self.conn.query_row("SELECT path FROM local_files WHERE id=?", [id], |r| r.get(0)).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Makes a one-second tagged file with ffmpeg (skips the test without it).
    fn make(path: &Path, tags: &[(&str, &str)]) -> bool {
        let mut c = Command::new("ffmpeg");
        c.args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]);
        for (k, v) in tags {
            c.args(["-metadata", &format!("{k}={v}")]);
        }
        c.arg(path);
        c.status().map(|s| s.success()).unwrap_or(false)
    }

    #[test]
    fn scans_reads_tags_and_rescans() {
        let dir = std::env::temp_dir().join(format!("harp-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Album/.hidden")).unwrap();
        let tagged = dir.join("Album/01 track.mp3");
        let untagged = dir.join("Album/Prateek Kuhad - cold mess.ogg");
        if !make(&tagged, &[("title", "Kasoor"), ("artist", "Prateek Kuhad"), ("album", "cold/mess")]) || !make(&untagged, &[]) {
            eprintln!("ffmpeg not available: skipped");
            return;
        }
        std::fs::write(dir.join("Album/notes.txt"), "x").unwrap();
        make(&dir.join("Album/.hidden/x.mp3"), &[]);

        assert_eq!(scan(&[dir.clone()], 100).len(), 2, "audio only, hidden folders skipped");
        let s = read(&tagged, None);
        assert_eq!((s.title.as_str(), s.artist.as_str(), s.album.as_deref()), ("Kasoor", "Prateek Kuhad", Some("cold/mess")));
        assert_eq!(s.duration_secs, Some(1));
        assert!(is_local(&s.id) && s.id == id_for(&tagged));
        let u = read(&untagged, None);
        assert_eq!((u.title.as_str(), u.artist.as_str()), ("cold mess", "Prateek Kuhad"), "from the file name");

        let st = Store::open_memory().unwrap();
        st.set_local_folders(&[dir.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(st.local_rescan(None).unwrap(), 2);
        assert_eq!(st.local_songs().unwrap().iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["cold mess", "Kasoor"]);
        assert_eq!(st.local_path(&s.id).unwrap().as_deref(), Some(tagged.to_string_lossy().as_ref()));
        std::fs::remove_file(&untagged).unwrap();
        assert_eq!(st.local_rescan(None).unwrap(), 1, "deleted files go");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
