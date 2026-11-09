//! Backing up the whole library (likes, playlists, play history, taste,
//! search history, settings, saved lyrics) to one file, and restoring it.
//!
//! A backup is a compact copy of the SQLite database. Restoring never touches
//! the open database: the file is checked, staged next to it, and swapped in
//! the next time the app starts ([`apply_pending`]), keeping the old one as
//! `harp.db.before-restore`.

use crate::error::{Error, Result};
use crate::taste::Store;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// What a backup holds, shown before restoring.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupInfo {
    pub liked: u32,
    pub playlists: u32,
    pub plays: u32,
}

impl Store {
    /// Writes a compact copy of the library to `path` (which must not exist).
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        self.conn.execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        Ok(())
    }
}

/// Checks that `path` is a Harp library and says what is in it.
pub fn inspect(path: &Path) -> Result<BackupInfo> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| Error::Source("this isn't a Harp backup".into()))?;
    let has = |t: &str| -> bool {
        conn.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1", [t], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    };
    if !["plays", "likes", "playlists", "playlist_items", "kv"].iter().all(|t| has(t)) {
        return Err(Error::Source("this isn't a Harp backup".into()));
    }
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u32;
    Ok(BackupInfo {
        liked: count("SELECT count(*) FROM likes"),
        playlists: count("SELECT count(*) FROM playlists"),
        plays: count("SELECT count(*) FROM plays"),
    })
}

fn pending_path(db: &Path) -> PathBuf {
    db.with_extension("db.restore")
}

/// Checks the backup at `src` and stages it to replace `db` on the next start.
pub fn stage_restore(db: &Path, src: &Path) -> Result<BackupInfo> {
    let info = inspect(src)?;
    let pending = pending_path(db);
    std::fs::copy(src, &pending).map_err(|e| Error::Source(format!("couldn't stage the backup: {e}")))?;
    Ok(info)
}

/// Run before opening the database: swaps in a staged backup, if any. Downloads
/// listed in the backup whose files aren't on this device are dropped.
/// Returns true when a backup was applied.
pub fn apply_pending(db: &Path) -> Result<bool> {
    let pending = pending_path(db);
    if !pending.is_file() {
        return Ok(false);
    }
    if inspect(&pending).is_err() {
        let _ = std::fs::remove_file(&pending);
        return Ok(false);
    }
    let io = |e: std::io::Error| Error::Source(format!("couldn't restore: {e}"));
    if db.is_file() {
        std::fs::rename(db, db.with_extension("db.before-restore")).map_err(io)?;
    }
    for side in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db.with_extension(side));
    }
    std::fs::rename(&pending, db).map_err(io)?;
    let st = Store::open(db)?;
    let gone: Vec<String> = st
        .downloads()?
        .into_iter()
        .filter(|d| !Path::new(&d.path).is_file())
        .map(|d| d.song.id)
        .collect();
    for id in gone {
        st.conn.execute("DELETE FROM downloads WHERE song_id=?1", [id])?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Song;

    fn song(id: &str) -> Song {
        Song { id: id.into(), title: format!("Song {id}"), artist: "A".into(), artist_id: None, album: None, duration_secs: Some(200), thumbnail: None }
    }

    #[test]
    fn backup_restores_on_next_start() {
        let dir = std::env::temp_dir().join(format!("harp-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("harp.db");
        let file = dir.join("backup.harp");
        {
            let st = Store::open(&db).unwrap();
            st.toggle_like(&song("a")).unwrap();
            let pl = st.create_playlist("Road").unwrap();
            st.add_to_playlist(pl, &[song("b")]).unwrap();
            st.backup_to(&file).unwrap();
            // Changes after the backup are what a restore undoes.
            st.toggle_like(&song("c")).unwrap();
        }
        assert_eq!(inspect(&file).unwrap(), BackupInfo { liked: 1, playlists: 1, plays: 0 });
        assert_eq!(stage_restore(&db, &file).unwrap().liked, 1);
        assert!(apply_pending(&db).unwrap());
        let st = Store::open(&db).unwrap();
        assert_eq!(st.liked_songs().unwrap().len(), 1);
        assert_eq!(st.playlists().unwrap()[0].name, "Road");
        assert!(db.with_extension("db.before-restore").is_file());
        assert!(!apply_pending(&db).unwrap(), "applied only once");

        let junk = dir.join("junk.db");
        std::fs::write(&junk, b"not a database").unwrap();
        assert!(inspect(&junk).is_err());
        assert!(stage_restore(&db, &junk).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
