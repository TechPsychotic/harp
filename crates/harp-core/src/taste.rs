//! The on-device taste model: plays, completion, likes, skip reasons and
//! per-artist scores, in a local SQLite file. Nothing leaves the device.

use crate::error::Result;
use crate::model::Song;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

/// Played at least this far counts as liked.
pub const LIKE_THRESHOLD: f64 = 0.80;
/// Choosing a song by search is itself a strong taste signal.
pub const SEARCH_LIKE_FLOOR: f64 = 0.5;
/// A quick bail after a deliberate search is probably "wrong version", not "wrong artist".
pub const SEARCH_SKIP_DISCOUNT: f64 = 0.3;
/// A superlike boosts the artist this much.
pub const SUPERLIKE_WEIGHT: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaySource {
    Recommended,
    Searched,
    Queue,
}

impl PlaySource {
    fn as_str(self) -> &'static str {
        match self {
            PlaySource::Recommended => "recommended",
            PlaySource::Searched => "searched",
            PlaySource::Queue => "queue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    WrongGenre,
    NotThisArtist,
    NotTheMood,
    Overplayed,
}

impl SkipReason {
    fn as_str(self) -> &'static str {
        match self {
            SkipReason::WrongGenre => "wrong_genre",
            SkipReason::NotThisArtist => "not_artist",
            SkipReason::NotTheMood => "not_mood",
            SkipReason::Overplayed => "overplayed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlayRecord {
    pub song: Song,
    /// Fraction of the song heard, 0..=1.
    pub completion: f64,
    pub superliked: bool,
    pub source: PlaySource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArtistScore {
    pub likes: f64,
    pub skips: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub plays: u32,
    pub liked: u32,
    pub artists: u32,
}

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS plays(
  id INTEGER PRIMARY KEY, song_id TEXT NOT NULL, song_json TEXT NOT NULL,
  artist_key TEXT NOT NULL, completion REAL NOT NULL, liked INTEGER NOT NULL,
  superliked INTEGER NOT NULL, source TEXT NOT NULL,
  played_at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE INDEX IF NOT EXISTS plays_recent ON plays(id DESC);
CREATE TABLE IF NOT EXISTS artist_scores(
  artist_key TEXT PRIMARY KEY, name TEXT NOT NULL,
  likes REAL NOT NULL DEFAULT 0, skips REAL NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS skip_reasons(
  song_id TEXT NOT NULL, artist_key TEXT NOT NULL, reason TEXT NOT NULL,
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS likes(
  song_id TEXT PRIMARY KEY, song_json TEXT NOT NULL,
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT NOT NULL);
";

fn from_json(s: String) -> Option<Song> {
    serde_json::from_str(&s).ok()
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Records a play and updates the artist's score.
    pub fn record_play(&self, r: &PlayRecord) -> Result<()> {
        let c = r.completion.clamp(0.0, 1.0);
        let liked = r.superliked || c >= LIKE_THRESHOLD;
        let key = r.song.artist_key();
        self.conn.execute(
            "INSERT INTO plays(song_id,song_json,artist_key,completion,liked,superliked,source)
             VALUES(?,?,?,?,?,?,?)",
            params![
                r.song.id,
                serde_json::to_string(&r.song).unwrap_or_default(),
                key,
                c,
                liked,
                r.superliked,
                r.source.as_str()
            ],
        )?;
        let (like_w, skip_w) = if r.superliked {
            (SUPERLIKE_WEIGHT, 0.0)
        } else if r.source == PlaySource::Searched {
            (c.max(SEARCH_LIKE_FLOOR), (1.0 - c) * SEARCH_SKIP_DISCOUNT)
        } else {
            (c, 1.0 - c)
        };
        self.bump_artist(&key, &r.song.artist, like_w, skip_w)
    }

    fn bump_artist(&self, key: &str, name: &str, likes: f64, skips: f64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artist_scores(artist_key,name,likes,skips) VALUES(?,?,?,?)
             ON CONFLICT(artist_key) DO UPDATE SET
               likes=likes+excluded.likes, skips=skips+excluded.skips, name=excluded.name",
            params![key, name, likes, skips],
        )?;
        Ok(())
    }

    /// Why the user skipped a song. "Not this artist" and "overplayed" push the
    /// artist down further; the other reasons are kept for later tuning.
    pub fn skip_reason(&self, song: &Song, reason: SkipReason) -> Result<()> {
        let key = song.artist_key();
        self.conn.execute(
            "INSERT INTO skip_reasons(song_id,artist_key,reason) VALUES(?,?,?)",
            params![song.id, key, reason.as_str()],
        )?;
        match reason {
            SkipReason::NotThisArtist => self.bump_artist(&key, &song.artist, 0.0, 1.0)?,
            SkipReason::Overplayed => self.bump_artist(&key, &song.artist, 0.0, 0.5)?,
            _ => {}
        }
        Ok(())
    }

    pub fn artist_scores(&self) -> Result<HashMap<String, ArtistScore>> {
        let mut st = self.conn.prepare("SELECT artist_key,likes,skips FROM artist_scores")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, ArtistScore { likes: r.get(1)?, skips: r.get(2)? }))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Ids of the last `n` plays (to avoid repeating).
    pub fn recent_ids(&self, n: usize) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT song_id FROM plays ORDER BY id DESC LIMIT ?")?;
        let rows = st.query_map([n as i64], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Recent songs, newest first, one entry per song.
    pub fn history(&self, limit: usize) -> Result<Vec<Song>> {
        let mut st = self.conn.prepare(
            "SELECT song_json FROM plays WHERE id IN (SELECT MAX(id) FROM plays GROUP BY song_id)
             ORDER BY id DESC LIMIT ?",
        )?;
        let rows = st.query_map([limit as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(from_json).collect())
    }

    fn songs_where(&self, cond: &str, limit: usize) -> Result<Vec<Song>> {
        let sql = format!(
            "SELECT song_json FROM plays WHERE id IN
               (SELECT MAX(id) FROM plays WHERE {cond} GROUP BY song_id)
             ORDER BY id DESC LIMIT ?"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map([limit as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(from_json).collect())
    }

    pub fn superliked(&self, limit: usize) -> Result<Vec<Song>> {
        self.songs_where("superliked=1", limit)
    }
    pub fn liked_by_play(&self, limit: usize) -> Result<Vec<Song>> {
        self.songs_where("liked=1", limit)
    }
    /// Searched songs heard at least a little: they may seed recommendations.
    pub fn searched(&self, limit: usize) -> Result<Vec<Song>> {
        self.songs_where("source='searched' AND completion>=0.1", limit)
    }

    // --- Liked songs (explicit hearts, the library's "Liked songs") ---

    /// Toggles the heart; returns the new state.
    pub fn toggle_like(&self, song: &Song) -> Result<bool> {
        let had = self.is_liked(&song.id)?;
        if had {
            self.conn.execute("DELETE FROM likes WHERE song_id=?", [&song.id])?;
        } else {
            self.conn.execute(
                "INSERT OR REPLACE INTO likes(song_id,song_json) VALUES(?,?)",
                params![song.id, serde_json::to_string(song).unwrap_or_default()],
            )?;
        }
        Ok(!had)
    }

    pub fn is_liked(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM likes WHERE song_id=?", [id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    pub fn liked_songs(&self) -> Result<Vec<Song>> {
        let mut st = self.conn.prepare("SELECT song_json FROM likes ORDER BY at DESC, rowid DESC")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(from_json).collect())
    }

    pub fn stats(&self) -> Result<Stats> {
        let q = |sql: &str| -> Result<u32> { Ok(self.conn.query_row(sql, [], |r| r.get(0))?) };
        Ok(Stats {
            plays: q("SELECT COUNT(*) FROM plays")?,
            liked: q("SELECT COUNT(*) FROM plays WHERE liked=1")?,
            artists: q("SELECT COUNT(*) FROM artist_scores")?,
        })
    }

    pub fn kv_get(&self, k: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT v FROM kv WHERE k=?", [k], |r| r.get(0)).optional()?)
    }

    pub fn kv_set(&self, k: &str, v: &str) -> Result<()> {
        self.conn
            .execute("INSERT OR REPLACE INTO kv(k,v) VALUES(?,?)", params![k, v])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::song;

    fn rec(s: &Song, c: f64, src: PlaySource, sl: bool) -> PlayRecord {
        PlayRecord { song: s.clone(), completion: c, superliked: sl, source: src }
    }

    #[test]
    fn graded_completion_scores() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        st.record_play(&rec(&s, 0.05, PlaySource::Recommended, false)).unwrap();
        st.record_play(&rec(&s, 1.0, PlaySource::Recommended, false)).unwrap();
        let sc = st.artist_scores().unwrap()[&s.artist_key()];
        assert!((sc.likes - 1.05).abs() < 1e-9);
        assert!((sc.skips - 0.95).abs() < 1e-9);
    }

    #[test]
    fn superlike_is_four_x() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        st.record_play(&rec(&s, 0.3, PlaySource::Recommended, true)).unwrap();
        let sc = st.artist_scores().unwrap()[&s.artist_key()];
        assert_eq!(sc, ArtistScore { likes: 4.0, skips: 0.0 });
        assert_eq!(st.superliked(5).unwrap().len(), 1);
    }

    #[test]
    fn search_floor_and_discounted_skip() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        st.record_play(&rec(&s, 0.0, PlaySource::Searched, false)).unwrap();
        let sc = st.artist_scores().unwrap()[&s.artist_key()];
        assert_eq!(sc.likes, 0.5);
        assert!((sc.skips - 0.3).abs() < 1e-9);
    }

    #[test]
    fn skip_reasons_move_scores() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        st.skip_reason(&s, SkipReason::NotThisArtist).unwrap();
        st.skip_reason(&s, SkipReason::Overplayed).unwrap();
        st.skip_reason(&s, SkipReason::WrongGenre).unwrap();
        assert_eq!(st.artist_scores().unwrap()[&s.artist_key()].skips, 1.5);
    }

    #[test]
    fn history_dedupes_newest_first() {
        let st = Store::open_memory().unwrap();
        let (a, b) = (song("a", "a", "X"), song("b", "b", "Y"));
        for s in [&a, &b, &a] {
            st.record_play(&rec(s, 0.9, PlaySource::Queue, false)).unwrap();
        }
        let h: Vec<_> = st.history(10).unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(h, ["a", "b"]);
        assert_eq!(st.recent_ids(2).unwrap(), ["a", "b"]);
    }

    #[test]
    fn hearts_and_kv() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        assert!(st.toggle_like(&s).unwrap());
        assert!(st.is_liked("1").unwrap());
        assert_eq!(st.liked_songs().unwrap().len(), 1);
        assert!(!st.toggle_like(&s).unwrap());
        st.kv_set("last", "x").unwrap();
        assert_eq!(st.kv_get("last").unwrap().as_deref(), Some("x"));
        assert_eq!(st.kv_get("nope").unwrap(), None);
    }
}
