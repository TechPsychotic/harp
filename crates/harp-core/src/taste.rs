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

/// A skip only counts against a song (and its artist) once you've skipped that
/// song early more than this many times within [`SKIP_WINDOW_DAYS`]. A skip
/// now and then just means "not now".
pub const REPEAT_SKIPS: i64 = 5;
pub const SKIP_WINDOW_DAYS: i64 = 30;

/// Under this much heard, a play counts as an early skip.
pub const EARLY_SKIP: f64 = 0.3;

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
    pub(crate) conn: Connection,
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
CREATE TABLE IF NOT EXISTS searches(
  id INTEGER PRIMARY KEY, q TEXT NOT NULL,
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS lyrics(
  song_id TEXT PRIMARY KEY, json TEXT NOT NULL,
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS playlists(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL,
  created INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS playlist_items(
  id INTEGER PRIMARY KEY, playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  pos INTEGER NOT NULL, song_id TEXT NOT NULL, song_json TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS playlist_items_pl ON playlist_items(playlist_id, pos);
CREATE TABLE IF NOT EXISTS local_files(
  id TEXT PRIMARY KEY, path TEXT NOT NULL, mtime INTEGER NOT NULL, song_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS saved(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, title TEXT NOT NULL,
  subtitle TEXT NOT NULL DEFAULT '', thumbnail TEXT NOT NULL DEFAULT '',
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
CREATE TABLE IF NOT EXISTS downloads(
  song_id TEXT PRIMARY KEY, song_json TEXT NOT NULL, path TEXT NOT NULL,
  bytes INTEGER NOT NULL, quality TEXT NOT NULL,
  at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
";

/// Schema changes after the first release, in order. Each runs once; the database
/// remembers how far it got (`PRAGMA user_version`). Never edit an entry, only append.
const MIGRATIONS: &[&str] = &[
    // 1: playlists can be pinned to the top.
    "ALTER TABLE playlists ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;",
    // 2: lyrics now come from more sources, so look again for songs that had none.
    "DELETE FROM lyrics WHERE json LIKE '%\"lines\":[]%';",
    // 3: playlists that download new songs by themselves.
    "ALTER TABLE playlists ADD COLUMN autodl INTEGER NOT NULL DEFAULT 0;",
    // 4: artist scores fade with time; this is when each was last brought up to date.
    "ALTER TABLE artist_scores ADD COLUMN updated INTEGER NOT NULL DEFAULT 0;",
];

/// Taste fades: an artist's score halves every 60 days without new plays,
/// so what you listen to now outweighs what you played a year ago.
pub const HALF_LIFE_SECS: f64 = 60.0 * 86400.0;

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// How much of a score is left after `age` seconds. Scores never stamped
/// (from before fading existed) are treated as fresh.
pub fn decay(age_secs: i64, stamped: bool) -> f64 {
    if !stamped || age_secs <= 0 {
        return 1.0;
    }
    0.5f64.powf(age_secs as f64 / HALF_LIFE_SECS)
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let done: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(done.max(0) as usize) {
        conn.execute_batch(&format!("BEGIN; {sql} PRAGMA user_version = {}; COMMIT;", i + 1))?;
    }
    Ok(())
}

fn from_json(s: String) -> Option<Song> {
    serde_json::from_str(&s).ok()
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Store { conn })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
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
        let (like_w, mut skip_w) = if r.superliked {
            (SUPERLIKE_WEIGHT, 0.0)
        } else if r.source == PlaySource::Searched {
            (c.max(SEARCH_LIKE_FLOOR), (1.0 - c) * SEARCH_SKIP_DISCOUNT)
        } else {
            (c, 1.0 - c)
        };
        // Skipping isn't held against anything until it keeps happening to this song.
        if skip_w > 0.0 && self.early_skips(&r.song.id, SKIP_WINDOW_DAYS, now_secs())? <= REPEAT_SKIPS {
            skip_w = 0.0;
        }
        self.bump_artist(&key, &r.song.artist, like_w, skip_w)
    }

    fn bump_artist(&self, key: &str, name: &str, likes: f64, skips: f64) -> Result<()> {
        self.bump_artist_at(key, name, likes, skips, now_secs())
    }

    /// Fades the old score to `now`, then adds the new evidence.
    pub(crate) fn bump_artist_at(&self, key: &str, name: &str, likes: f64, skips: f64, now: i64) -> Result<()> {
        use rusqlite::OptionalExtension;
        let old: Option<(f64, f64, i64)> = self
            .conn
            .query_row("SELECT likes,skips,updated FROM artist_scores WHERE artist_key=?", [key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let (l, s) = match old {
            Some((l, s, at)) => {
                let k = decay(now - at, at > 0);
                (l * k, s * k)
            }
            None => (0.0, 0.0),
        };
        self.conn.execute(
            "INSERT INTO artist_scores(artist_key,name,likes,skips,updated) VALUES(?,?,?,?,?)
             ON CONFLICT(artist_key) DO UPDATE SET
               likes=excluded.likes, skips=excluded.skips, name=excluded.name, updated=excluded.updated",
            params![key, name, l + likes, s + skips, now],
        )?;
        Ok(())
    }

    /// Artists you tend to play around this time of day: for each artist with at
    /// least 3 plays, the share of them within two hours of the hour now (local
    /// time). Morning music in the morning, night music at night.
    pub fn hour_affinity(&self) -> Result<HashMap<String, f64>> {
        self.hour_affinity_at(None)
    }

    pub(crate) fn hour_affinity_at(&self, hour: Option<u32>) -> Result<HashMap<String, f64>> {
        let h: i64 = match hour {
            Some(h) => h as i64,
            None => self.conn.query_row("SELECT CAST(strftime('%H','now','localtime') AS INTEGER)", [], |r| r.get(0))?,
        };
        let mut st = self.conn.prepare(
            "SELECT artist_key, count(*),
               sum(CASE WHEN min(abs(CAST(strftime('%H',played_at,'unixepoch','localtime') AS INTEGER) - ?1),
                                 24 - abs(CAST(strftime('%H',played_at,'unixepoch','localtime') AS INTEGER) - ?1)) <= 2
                   THEN 1 ELSE 0 END)
             FROM plays GROUP BY artist_key HAVING count(*) >= 3",
        )?;
        let rows = st.query_map([h], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
        Ok(rows.filter_map(|r| r.ok()).map(|(k, n, near)| (k, near as f64 / n as f64)).collect())
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

    /// Every artist's score, faded to now.
    pub fn artist_scores(&self) -> Result<HashMap<String, ArtistScore>> {
        let now = now_secs();
        let mut st = self.conn.prepare("SELECT artist_key,likes,skips,updated FROM artist_scores")?;
        let rows = st.query_map([], |r| {
            let at: i64 = r.get(3)?;
            let k = decay(now - at, at > 0);
            Ok((r.get::<_, String>(0)?, ArtistScore { likes: r.get::<_, f64>(1)? * k, skips: r.get::<_, f64>(2)? * k }))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// How often a song was skipped early in the last `days` days.
    pub(crate) fn early_skips(&self, song_id: &str, days: i64, now: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM plays WHERE song_id=?1 AND completion < ?2 AND played_at >= ?3",
            params![song_id, EARLY_SKIP, now - days * 86400],
            |r| r.get(0),
        )?)
    }

    /// Songs you keep skipping: early more than [`REPEAT_SKIPS`] times in the
    /// last `days` days (and not finished since). Only these are set aside.
    pub fn repeatedly_skipped(&self, days: i64) -> Result<Vec<String>> {
        self.repeatedly_skipped_at(days, now_secs())
    }

    pub(crate) fn repeatedly_skipped_at(&self, days: i64, now: i64) -> Result<Vec<String>> {
        let since = now - days * 86400;
        let mut st = self.conn.prepare(
            "SELECT song_id FROM plays WHERE played_at >= ?1 GROUP BY song_id
             HAVING sum(CASE WHEN completion < ?2 THEN 1 ELSE 0 END) > ?3
                AND coalesce(max(CASE WHEN completion >= 0.8 THEN played_at END), 0)
                    < max(CASE WHEN completion < ?2 THEN played_at END)",
        )?;
        let rows = st.query_map(params![since, EARLY_SKIP, REPEAT_SKIPS], |r| r.get(0))?;
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
        assert_eq!(sc.skips, 0.0, "one skip isn't held against anyone");
    }

    #[test]
    fn only_repeated_skips_count() {
        let st = Store::open_memory().unwrap();
        let s = song("1", "t", "A");
        for _ in 0..REPEAT_SKIPS {
            st.record_play(&rec(&s, 0.05, PlaySource::Recommended, false)).unwrap();
        }
        assert_eq!(st.artist_scores().unwrap()[&s.artist_key()].skips, 0.0, "{REPEAT_SKIPS} skips: still nothing");
        assert!(st.repeatedly_skipped(SKIP_WINDOW_DAYS).unwrap().is_empty());
        st.record_play(&rec(&s, 0.05, PlaySource::Recommended, false)).unwrap();
        assert!(st.artist_scores().unwrap()[&s.artist_key()].skips > 0.9, "the 6th one counts");
        assert_eq!(st.repeatedly_skipped(SKIP_WINDOW_DAYS).unwrap(), ["1"]);
        // Finishing it after all takes it back.
        st.record_play(&rec(&s, 1.0, PlaySource::Recommended, false)).unwrap();
        assert!(st.repeatedly_skipped(SKIP_WINDOW_DAYS).unwrap().is_empty());
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
        assert_eq!(sc.skips, 0.0, "a single skip of a searched song doesn't count either");
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
    fn old_databases_are_upgraded_in_place() {
        let dir = std::env::temp_dir().join(format!("harp-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        let _ = std::fs::remove_file(&path);
        {
            // A database as the first release made it: no `pinned` column, version 0.
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE playlists(id INTEGER PRIMARY KEY, name TEXT NOT NULL, created INTEGER NOT NULL DEFAULT 0);").unwrap();
            c.execute("INSERT INTO playlists(name) VALUES('Kept')", []).unwrap();
            c.execute_batch(
                r#"CREATE TABLE lyrics(song_id TEXT PRIMARY KEY, json TEXT NOT NULL, at INTEGER NOT NULL DEFAULT 0);
                   INSERT INTO lyrics(song_id,json) VALUES('none','{"synced":false,"lines":[],"source":""}');
                   INSERT INTO lyrics(song_id,json) VALUES('some','{"synced":false,"lines":[{"at_ms":0,"text":"hi"}],"source":"LRCLIB"}');"#,
            )
            .unwrap();
        }
        let st = Store::open(&path).unwrap();
        let v: i64 = st.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        let p = st.playlists().unwrap();
        assert_eq!((p[0].name.as_str(), p[0].pinned), ("Kept", false));
        // Remembered "no lyrics" is forgotten (more sources now); real lyrics stay.
        assert!(st.cached_lyrics("none").unwrap().is_none());
        assert!(st.conn.query_row("SELECT count(*) FROM lyrics WHERE song_id='some'", [], |r| r.get::<_, i64>(0)).unwrap() == 1);
        drop(st);
        // Opening again runs nothing twice.
        assert!(Store::open(&path).is_ok());
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

    #[test]
    fn taste_fades_with_time() {
        assert_eq!(decay(0, true), 1.0);
        assert!((decay(HALF_LIFE_SECS as i64, true) - 0.5).abs() < 1e-9);
        assert_eq!(decay(10_000_000, false), 1.0, "never-stamped scores aren't faded");
        let st = Store::open_memory().unwrap();
        let day = 86400;
        // Loved a year ago, nothing since...
        st.bump_artist_at("old", "Old", 10.0, 0.0, 1_000_000).unwrap();
        // ...versus a little, recently: the old love is down to ~1/64 of itself.
        st.bump_artist_at("old", "Old", 0.0, 0.0, 1_000_000 + 360 * day).unwrap();
        st.bump_artist_at("new", "New", 1.0, 0.0, 1_000_000 + 360 * day).unwrap();
        let (l, _, _): (f64, f64, i64) = st.conn.query_row("SELECT likes,skips,updated FROM artist_scores WHERE artist_key='old'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert!(l < 0.2, "{l}");
    }

    #[test]
    fn time_of_day_affinity() {
        let st = Store::open_memory().unwrap();
        // Plays stamped at chosen local hours.
        let at = |hour: i64| -> i64 {
            let off: i64 = st.conn.query_row("SELECT strftime('%s','2026-01-01 00:00:00') - strftime('%s','2026-01-01 00:00:00','utc')", [], |r| r.get(0)).unwrap();
            1_767_225_600 + hour * 3600 - off // 2026-01-01 local
        };
        for (artist, hour) in [("m", 7), ("m", 8), ("m", 9), ("m", 21), ("n", 22), ("n", 23), ("n", 0), ("x", 8)] {
            st.conn.execute(
                "INSERT INTO plays(song_id,song_json,artist_key,completion,liked,superliked,source,played_at) VALUES('s','{}',?,1,1,0,'auto',?)",
                params![artist, at(hour)],
            ).unwrap();
        }
        let morning = st.hour_affinity_at(Some(8)).unwrap();
        assert_eq!(morning.get("m"), Some(&0.75));
        assert_eq!(morning.get("n"), Some(&0.0));
        assert!(!morning.contains_key("x"), "too few plays to tell");
        let night = st.hour_affinity_at(Some(23)).unwrap();
        assert_eq!(night.get("n"), Some(&1.0), "wraps around midnight");
        assert_eq!(night.get("m"), Some(&0.25));
    }

    #[test]
    fn repeated_skips_inside_the_window_only() {
        let st = Store::open_memory().unwrap();
        let day = 86400;
        let now = 2_000_000_000;
        let play = |id: &str, c: f64, at: i64| {
            st.conn.execute(
                "INSERT INTO plays(song_id,song_json,artist_key,completion,liked,superliked,source,played_at) VALUES(?,'{}','a',?,0,0,'auto',?)",
                params![id, c, at],
            ).unwrap();
        };
        for i in 0..6 {
            play("often", 0.1, now - i * day);
            play("long_ago", 0.1, now - (40 + i) * day);
        }
        for i in 0..3 {
            play("sometimes", 0.1, now - i * day);
        }
        let v = st.repeatedly_skipped_at(SKIP_WINDOW_DAYS, now).unwrap();
        assert_eq!(v, ["often"], "6 skips in 30 days counts; 3, or old ones, don't");
    }
}
