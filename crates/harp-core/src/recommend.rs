//! The recommender. Same ideas as a "self-learning shuffle", on the device and
//! with no accounts or API keys:
//!
//! - skipping at 5% vs 75% is different; finishing a song scores highest;
//! - a superlike (4x) also seeds future picks first;
//! - a deliberate search is a strong signal for that artist;
//! - cold start uses genre seed queries; once there are liked songs it pulls
//!   "watch next" candidates from the source, with a 20% genre injection to
//!   avoid a taste bubble;
//! - every pick carries a reason line.

use crate::error::Result;
use crate::model::Song;
use crate::rng::Rng;
use crate::taste::{ArtistScore, Store};
use std::collections::{HashMap, HashSet};

pub const RECENT_WINDOW: usize = 50;
pub const COLD_INJECT_RATE: f64 = 0.20;
const SEARCH_SEED_WINDOW: usize = 10;

/// Where candidates come from (yt-dlp in the app, a fake in tests).
pub trait CandidateSource {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<Song>>;
    /// Songs YouTube suggests after this one ("watch next" / mix).
    fn related(&self, song_id: &str, limit: usize) -> Result<Vec<Song>>;
}

pub const SEED_QUERIES: &[&str] = &[
    "80s pop hits", "90s pop hits", "2000s pop hits", "2010s pop hits", "70s classic rock",
    "80s rock hits", "90s alternative rock", "classic rock hits", "classic country hits",
    "90s country hits", "motown hits", "70s soul hits", "90s r&b hits", "2000s r&b hits",
    "90s hip hop hits", "2000s hip hop hits", "classic rap hits", "90s dance hits",
    "2000s euro dance hits", "classic edm hits", "80s new wave hits", "60s hits",
    "jazz standards", "classic blues hits", "reggae hits", "latin pop hits", "folk hits",
    "bollywood hits", "k-pop hits", "lofi beats", "indie pop hits", "synthwave hits",
];

/// (key, label, queries) for explicit genre requests and the "more/less energy" nudges.
pub const GENRES: &[(&str, &str, &[&str])] = &[
    ("pop", "Pop", &["pop hits", "2010s pop hits"]),
    ("rock", "Rock", &["classic rock hits", "90s alternative rock"]),
    ("hiphop", "Hip-Hop", &["90s hip hop hits", "2000s hip hop hits"]),
    ("rnb", "R&B", &["90s r&b hits", "motown hits"]),
    ("electronic", "Electronic", &["classic edm hits", "house music hits", "synthwave hits"]),
    ("jazz", "Jazz", &["jazz standards", "smooth jazz hits"]),
    ("country", "Country", &["classic country hits", "country pop hits"]),
    ("folk", "Folk", &["folk hits", "singer songwriter hits"]),
    ("latin", "Latin", &["latin pop hits", "reggaeton hits"]),
    ("reggae", "Reggae", &["reggae hits", "ska hits"]),
    ("indie", "Indie", &["indie pop hits", "indie rock hits"]),
    ("chill", "Chill", &["chill vibes songs", "lofi beats", "acoustic chill songs"]),
    ("focus", "Focus", &["focus music instrumental", "deep focus lofi", "study beats"]),
    ("workout", "Workout", &["workout hits", "gym motivation songs", "running playlist hits"]),
    ("party", "Party", &["party hits", "club dance hits", "bollywood party songs"]),
    ("sad", "Feeling blue", &["sad songs", "heartbreak songs", "melancholy acoustic songs"]),
    ("romance", "Romance", &["romantic songs", "love songs hits", "slow love ballads"]),
    ("sleep", "Wind down", &["sleep music calm", "soft piano relaxing", "ambient calm songs"]),
    ("commute", "On the road", &["road trip songs", "driving songs hits", "feel good hits"]),
    ("more_energy", "More Energy", &["90s dance hits", "80s rock hits", "2000s euro dance hits"]),
    ("less_energy", "Less Energy", &["jazz standards", "folk hits", "70s soul hits", "lofi beats"]),
];

/// The genres above that are really moods, for the home page's "change the mood".
pub const MOOD_KEYS: &[&str] = &["chill", "focus", "workout", "party", "sad", "romance", "sleep", "commute"];

#[derive(Clone, Debug, Default)]
pub struct Request {
    /// Genre key from [`GENRES`].
    pub genre: Option<String>,
    /// Play more by this artist (name).
    pub artist: Option<String>,
    /// The user explicitly asked (changes the wording of the reason).
    pub requested: bool,
    /// Song ids not to pick (e.g. what is already queued).
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Recommendation {
    pub song: Song,
    pub reason: String,
}

fn playable(s: &Song) -> bool {
    s.duration_secs.map_or(true, |d| (60..=600).contains(&d))
}

fn score(song: &Song, scores: &HashMap<String, ArtistScore>, rng: &mut Rng) -> f64 {
    let mut base = rng.f64();
    if let Some(s) = scores.get(&song.artist_key()) {
        base += s.likes * 3.0 - s.skips * 2.0;
    }
    base
}

fn cold(src: &dyn CandidateSource, q: &str) -> Vec<Song> {
    src.search(q, 25).unwrap_or_default().into_iter().filter(playable).collect()
}

fn format_query(q: &str) -> String {
    let q = q.trim();
    let q = q.strip_suffix(" hits").or_else(|| q.strip_suffix(" music")).unwrap_or(q);
    q.split_whitespace()
        .map(|w| {
            if w.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                w.to_string()
            } else {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What you search for says what you want to hear: now and then (more often
/// before there is any listening history) a recent search seeds the picks.
fn searched_query(store: &Store, rng: &mut Rng, has_history: bool) -> Result<Option<String>> {
    let recent = store.recent_searches(6)?;
    if recent.is_empty() {
        return Ok(None);
    }
    let rate = if has_history { 0.22 } else { 0.6 };
    if rng.f64() >= rate {
        return Ok(None);
    }
    Ok(rng.choose(&recent).cloned())
}

struct Gathered {
    candidates: Vec<Song>,
    reason: String,
    used_seed: Option<Song>,
}

/// Collects a pool of candidates (a few network calls, once) and the reason
/// they were chosen. Picking from the pool afterwards is free.
fn gather(store: &Store, src: &dyn CandidateSource, req: &Request, rng: &mut Rng) -> Result<Option<Gathered>> {
    let superliked = store.superliked(10)?;
    let liked = store.liked_by_play(20)?;
    let searched = store.searched(SEARCH_SEED_WINDOW)?;
    let super_ids: HashSet<_> = superliked.iter().map(|s| s.id.clone()).collect();
    let liked_ids: HashSet<_> = liked.iter().map(|s| s.id.clone()).collect();
    let searched_only: HashSet<String> = searched
        .iter()
        .map(|s| s.id.clone())
        .filter(|i| !liked_ids.contains(i) && !super_ids.contains(i))
        .collect();

    // Songs you saved in playlists say what you like too.
    let from_playlists = store.playlist_seeds(8)?;
    let playlist_of: HashMap<String, String> = from_playlists.iter().map(|(s, n)| (s.id.clone(), n.clone())).collect();
    let seeds: Vec<Song> = if !superliked.is_empty() {
        superliked
    } else {
        liked
            .into_iter()
            .chain(searched.into_iter().filter(|s| searched_only.contains(&s.id)))
            .chain(from_playlists.into_iter().map(|(s, _)| s))
            .collect()
    };

    let mut used_seed: Option<Song> = None;
    let candidates: Vec<Song>;
    let reason: String;

    let genre = req.genre.as_deref().and_then(|g| GENRES.iter().find(|x| x.0 == g));

    if let Some(artist) = req.artist.as_deref() {
        let mut c = cold(src, artist);
        if c.is_empty() {
            c = cold(src, rng.choose(SEED_QUERIES).copied().unwrap_or("pop hits"));
        }
        candidates = c;
        reason = format!("Playing: {artist}");
    } else if let Some((key, label, qs)) = genre {
        candidates = cold(src, rng.choose(qs).copied().unwrap_or("pop hits"));
        reason = match (*key, req.requested) {
            ("more_energy", true) => "You wanted more energy".into(),
            ("less_energy", true) => "You wanted something calmer".into(),
            (_, true) => format!("You asked for: {label}"),
            _ => format!("Genre: {label}"),
        };
    } else if let Some(q) = searched_query(store, rng, !seeds.is_empty())? {
        let mut c = cold(src, &q);
        if c.is_empty() {
            c = cold(src, rng.choose(SEED_QUERIES).copied().unwrap_or("pop hits"));
        }
        candidates = c;
        reason = format!("Because you searched for “{q}”");
    } else if seeds.is_empty() || rng.f64() < COLD_INJECT_RATE {
        let q = rng.choose(SEED_QUERIES).copied().unwrap_or("pop hits");
        let mut c = cold(src, q);
        if c.is_empty() && !seeds.is_empty() {
            c = warm(src, &seeds, rng).0;
        }
        candidates = c;
        reason = format!("Mixing it up · {}", format_query(q));
    } else {
        let (c, picked) = warm(src, &seeds, rng);
        used_seed = picked.first().cloned();
        candidates = c;
        reason = match &used_seed {
            Some(s) if !super_ids.contains(&s.id) && !liked_ids.contains(&s.id) && !searched_only.contains(&s.id) && playlist_of.contains_key(&s.id) => {
                format!("Because “{}” is in your playlist {}", s.title, playlist_of[&s.id])
            }
            Some(s) => {
                let verb = if super_ids.contains(&s.id) {
                    "loved"
                } else if searched_only.contains(&s.id) {
                    "searched for"
                } else {
                    "liked"
                };
                format!("Because you {verb}: {} · {}", s.title, s.artist)
            }
            None => "Similar to songs you've liked".into(),
        };
    }

    if candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(Gathered { candidates, reason, used_seed }))
}

/// Picks `n` songs from a gathered pool: best by taste first, at most two per
/// artist when picking several so a list is varied.
fn pick_n(
    store: &Store,
    g: &Gathered,
    req: &Request,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<Recommendation>> {
    let mut avoid: HashSet<String> = store.recent_ids(RECENT_WINDOW)?.into_iter().collect();
    avoid.extend(req.exclude.iter().cloned());
    let fresh: Vec<&Song> = g.candidates.iter().filter(|c| !avoid.contains(&c.id)).collect();
    let pool: Vec<&Song> = if fresh.is_empty() { g.candidates.iter().collect() } else { fresh };

    let scores = store.artist_scores()?;
    let mut ranked: Vec<(f64, &Song)> = pool.into_iter().map(|s| (score(s, &scores, rng), s)).collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let cap = if n > 1 { 2 } else { usize::MAX };
    let mut per_artist: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for (_, song) in ranked {
        if out.len() >= n {
            break;
        }
        let c = per_artist.entry(song.artist_key()).or_insert(0);
        if *c >= cap {
            continue;
        }
        *c += 1;
        let mut reason = g.reason.clone();
        if req.artist.is_none() {
            let key = song.artist_key();
            match scores.get(&key) {
                None if scores.len() >= 3 => reason = format!("Introducing: {}", song.artist),
                Some(s) => {
                    let liked_n = s.likes.round() as i64;
                    let seed_key = g.used_seed.as_ref().map(|u| u.artist_key());
                    if liked_n >= 3 && seed_key.as_deref() != Some(&key) {
                        reason.push_str(&format!(" · you've liked {} {liked_n}×", song.artist));
                    }
                }
                _ => {}
            }
        }
        out.push(Recommendation { song: song.clone(), reason });
    }
    Ok(out)
}

/// Picks the next song to play, with a reason.
pub fn next(
    store: &Store,
    src: &dyn CandidateSource,
    req: &Request,
    rng: &mut Rng,
) -> Result<Option<Recommendation>> {
    let Some(g) = gather(store, src, req, rng)? else { return Ok(None) };
    Ok(pick_n(store, &g, req, 1, rng)?.into_iter().next())
}

/// `n` picks from one pool of candidates (for "up next", and the home page).
/// A few network calls in total, not one per song.
pub fn batch(
    store: &Store,
    src: &dyn CandidateSource,
    req: &Request,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<Recommendation>> {
    let Some(g) = gather(store, src, req, rng)? else { return Ok(vec![]) };
    pick_n(store, &g, req, n, rng)
}

fn warm(src: &dyn CandidateSource, seeds: &[Song], rng: &mut Rng) -> (Vec<Song>, Vec<Song>) {
    let picked = rng.sample(seeds, 3);
    let mut seen: HashSet<String> = picked.iter().map(|s| s.id.clone()).collect();
    let mut out = Vec::new();
    for seed in &picked {
        for s in src.related(&seed.id, 25).unwrap_or_default() {
            if playable(&s) && seen.insert(s.id.clone()) {
                out.push(s);
            }
        }
    }
    if out.len() < 10 {
        out.extend(cold(src, rng.choose(SEED_QUERIES).copied().unwrap_or("pop hits")));
    }
    (out, picked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::song;
    use crate::taste::{PlayRecord, PlaySource};

    struct Fake;
    impl CandidateSource for Fake {
        fn search(&self, q: &str, _: usize) -> Result<Vec<Song>> {
            Ok((0..5).map(|i| song(&format!("s{i}"), &format!("{q} {i}"), &format!("Cold{i}"))).collect())
        }
        fn related(&self, id: &str, _: usize) -> Result<Vec<Song>> {
            Ok((0..12).map(|i| song(&format!("{id}-r{i}"), "rel", &format!("Rel{i}"))).collect())
        }
    }

    fn play(st: &Store, s: &Song, c: f64, sl: bool, src: PlaySource) {
        st.record_play(&PlayRecord { song: s.clone(), completion: c, superliked: sl, source: src }).unwrap();
    }

    #[test]
    fn cold_start_mixes_it_up() {
        let st = Store::open_memory().unwrap();
        let r = next(&st, &Fake, &Request::default(), &mut Rng::new(1)).unwrap().unwrap();
        assert!(r.reason.starts_with("Mixing it up"), "{}", r.reason);
    }

    #[test]
    fn warm_seeds_from_a_liked_song() {
        let st = Store::open_memory().unwrap();
        play(&st, &song("seed", "Seed", "Z"), 1.0, false, PlaySource::Recommended);
        // find a seed where the 20% injection doesn't fire
        let mut hit = false;
        for seed in 1..30 {
            let r = next(&st, &Fake, &Request::default(), &mut Rng::new(seed)).unwrap().unwrap();
            if r.reason.starts_with("Because you liked: Seed") {
                assert!(r.song.id.starts_with("seed-r"));
                hit = true;
                break;
            }
        }
        assert!(hit);
    }

    #[test]
    fn superlike_beats_likes_as_seed() {
        let st = Store::open_memory().unwrap();
        play(&st, &song("l", "Liked", "A"), 1.0, false, PlaySource::Recommended);
        play(&st, &song("sl", "Loved", "B"), 0.5, true, PlaySource::Recommended);
        for seed in 1..30 {
            let r = next(&st, &Fake, &Request::default(), &mut Rng::new(seed)).unwrap().unwrap();
            assert!(!r.reason.contains("Liked"), "{}", r.reason);
        }
    }

    #[test]
    fn avoids_recent_and_excluded() {
        let st = Store::open_memory().unwrap();
        play(&st, &song("s0", "x", "Cold0"), 0.9, false, PlaySource::Queue);
        let req = Request { exclude: vec!["s1".into()], ..Default::default() };
        for seed in 1..20 {
            let r = next(&st, &Fake, &req, &mut Rng::new(seed)).unwrap().unwrap();
            if r.reason.starts_with("Mixing") {
                assert!(r.song.id != "s0" && r.song.id != "s1");
            }
        }
    }

    #[test]
    fn disliked_artist_loses() {
        let st = Store::open_memory().unwrap();
        for i in 0..3 {
            play(&st, &song(&format!("x{i}"), "x", "Cold0"), 0.0, false, PlaySource::Recommended);
        }
        let req = Request { genre: Some("jazz".into()), ..Default::default() };
        for seed in 1..20 {
            let r = next(&st, &Fake, &req, &mut Rng::new(seed)).unwrap().unwrap();
            assert_ne!(r.song.artist, "Cold0");
        }
    }

    #[test]
    fn artist_and_genre_requests() {
        let st = Store::open_memory().unwrap();
        let r = next(&st, &Fake, &Request { artist: Some("Queen".into()), ..Default::default() }, &mut Rng::new(2)).unwrap().unwrap();
        assert_eq!(r.reason, "Playing: Queen");
        let r = next(&st, &Fake, &Request { genre: Some("more_energy".into()), requested: true, ..Default::default() }, &mut Rng::new(2)).unwrap().unwrap();
        assert_eq!(r.reason, "You wanted more energy");
    }

    #[test]
    fn batch_is_cheap_and_varied() {
        use std::cell::Cell;
        struct Counting(Cell<u32>);
        impl CandidateSource for Counting {
            fn search(&self, q: &str, l: usize) -> Result<Vec<Song>> {
                self.0.set(self.0.get() + 1);
                Fake.search(q, l)
            }
            fn related(&self, id: &str, l: usize) -> Result<Vec<Song>> {
                self.0.set(self.0.get() + 1);
                Fake.related(id, l)
            }
        }
        let st = Store::open_memory().unwrap();
        play(&st, &song("seed", "Seed", "Z"), 1.0, false, PlaySource::Recommended);
        let src = Counting(Cell::new(0));
        let b = batch(&st, &src, &Request::default(), 10, &mut Rng::new(9)).unwrap();
        assert!(src.0.get() <= 5, "one pool, not one call per song: {}", src.0.get());
        assert_eq!(b.len(), 10);
        let mut per: HashMap<String, usize> = HashMap::new();
        for r in &b {
            *per.entry(r.song.artist_key()).or_default() += 1;
        }
        assert!(per.values().all(|c| *c <= 2), "{per:?}");
    }

    #[test]
    fn batch_has_no_duplicates() {
        let st = Store::open_memory().unwrap();
        let b = batch(&st, &Fake, &Request::default(), 4, &mut Rng::new(5)).unwrap();
        let ids: HashSet<_> = b.iter().map(|r| r.song.id.clone()).collect();
        assert_eq!(ids.len(), b.len());
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn searches_feed_the_picks() {
        let st = Store::open_memory().unwrap();
        st.add_search("arijit singh").unwrap();
        let mut hit = false;
        for seed in 1..40 {
            let r = next(&st, &Fake, &Request::default(), &mut Rng::new(seed)).unwrap().unwrap();
            if r.reason == "Because you searched for “arijit singh”" {
                assert!(r.song.title.starts_with("arijit singh"), "candidates came from that query: {}", r.song.title);
                hit = true;
            }
        }
        assert!(hit, "a recent search seeds some picks");
        // No searches: never mentioned.
        let empty = Store::open_memory().unwrap();
        for seed in 1..20 {
            let r = next(&empty, &Fake, &Request::default(), &mut Rng::new(seed)).unwrap().unwrap();
            assert!(!r.reason.contains("searched"));
        }
    }

    #[test]
    fn saved_playlists_seed_picks() {
        let st = Store::open_memory().unwrap();
        let id = st.create_playlist("Road").unwrap();
        st.add_to_playlist(id, &[song("pl1", "Saved Song", "Z")]).unwrap();
        let mut hit = false;
        for seed in 1..30 {
            let r = next(&st, &Fake, &Request::default(), &mut Rng::new(seed)).unwrap().unwrap();
            if r.reason.contains("is in your playlist Road") {
                assert!(r.song.id.starts_with("pl1-r"), "picked from what follows the saved song");
                hit = true;
                break;
            }
        }
        assert!(hit, "with no plays yet, a saved playlist already shapes the picks");
    }

    #[test]
    fn moods_are_known_genres() {
        for m in MOOD_KEYS {
            assert!(GENRES.iter().any(|g| g.0 == *m), "{m}");
        }
        let st = Store::open_memory().unwrap();
        let r = next(&st, &Fake, &Request { genre: Some("chill".into()), requested: true, ..Default::default() }, &mut Rng::new(3)).unwrap().unwrap();
        assert_eq!(r.reason, "You asked for: Chill");
    }

    #[test]
    fn query_formatting() {
        assert_eq!(format_query("90s pop hits"), "90s Pop");
        assert_eq!(format_query("jazz standards"), "Jazz Standards");
    }
}
