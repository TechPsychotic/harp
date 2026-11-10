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
    pub pinned: bool,
    /// New songs get downloaded (set by "Download All").
    pub auto_download: bool,
    /// A picture you chose (a file path); empty: the mosaic of its songs.
    pub cover: String,
    /// The folder it's in ("" for none).
    pub folder: String,
    /// The link it follows (a friend's shared playlist), or "".
    pub remote: String,
    /// Artwork of the first (up to) four songs, for a cover mosaic.
    pub covers: Vec<String>,
}

/// An artist you follow, or an album, playlist or podcast you saved to the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: String,
    /// "artist", "album", "playlist" or "podcast"
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
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
            "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id=p.id), p.pinned, p.autodl, p.cover, p.folder, p.remote
             FROM playlists p ORDER BY p.pinned DESC, p.created DESC, p.id DESC",
        )?;
        let rows = st.query_map([], |r| {
            Ok(PlaylistInfo { id: r.get(0)?, name: r.get(1)?, count: r.get(2)?, pinned: r.get::<_, i64>(3)? != 0, auto_download: r.get::<_, i64>(4)? != 0, cover: r.get(5)?, folder: r.get(6)?, remote: r.get(7)?, covers: vec![] })
        })?;
        let mut out: Vec<PlaylistInfo> = rows.collect::<std::result::Result<_, _>>()?;
        let mut cov = self.conn.prepare("SELECT song_json FROM playlist_items WHERE playlist_id=? ORDER BY pos LIMIT 4")?;
        for p in &mut out {
            let songs = cov.query_map([p.id], |r| r.get::<_, String>(0))?;
            p.covers = songs.filter_map(|s| s.ok()).filter_map(from_json).map(|s| s.thumbnail_or_default()).collect();
        }
        Ok(out)
    }

    pub fn pin_playlist(&self, id: i64, pinned: bool) -> Result<()> {
        self.conn.execute("UPDATE playlists SET pinned=? WHERE id=?", params![pinned as i64, id])?;
        Ok(())
    }

    /// A random handful of songs from your playlists, with the playlist each came from:
    /// what you saved says what you like, so these seed recommendations too.
    pub fn playlist_seeds(&self, n: usize) -> Result<Vec<(Song, String)>> {
        let mut st = self.conn.prepare(
            "SELECT i.song_json, p.name FROM playlist_items i JOIN playlists p ON p.id=i.playlist_id
             ORDER BY random() LIMIT ?",
        )?;
        let rows = st.query_map([n as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|(j, name)| from_json(j).map(|s| (s, name))).collect())
    }

    pub fn playlist_songs(&self, id: i64) -> Result<Vec<Song>> {
        let mut st = self
            .conn
            .prepare("SELECT song_json FROM playlist_items WHERE playlist_id=? ORDER BY pos")?;
        let rows = st.query_map([id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(from_json).collect())
    }

    /// Appends songs; a song already in the playlist is skipped. Returns how many were added.
    /// Adds songs at the **top** of the playlist (newest first, like a feed),
    /// keeping the order they're given in. Songs already in it are skipped.
    /// Returns how many were added.
    pub fn add_to_playlist(&self, id: i64, songs: &[Song]) -> Result<u32> {
        let mut fresh: Vec<&Song> = Vec::new();
        for s in songs {
            let dup = self.conn.query_row(
                "SELECT 1 FROM playlist_items WHERE playlist_id=? AND song_id=?",
                params![id, s.id],
                |_| Ok(()),
            );
            if dup.is_err() && !fresh.iter().any(|f| f.id == s.id) {
                fresh.push(s);
            }
        }
        if fresh.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE playlist_items SET pos = pos + ? WHERE playlist_id=?", params![fresh.len() as i64, id])?;
        for (i, s) in fresh.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_items(playlist_id,pos,song_id,song_json) VALUES(?,?,?,?)",
                params![id, i as i64, s.id, crate::model::to_json(s)],
            )?;
        }
        tx.commit()?;
        Ok(fresh.len() as u32)
    }

    /// Sets (or clears, with "") a playlist's own cover picture.
    pub fn set_playlist_cover(&self, id: i64, path: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET cover=? WHERE id=?", params![path, id])?;
        Ok(())
    }

    /// Marks a playlist as following a shared link.
    pub fn set_playlist_remote(&self, id: i64, link: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET remote=? WHERE id=?", params![link, id])?;
        Ok(())
    }

    /// Replaces a playlist's songs (a followed playlist updating). Returns
    /// whether anything changed.
    pub fn replace_playlist_songs(&self, id: i64, songs: &[Song]) -> Result<bool> {
        let now: Vec<String> = self.playlist_songs(id)?.into_iter().map(|s| s.id).collect();
        if now == songs.iter().map(|s| s.id.clone()).collect::<Vec<_>>() {
            return Ok(false);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_items WHERE playlist_id=?", [id])?;
        for (i, s) in songs.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_items(playlist_id,pos,song_id,song_json) VALUES(?,?,?,?)",
                params![id, i as i64, s.id, crate::model::to_json(s)],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// Puts a playlist in a folder ("" takes it out).
    pub fn set_playlist_folder(&self, id: i64, folder: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET folder=? WHERE id=?", params![folder.trim(), id])?;
        Ok(())
    }

    /// Renames a folder (every playlist in it moves along).
    pub fn rename_folder(&self, old: &str, new: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET folder=? WHERE folder=?", params![new.trim(), old])?;
        Ok(())
    }

    /// The playlists a song is in (for the ✓ "saved in" sheet).
    pub fn playlists_with_song(&self, song_id: &str) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare("SELECT DISTINCT playlist_id FROM playlist_items WHERE song_id=? ORDER BY playlist_id")?;
        let rows = st.query_map([song_id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Every song that's in at least one playlist (song rows show a ✓).
    pub fn playlisted_ids(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT DISTINCT song_id FROM playlist_items")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Takes a song out of a playlist by its id (wherever it is in the order).
    pub fn remove_song_from_playlist(&self, id: i64, song_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM playlist_items WHERE playlist_id=? AND song_id=?", params![id, song_id])?;
        Ok(())
    }

    /// "Download All" was chosen: songs added later get downloaded too.
    pub fn set_playlist_auto_download(&self, id: i64, on: bool) -> Result<()> {
        self.conn.execute("UPDATE playlists SET autodl=? WHERE id=?", params![on as i64, id])?;
        Ok(())
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

    // --- followed artists and saved albums ---

    /// Follows / saves (`on`) or forgets an artist, album, playlist or podcast.
    pub fn set_saved(&self, item: &Saved, on: bool) -> Result<()> {
        if on {
            self.conn.execute(
                "INSERT INTO saved(id,kind,title,subtitle,thumbnail) VALUES(?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET title=excluded.title, subtitle=excluded.subtitle,
                 thumbnail=CASE WHEN excluded.thumbnail='' THEN saved.thumbnail ELSE excluded.thumbnail END",
                params![item.id, item.kind, item.title, item.subtitle, item.thumbnail],
            )?;
        } else {
            self.conn.execute("DELETE FROM saved WHERE id=?", [&item.id])?;
        }
        Ok(())
    }

    pub fn is_saved(&self, id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT count(*) FROM saved WHERE id=?", [id], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Saved items of these kinds (all kinds when empty), newest first.
    pub fn saved(&self, kinds: &[&str]) -> Result<Vec<Saved>> {
        let mut st = self.conn.prepare("SELECT id,kind,title,subtitle,thumbnail FROM saved ORDER BY at DESC, rowid DESC")?;
        let rows = st.query_map([], |r| {
            Ok(Saved { id: r.get(0)?, kind: r.get(1)?, title: r.get(2)?, subtitle: r.get(3)?, thumbnail: r.get(4)? })
        })?;
        Ok(rows.filter_map(|r| r.ok()).filter(|s| kinds.is_empty() || kinds.contains(&s.kind.as_str())).collect())
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
    fn followed_playlists_update() {
        let st = Store::open_memory().unwrap();
        let a = st.create_playlist("From Ana").unwrap();
        st.set_playlist_remote(a, "harp://playlist/x?p=1").unwrap();
        assert!(st.replace_playlist_songs(a, &[song("1", "1", "A"), song("2", "2", "B")]).unwrap());
        assert!(!st.replace_playlist_songs(a, &[song("1", "1", "A"), song("2", "2", "B")]).unwrap(), "same: no change");
        assert!(st.replace_playlist_songs(a, &[song("3", "3", "C"), song("1", "1", "A")]).unwrap());
        assert_eq!(ids_of(&st.playlist_songs(a).unwrap()), ["3", "1"]);
        assert_eq!(st.playlists().unwrap()[0].remote, "harp://playlist/x?p=1");
    }

    #[test]
    fn covers_and_folders() {
        let st = Store::open_memory().unwrap();
        let a = st.create_playlist("A").unwrap();
        let b = st.create_playlist("B").unwrap();
        st.set_playlist_cover(a, "/covers/a.jpg").unwrap();
        st.set_playlist_folder(a, " Workout ").unwrap();
        st.set_playlist_folder(b, "Workout").unwrap();
        let p = st.playlists().unwrap();
        let pa = p.iter().find(|x| x.id == a).unwrap();
        assert_eq!((pa.cover.as_str(), pa.folder.as_str()), ("/covers/a.jpg", "Workout"));
        st.rename_folder("Workout", "Gym").unwrap();
        assert!(st.playlists().unwrap().iter().all(|x| x.folder == "Gym"));
        st.set_playlist_folder(b, "").unwrap();
        assert_eq!(st.playlists().unwrap().iter().find(|x| x.id == b).unwrap().folder, "");
    }

    #[test]
    fn which_playlists_a_song_is_in() {
        let st = Store::open_memory().unwrap();
        let a = st.create_playlist("A").unwrap();
        let b = st.create_playlist("B").unwrap();
        st.add_to_playlist(a, &[song("x", "x", "X"), song("y", "y", "Y")]).unwrap();
        st.add_to_playlist(b, &[song("x", "x", "X")]).unwrap();
        assert_eq!(st.playlists_with_song("x").unwrap(), [a, b]);
        let mut ids = st.playlisted_ids().unwrap();
        ids.sort();
        assert_eq!(ids, ["x", "y"]);
        st.remove_song_from_playlist(a, "x").unwrap();
        assert_eq!(st.playlists_with_song("x").unwrap(), [b]);
        assert_eq!(ids_of(&st.playlist_songs(a).unwrap()), ["y"]);
    }

    fn ids_of(v: &[Song]) -> Vec<String> {
        v.iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn follow_and_save() {
        let st = Store::open_memory().unwrap();
        let item = |id: &str, kind: &str, thumb: &str| Saved { id: id.into(), kind: kind.into(), title: id.to_uppercase(), subtitle: String::new(), thumbnail: thumb.into() };
        st.set_saved(&item("UCa", "artist", "a.jpg"), true).unwrap();
        st.set_saved(&item("MPb", "album", ""), true).unwrap();
        st.set_saved(&item("UCa", "artist", ""), true).unwrap(); // again, without a picture
        assert!(st.is_saved("UCa").unwrap());
        assert_eq!(st.saved(&["artist"]).unwrap()[0].thumbnail, "a.jpg", "a missing picture doesn't wipe the old one");
        assert_eq!(st.saved(&[]).unwrap().len(), 2);
        assert_eq!(st.saved(&["album", "playlist"]).unwrap()[0].id, "MPb");
        st.set_saved(&item("UCa", "artist", ""), false).unwrap();
        assert!(!st.is_saved("UCa").unwrap());
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
        // New songs go on top, a batch in its own order, after removals too.
        st.add_to_playlist(id, &[song("d", "d", "Z")]).unwrap();
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["d", "c", "b"]);
        assert_eq!(st.add_to_playlist(id, &[song("e", "e", "Z"), song("f", "f", "Z"), song("d", "d", "Z")]).unwrap(), 2);
        assert_eq!(ids(&st.playlist_songs(id).unwrap()), ["e", "f", "d", "c", "b"]);
        st.remove_from_playlist(id, 0).unwrap();
        st.remove_from_playlist(id, 0).unwrap();
        assert!(!st.playlists().unwrap()[0].auto_download);
        st.set_playlist_auto_download(id, true).unwrap();
        assert!(st.playlists().unwrap()[0].auto_download);
        st.rename_playlist(id, "Trips").unwrap();
        let p = st.playlists().unwrap();
        assert_eq!((p[0].id, p[0].name.as_str(), p[0].count, p[0].pinned), (id, "Trips", 3, false));
        assert_eq!(p[0].covers.len(), 3, "a mosaic of the first songs' artwork");
        let other = st.create_playlist("Other").unwrap();
        assert_eq!(st.playlists().unwrap()[0].id, other, "newest first");
        st.pin_playlist(id, true).unwrap();
        assert_eq!(st.playlists().unwrap()[0].id, id, "pinned ones come first");
        assert!(st.playlists().unwrap()[0].pinned);
        assert_eq!(st.playlist_seeds(10).unwrap().len(), 3);
        assert!(st.playlist_seeds(10).unwrap().iter().all(|(_, n)| n == "Trips"));
        st.delete_playlist(other).unwrap();
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
