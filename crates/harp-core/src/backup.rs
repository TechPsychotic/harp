//! Backing up the whole library (likes, playlists, play history, taste,
//! search history, settings, saved lyrics) to one file, and restoring it.
//!
//! A backup is a compact copy of the SQLite database. Restoring **adds** the
//! backup to what is already here ([`Store::merge_from`]); nothing is lost.

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

/// What a merge added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeInfo {
    pub liked: u32,
    pub playlists_new: u32,
    pub playlists_grown: u32,
    pub plays: u32,
}

impl Store {
    /// Adds a backup's library to this one, keeping everything already here:
    ///
    /// * liked songs, followed artists, saved albums, lyrics, searches: added
    ///   when missing;
    /// * playlists: a playlist with the same name gets the songs it doesn't
    ///   have yet (at the end, in the backup's order); others are created;
    /// * play history: plays not already here are added;
    /// * taste: per artist, the stronger of the two scores;
    /// * settings, the queue and your music folders stay as they are here;
    /// * downloads only when their file is on this device.
    ///
    /// Merging the same backup twice adds nothing the second time.
    pub fn merge_from(&self, backup: &Path) -> Result<MergeInfo> {
        inspect(backup)?;
        // Bring an older backup up to today's layout first (on a copy).
        let work = backup.with_extension("merging");
        std::fs::copy(backup, &work).map_err(|e| Error::Source(format!("couldn't read the backup: {e}")))?;
        drop(Store::open(&work)?);
        let r = self.merge_attached(&work);
        let _ = std::fs::remove_file(&work);
        for side in ["merging-wal", "merging-shm"] {
            let _ = std::fs::remove_file(backup.with_extension(side));
        }
        r
    }

    fn merge_attached(&self, work: &Path) -> Result<MergeInfo> {
        let c = &self.conn;
        c.execute("ATTACH DATABASE ?1 AS b", [work.to_string_lossy()])?;
        let r = (|| -> Result<MergeInfo> {
            let tx = c.unchecked_transaction()?;
            let mut info = MergeInfo::default();
            info.liked = tx.execute("INSERT OR IGNORE INTO likes(song_id,song_json,at) SELECT song_id,song_json,at FROM b.likes", [])? as u32;
            info.plays = tx.execute(
                "INSERT INTO plays(song_id,song_json,artist_key,completion,liked,superliked,source,played_at)
                 SELECT song_id,song_json,artist_key,completion,liked,superliked,source,played_at FROM b.plays p
                 WHERE NOT EXISTS (SELECT 1 FROM main.plays m WHERE m.song_id=p.song_id AND m.played_at=p.played_at)",
                [],
            )? as u32;
            tx.execute(
                "INSERT INTO artist_scores(artist_key,name,likes,skips,updated)
                 SELECT artist_key,name,likes,skips,updated FROM b.artist_scores WHERE true
                 ON CONFLICT(artist_key) DO UPDATE SET
                   likes=max(likes, excluded.likes), skips=max(skips, excluded.skips), updated=max(updated, excluded.updated)",
                [],
            )?;
            tx.execute(
                "INSERT INTO skip_reasons(song_id,artist_key,reason,at) SELECT song_id,artist_key,reason,at FROM b.skip_reasons s
                 WHERE NOT EXISTS (SELECT 1 FROM main.skip_reasons m WHERE m.song_id=s.song_id AND m.at=s.at AND m.reason=s.reason)",
                [],
            )?;
            tx.execute("INSERT INTO searches(q,at) SELECT q,at FROM b.searches s WHERE NOT EXISTS (SELECT 1 FROM main.searches m WHERE m.q=s.q)", [])?;
            tx.execute("INSERT OR IGNORE INTO lyrics(song_id,json,at) SELECT song_id,json,at FROM b.lyrics", [])?;
            tx.execute("INSERT OR IGNORE INTO saved(id,kind,title,subtitle,thumbnail,at) SELECT id,kind,title,subtitle,thumbnail,at FROM b.saved", [])?;
            // Downloads: only files that exist here.
            let dls: Vec<(String, String, String, i64, String, i64)> = {
                let mut st = tx.prepare("SELECT song_id,song_json,path,bytes,quality,at FROM b.downloads")?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.flatten().collect();
                v
            };
            for d in dls.into_iter().filter(|d| Path::new(&d.2).is_file()) {
                tx.execute("INSERT OR IGNORE INTO downloads(song_id,song_json,path,bytes,quality,at) VALUES(?,?,?,?,?,?)", rusqlite::params![d.0, d.1, d.2, d.3, d.4, d.5])?;
            }
            // Playlists, matched by name.
            let pls: Vec<(i64, String, i64, i64, i64)> = {
                let mut st = tx.prepare("SELECT id,name,created,pinned,autodl FROM b.playlists ORDER BY id")?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.flatten().collect();
                v
            };
            for (bid, name, created, pinned, autodl) in pls {
                use rusqlite::OptionalExtension;
                let here: Option<i64> = tx.query_row("SELECT id FROM main.playlists WHERE name=?1 ORDER BY id LIMIT 1", [&name], |r| r.get(0)).optional()?;
                let id = match here {
                    Some(id) => id,
                    None => {
                        tx.execute("INSERT INTO main.playlists(name,created,pinned,autodl) VALUES(?,?,?,?)", rusqlite::params![name, created, pinned, autodl])?;
                        info.playlists_new += 1;
                        tx.last_insert_rowid()
                    }
                };
                let added = tx.execute(
                    "INSERT INTO main.playlist_items(playlist_id,pos,song_id,song_json)
                     SELECT ?1, (SELECT COALESCE(MAX(pos),-1) FROM main.playlist_items WHERE playlist_id=?1) + ROW_NUMBER() OVER (ORDER BY i.pos),
                            i.song_id, i.song_json
                     FROM b.playlist_items i
                     WHERE i.playlist_id=?2 AND NOT EXISTS (SELECT 1 FROM main.playlist_items m WHERE m.playlist_id=?1 AND m.song_id=i.song_id)
                     GROUP BY i.song_id",
                    rusqlite::params![id, bid],
                )?;
                if here.is_some() && added > 0 {
                    info.playlists_grown += 1;
                }
            }
            tx.commit()?;
            Ok(info)
        })();
        c.execute("DETACH DATABASE b", [])?;
        r
    }
}

fn pending_path(db: &Path) -> PathBuf {
    db.with_extension("db.restore")
}

/// Older builds staged a backup to replace the library at the next start.
/// Restoring now adds instead, so a backup left staged is merged in, not swapped.
pub fn apply_pending(db: &Path) -> Result<bool> {
    let pending = pending_path(db);
    if !pending.is_file() {
        return Ok(false);
    }
    let r = Store::open(db).and_then(|st| st.merge_from(&pending));
    let _ = std::fs::remove_file(&pending);
    r.map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Song;

    fn song(id: &str) -> Song {
        Song { id: id.into(), title: format!("Song {id}"), artist: "A".into(), artist_id: None, album: None, duration_secs: Some(200), thumbnail: None }
    }

    #[test]
    fn restoring_adds_to_what_is_here() {
        let dir = std::env::temp_dir().join(format!("harp-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("backup.harp");
        {
            // The other device: likes a and b, playlist "Road" with b and c, "Gym" with d.
            let other = Store::open(dir.join("other.db")).unwrap();
            other.toggle_like(&song("a")).unwrap();
            other.toggle_like(&song("b")).unwrap();
            let road = other.create_playlist("Road").unwrap();
            other.add_to_playlist(road, &[song("b"), song("c")]).unwrap();
            let gym = other.create_playlist("Gym").unwrap();
            other.add_to_playlist(gym, &[song("d")]).unwrap();
            other.backup_to(&file).unwrap();
        }
        assert_eq!(inspect(&file).unwrap(), BackupInfo { liked: 2, playlists: 2, plays: 0 });

        // This device: likes a and x, playlist "Road" with a and b.
        let st = Store::open(dir.join("harp.db")).unwrap();
        st.toggle_like(&song("a")).unwrap();
        st.toggle_like(&song("x")).unwrap();
        let road = st.create_playlist("Road").unwrap();
        st.add_to_playlist(road, &[song("a"), song("b")]).unwrap();

        let info = st.merge_from(&file).unwrap();
        assert_eq!(info, MergeInfo { liked: 1, playlists_new: 1, playlists_grown: 1, plays: 0 });
        let mut liked: Vec<String> = st.liked_songs().unwrap().into_iter().map(|s| s.id).collect();
        liked.sort();
        assert_eq!(liked, ["a", "b", "x"], "nothing here was lost");
        let ids = |id| st.playlist_songs(id).unwrap().into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(road), ["a", "b", "c"], "same-name playlist grows at the end, no duplicates");
        let gym = st.playlists().unwrap().into_iter().find(|p| p.name == "Gym").unwrap();
        assert_eq!(ids(gym.id), ["d"]);
        // Twice is the same as once.
        assert_eq!(st.merge_from(&file).unwrap(), MergeInfo::default());
        assert_eq!(st.playlists().unwrap().len(), 2);

        let junk = dir.join("junk.db");
        std::fs::write(&junk, b"not a database").unwrap();
        assert!(inspect(&junk).is_err());
        assert!(st.merge_from(&junk).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
