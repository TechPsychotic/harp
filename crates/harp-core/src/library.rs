//! Playlists and offline downloads, in the same SQLite file as the taste model.

use crate::error::Result;
use crate::model::Song;
use crate::taste::Store;
use rusqlite::params;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistInfo {
    pub id: i64,
    pub name: String,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub song: Song,
    pub path: String,
    pub bytes: u64,
    pub quality: String,
}

fn from_json(s: String) -> Option<Song> {
    crate::model::from_json(&s)
}

impl Store {
    // --- playlists ---

    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        let name = if name.is_empty() { "New playlist" } else { name };
        self.conn.execute("INSERT INTO playlists(name) VALUES(?)", [name])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET name=? WHERE id=?", params![name.trim(), id])?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM playlists WHERE id=?", [id])?;
        Ok(())
    }

    pub fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        let mut st = self.conn.prepare(
            "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id=p.id)
             FROM playlists p ORDER BY p.created DESC, p.id DESC",
        )?;
        let rows = st.query_map([], |r| Ok(PlaylistInfo { id: r.get(0)?, name: r.get(1)?, count: r.get(2)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn playlist_songs(&self, id: i64) -> Result<Vec<Song>> {
        let mut st = self
            .conn
            .prepare("SELECT song_json FROM playlist_items WHERE playlist_id=? ORDER BY pos")?;
        let rows = st.query_map([id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(from_json).collect())
    }

    /// Appends songs; a song already in the playlist is skipped. Returns how many were added.
    pub fn add_to_playlist(&self, id: i64, songs: &[Song]) -> Result<u32> {
        let mut pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(pos)+1, 0) FROM playlist_items WHERE playlist_id=?",
            [id],
            |r| r.get(0),
        )?;
        let mut added = 0;
        for s in songs {
            let dup = self.conn.query_row(
                "SELECT 1 FROM playlist_items WHERE playlist_id=? AND song_id=?",
                params![id, s.id],
                |_| Ok(()),
            );
            if dup.is_ok() {
                continue;
            }
            self.conn.execute(
                "INSERT INTO playlist_items(playlist_id,pos,song_id,song_json) VALUES(?,?,?,?)",
                params![id, pos, s.id, crate::model::to_json(s)],
            )?;
            pos += 1;
            added += 1;
        }
        Ok(added)
    }

    fn renumber(&self, id: i64) -> Result<()> {
        let ids: Vec<i64> = {
            let mut st = self.conn.prepare("SELECT id FROM playlist_items WHERE playlist_id=? ORDER BY pos")?;
            let rows = st.query_map([id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (i, row) in ids.iter().enumerate() {
            self.conn.execute("UPDATE playlist_items SET pos=? WHERE id=?", params![i as i64, row])?;
        }
        Ok(())
    }

    pub fn remove_from_playlist(&self, id: i64, index: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM playlist_items WHERE id=(SELECT id FROM playlist_items WHERE playlist_id=? ORDER BY pos LIMIT 1 OFFSET ?)",
            params![id, index as i64],
        )?;
        self.renumber(id)
    }

    pub fn move_in_playlist(&self, id: i64, from: usize, to: usize) -> Result<()> {
        let mut rows: Vec<i64> = {
            let mut st = self.conn.prepare("SELECT id FROM playlist_items WHERE playlist_id=? ORDER BY pos")?;
            let r = st.query_map([id], |r| r.get(0))?;
            r.collect::<std::result::Result<_, _>>()?
        };
        if from >= rows.len() || to >= rows.len() {
            return Ok(());
        }
        let x = rows.remove(from);
        rows.insert(to, x);
        for (i, row) in rows.iter().enumerate() {
            self.conn.execute("UPDATE playlist_items SET pos=? WHERE id=?", params![i as i64, row])?;
        }
        Ok(())
    }

    // --- search history ---

    /// Remembers a search (newest first, no repeats).
    pub fn add_search(&self, q: &str) -> Result<()> {
        let q = q.trim();
        if q.len() < 2 {
            return Ok(());
        }
        self.conn.execute("DELETE FROM searches WHERE lower(q)=lower(?)", [q])?;
        self.conn.execute("INSERT INTO searches(q) VALUES(?)", [q])?;
        self.conn.execute("DELETE FROM searches WHERE id NOT IN (SELECT id FROM searches ORDER BY id DESC LIMIT 200)", [])?;
        Ok(())
    }

    pub fn recent_searches(&self, n: usize) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT q FROM searches ORDER BY id DESC LIMIT ?")?;
        let rows = st.query_map([n as i64], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn remove_search(&self, q: &str) -> Result<()> {
        self.conn.execute("DELETE FROM searches WHERE lower(q)=lower(?)", [q.trim()])?;
        Ok(())
    }

    pub fn clear_searches(&self) -> Result<()> {
        self.conn.execute("DELETE FROM searches", [])?;
        Ok(())
    }

    // --- lyrics cache ---

    /// A saved result and when it was saved (an empty `Lyrics` means "none found").
    pub fn cached_lyrics(&self, id: &str) -> Result<Option<(crate::lyrics::Lyrics, i64)>> {
        use rusqlite::OptionalExtension;
        let row: Option<(String, i64)> = self
            .conn
            .query_row("SELECT json, at FROM lyrics WHERE song_id=?", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        Ok(row.and_then(|(j, at)| serde_json::from_str(&j).ok().map(|l| (l, at))))
    }

    pub fn save_lyrics(&self, id: &str, l: &crate::lyrics::Lyrics) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO lyrics(song_id,json) VALUES(?,?)",
            params![id, serde_json::to_string(l).unwrap_or_default()],
        )?;
        Ok(())
    }

    // --- downloads ---

    pub fn record_download(&self, song: &Song, path: &str, bytes: u64, quality: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO downloads(song_id,song_json,path,bytes,quality) VALUES(?,?,?,?,?)",
            params![song.id, crate::model::to_json(song), path, bytes as i64, quality],
        )?;
        Ok(())
    }

    pub fn downloaded_path(&self, id: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT path FROM downloads WHERE song_id=?", [id], |r| r.get(0))
            .optional()?)
    }

    pub fn downloads(&self) -> Result<Vec<Download>> {
        let mut st = self
            .conn
            .prepare("SELECT song_json,path,bytes,quality FROM downloads ORDER BY at DESC, rowid DESC")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|(j, path, bytes, quality)| {
                from_json(j).map(|song| Download { song, path, bytes: bytes as u64, quality })
            })
            .collect())
    }

    /// Forgets a download; returns its file path so the caller can delete the file.
    pub fn delete_download(&self, id: &str) -> Result<Option<String>> {
        let p = self.downloaded_path(id)?;
        self.conn.execute("DELETE FROM downloads WHERE song_id=?", [id])?;
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::song;

    fn ids(v: &[Song]) -> Vec<&str> {
        v.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn playlist_lifecycle() {
        let st = Store::open_memory().unwrap();
        let id = st.create_playlist("  Road trip ").unwrap();
        let (a, b, c) = (song("a", "a", "X"), song("b", "b", "X"), song("c", "c", "Y"));
        assert_eq!(st.add_to_playlist(id, &[a.clone(), b.clone(), c.clone(), a.clone()]).unwrap(), 3);
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["a", "b", "c"]);
        st.move_in_playlist(id, 2, 0).unwrap();
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["c", "a", "b"]);
        st.remove_from_playlist(id, 1).unwrap();
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["c", "b"]);
        // appending after removals keeps order
        st.add_to_playlist(id, &[song("d", "d", "Z")]).unwrap();
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["c", "b", "d"]);
        st.rename_playlist(id, "Trips").unwrap();
        let p = st.playlists().unwrap();
        assert_eq!(p, vec![PlaylistInfo { id, name: "Trips".into(), count: 3 }]);
        st.delete_playlist(id).unwrap();
        assert!(st.playlists().unwrap().is_empty());
        assert!(st.playlist_songs(id).unwrap().is_empty(), "items go with the playlist");
    }

    #[test]
    fn search_history_is_recent_first_unique_and_clearable() {
        let st = Store::open_memory().unwrap();
        for q in ["arijit", "daft punk", "x", "Arijit", "  "] {
            st.add_search(q).unwrap();
        }
        assert_eq!(st.recent_searches(10).unwrap(), ["Arijit", "daft punk"], "short ones skipped, repeats move up");
        st.remove_search("DAFT PUNK").unwrap();
        assert_eq!(st.recent_searches(10).unwrap(), ["Arijit"]);
        st.clear_searches().unwrap();
        assert!(st.recent_searches(10).unwrap().is_empty());
    }

    #[test]
    fn lyrics_cache_roundtrip_including_none() {
        let st = Store::open_memory().unwrap();
        assert!(st.cached_lyrics("a").unwrap().is_none());
        let l = crate::lyrics::from_texts(Some("[00:01.00]hi"), None, "lrclib");
        st.save_lyrics("a", &l).unwrap();
        assert_eq!(st.cached_lyrics("a").unwrap().unwrap().0, l);
        st.save_lyrics("b", &crate::lyrics::Lyrics::default()).unwrap();
        assert!(st.cached_lyrics("b").unwrap().unwrap().0.is_empty());
    }

    #[test]
    fn blank_name_gets_a_default() {
        let st = Store::open_memory().unwrap();
        st.create_playlist("   ").unwrap();
        assert_eq!(st.playlists().unwrap()[0].name, "New playlist");
    }

    #[test]
    fn downloads_roundtrip() {
        let st = Store::open_memory().unwrap();
        let s = song("a", "A", "X");
        assert_eq!(st.downloaded_path("a").unwrap(), None);
        st.record_download(&s, "/music/a.m4a", 1234, "high").unwrap();
        assert_eq!(st.downloaded_path("a").unwrap().as_deref(), Some("/music/a.m4a"));
        let d = st.downloads().unwrap();
        assert_eq!((d[0].bytes, d[0].quality.as_str()), (1234, "high"));
        assert_eq!(st.delete_download("a").unwrap().as_deref(), Some("/music/a.m4a"));
        assert!(st.downloads().unwrap().is_empty());
    }
}
