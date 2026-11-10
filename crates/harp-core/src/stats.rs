//! Listening stats from the play history: "Your Rhythm" (any week, month or
//! year) and "Harp Encore" (the year in review). Everything is computed on the
//! device from the `plays` table; nothing leaves it.

use crate::taste::Store;
use crate::{Result, Song};
use std::collections::HashMap;

/// A song shorter than this many seconds of listening isn't counted as a play.
const MIN_PLAY: f64 = 0.3;
/// When a song's length is unknown.
const DEFAULT_SECS: f64 = 210.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ArtistStat {
    pub name: String,
    pub minutes: f64,
    pub plays: u32,
    /// A cover from one of their songs.
    pub thumbnail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SongStat {
    pub song: Song,
    pub plays: u32,
    pub minutes: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub minutes: f64,
    /// Plays heard past 30%.
    pub plays: u32,
    pub songs: u32,
    pub artists: u32,
    pub top_artists: Vec<ArtistStat>,
    pub top_songs: Vec<SongStat>,
    /// Minutes for each local hour of the day, 0..24.
    pub by_hour: Vec<f64>,
    /// Minutes for each local day that had listening, oldest first ("2026-11-10").
    pub by_day: Vec<(String, f64)>,
    /// The most days in a row with listening.
    pub streak: u32,
    /// The first song heard in the range.
    pub first: Option<Song>,
}

impl Stats {
    /// The local hour with the most listening.
    pub fn peak_hour(&self) -> Option<u32> {
        let (h, m) = self.by_hour.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
        (*m > 0.0).then_some(h as u32)
    }
}

fn next_day(d: &str) -> String {
    // d is "YYYY-MM-DD"; enough calendar math for streaks.
    let p: Vec<u32> = d.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return String::new();
    }
    let (mut y, mut m, mut day) = (p[0], p[1], p[2] + 1);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let len = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m as usize).clamp(1, 12) - 1];
    if day > len {
        day = 1;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    format!("{y:04}-{m:02}-{day:02}")
}

impl Store {
    /// Stats for plays with `from <= played_at < to` (unix seconds), `top` entries per list.
    pub fn listening_stats(&self, from: i64, to: i64, top: usize) -> Result<Stats> {
        let mut st = self.conn.prepare(
            "SELECT song_id, song_json, artist_key, completion,
                    CAST(strftime('%H', played_at, 'unixepoch', 'localtime') AS INTEGER),
                    date(played_at, 'unixepoch', 'localtime')
             FROM plays WHERE played_at >= ?1 AND played_at < ?2 ORDER BY played_at, id",
        )?;
        let rows = st.query_map([from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut s = Stats { by_hour: vec![0.0; 24], ..Default::default() };
        let mut songs: HashMap<String, SongStat> = HashMap::new();
        let mut artists: HashMap<String, ArtistStat> = HashMap::new();
        let mut days: Vec<(String, f64)> = vec![];
        for row in rows {
            let (id, json, key, completion, hour, day) = row?;
            let Ok(song) = serde_json::from_str::<Song>(&json) else { continue };
            let mins = completion.clamp(0.0, 1.0) * song.duration_secs.map(f64::from).unwrap_or(DEFAULT_SECS) / 60.0;
            let played = completion >= MIN_PLAY;
            s.minutes += mins;
            s.by_hour[(hour as usize).min(23)] += mins;
            match days.last_mut() {
                Some((d, m)) if *d == day => *m += mins,
                _ => days.push((day, mins)),
            }
            if s.first.is_none() && played {
                s.first = Some(song.clone());
            }
            let a = artists.entry(key).or_insert_with(|| ArtistStat {
                name: song.artist.split(',').next().unwrap_or(&song.artist).trim().to_string(),
                minutes: 0.0,
                plays: 0,
                thumbnail: None,
            });
            a.minutes += mins;
            if a.thumbnail.is_none() {
                a.thumbnail = song.thumbnail.clone();
            }
            let e = songs.entry(id).or_insert_with(|| SongStat { song, plays: 0, minutes: 0.0 });
            e.minutes += mins;
            if played {
                s.plays += 1;
                e.plays += 1;
                a.plays += 1;
            }
        }
        s.songs = songs.values().filter(|x| x.plays > 0).count() as u32;
        s.artists = artists.values().filter(|x| x.plays > 0).count() as u32;
        let mut ar: Vec<ArtistStat> = artists.into_values().filter(|a| a.plays > 0).collect();
        ar.sort_by(|a, b| b.minutes.total_cmp(&a.minutes).then(b.plays.cmp(&a.plays)));
        ar.truncate(top);
        s.top_artists = ar;
        let mut so: Vec<SongStat> = songs.into_values().filter(|x| x.plays > 0).collect();
        so.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.minutes.total_cmp(&a.minutes)));
        so.truncate(top);
        s.top_songs = so;
        let (mut run, mut best) = (0u32, 0u32);
        for i in 0..days.len() {
            run = if i > 0 && next_day(&days[i - 1].0) == days[i].0 { run + 1 } else { 1 };
            best = best.max(run);
        }
        s.streak = best;
        s.by_day = days;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, artist: &str, secs: u32) -> Song {
        Song {
            id: id.into(),
            title: id.into(),
            artist: artist.into(),
            artist_id: None,
            album: None,
            duration_secs: Some(secs),
            thumbnail: Some(format!("t/{id}")),
        }
    }

    fn play(st: &Store, s: &Song, completion: f64, at: i64) {
        st.conn
            .execute(
                "INSERT INTO plays(song_id,song_json,artist_key,completion,liked,superliked,source,played_at) VALUES(?,?,?,?,0,0,'auto',?)",
                rusqlite::params![s.id, serde_json::to_string(s).unwrap(), s.artist_key(), completion, at],
            )
            .unwrap();
    }

    #[test]
    fn counts_minutes_tops_and_streak() {
        let st = Store::open_memory().unwrap();
        let a = song("a", "Alpha", 240);
        let b = song("b", "Beta, Gamma", 120);
        let day = 86400;
        let t0 = 1_700_000_000 - 1_700_000_000 % day + 12 * 3600; // midday UTC
        play(&st, &a, 1.0, t0);
        play(&st, &a, 1.0, t0 + 600);
        play(&st, &b, 0.5, t0 + day);
        play(&st, &b, 0.1, t0 + 2 * day); // a skip: minutes, but not a play
        play(&st, &a, 1.0, t0 + 5 * day);
        play(&st, &a, 1.0, t0 + 400 * day); // out of range
        let s = st.listening_stats(t0 - 100, t0 + 10 * day, 5).unwrap();
        assert_eq!(s.plays, 4);
        assert!((s.minutes - (4.0 * 3.0 + 1.0 + 0.2)).abs() < 1e-9, "{}", s.minutes);
        assert_eq!(s.songs, 2);
        assert_eq!(s.artists, 2);
        assert_eq!(s.top_artists[0].name, "Alpha");
        assert_eq!(s.top_artists[1].name, "Beta");
        assert_eq!(s.top_songs[0].song.id, "a");
        assert_eq!(s.top_songs[0].plays, 3);
        assert_eq!(s.first.as_ref().unwrap().id, "a");
        assert_eq!(s.by_day.len(), 4);
        assert_eq!(s.streak, 3);
        assert!(s.peak_hour().is_some());
        assert_eq!(s.by_hour.iter().sum::<f64>(), s.minutes);
        let empty = st.listening_stats(0, 10, 5).unwrap();
        assert_eq!(empty.plays, 0);
        assert_eq!(empty.peak_hour(), None);
    }

    #[test]
    fn calendar_steps() {
        assert_eq!(next_day("2026-02-28"), "2026-03-01");
        assert_eq!(next_day("2028-02-28"), "2028-02-29");
        assert_eq!(next_day("2026-12-31"), "2027-01-01");
        assert_eq!(next_day("2026-11-09"), "2026-11-10");
    }
}
