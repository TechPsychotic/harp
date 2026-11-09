use crate::home_cache::{now_secs, HomeCache, HOME_TTL_SECS};
use crate::spotify_state::{PendingSignIn, SPOTIFY_CANCEL, SPOTIFY_PENDING};
use harp_core::import as imp;
use harp_core::queue::{Queue, Repeat};
use harp_core::recommend::{self, CandidateSource, Request};
use harp_core::rng::Rng;
use harp_core::taste::{PlayRecord, PlaySource, SkipReason, Store};
use harp_core::Song;
use harp_core::export;
use harp_ytdlp::{cache, install, lyrics as lrc, proxy::Proxy, spotify, thumbs, ytmusic, Quality, Tool, YtDlp};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct SongView {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub thumbnail: String,
    pub liked: bool,
    pub downloaded: bool,
}

#[derive(Clone, Debug)]
pub struct QueueView {
    pub current: Option<SongView>,
    pub user: Vec<SongView>,
    pub autoplay: Vec<SongView>,
    /// "off", "all" or "one"
    pub repeat: String,
}

#[derive(Clone, Debug)]
pub struct StreamView {
    /// A URL, or a file path when `local`.
    pub url: String,
    pub local: bool,
    pub headers: Vec<(String, String)>,
    /// Unix seconds, if known.
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Pick {
    pub song: SongView,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct TasteStats {
    pub plays: u32,
    pub liked: u32,
    pub artists: u32,
}

/// Where things live. On desktop only `data_dir` is needed; on Android the app
/// passes the bundled Python and QuickJS (native libraries) too.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub data_dir: String,
    /// Android: path of the Python executable (libpython.so).
    pub python: Option<String>,
    /// Android: PYTHONHOME (the unpacked standard library's parent).
    pub python_home: Option<String>,
    /// Android: QuickJS executable for YouTube's player challenges.
    pub quickjs: Option<String>,
    /// Android: directory of system certificates.
    pub ca_dir: Option<String>,
    /// Use a yt-dlp that is already on this path instead of downloading one.
    pub ytdlp_override: Option<String>,
    /// Android: a yt-dlp shipped inside the app, used only if downloading one fails.
    pub bundled_ytdlp: Option<String>,
}

struct Engine {
    yt: YtDlp,
    downloads_dir: PathBuf,
    cache_dir: PathBuf,
    thumbs_dir: PathBuf,
    /// Writes (plays, likes) never wait behind a recommendation's network calls.
    store: Mutex<Store>,
    rec_store: Mutex<Store>,
    queue: Mutex<Queue>,
    rng: Mutex<Rng>,
    _proxy: Option<Proxy>,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn eng() -> Result<&'static Engine, String> {
    ENGINE.get().ok_or_else(|| "engine not started".to_string())
}

fn e<T: std::fmt::Display>(x: T) -> String {
    x.to_string()
}

fn view(s: &Song, liked: bool, downloaded: bool) -> SongView {
    SongView {
        id: s.id.clone(),
        title: s.title.clone(),
        artist: s.artist.clone(),
        artist_id: s.artist_id.clone(),
        album: s.album.clone(),
        duration_secs: s.duration_secs,
        thumbnail: s.thumbnail_or_default(),
        liked,
        downloaded,
    }
}

fn song(v: &SongView) -> Song {
    Song {
        id: v.id.clone(),
        title: v.title.clone(),
        artist: v.artist.clone(),
        artist_id: v.artist_id.clone(),
        album: v.album.clone(),
        duration_secs: v.duration_secs,
        thumbnail: Some(v.thumbnail.clone()),
    }
}

fn views(en: &Engine, v: impl Iterator<Item = Song>) -> Vec<SongView> {
    let st = en.store.lock().unwrap();
    v.map(|s| {
        let liked = st.is_liked(&s.id).unwrap_or(false);
        let dl = st.downloaded_path(&s.id).ok().flatten().is_some();
        view(&s, liked, dl)
    })
    .collect()
}

fn queue_view(en: &Engine) -> QueueView {
    let q = en.queue.lock().unwrap();
    let cur: Vec<Song> = q.current().cloned().into_iter().collect();
    let user: Vec<Song> = q.user_queue().cloned().collect();
    let auto: Vec<Song> = q.autoplay().cloned().collect();
    let repeat = match q.repeat {
        Repeat::Off => "off",
        Repeat::All => "all",
        Repeat::One => "one",
    }
    .to_string();
    // Every change to the queue passes through here, so this is where it is saved:
    // the whole queue comes back after a restart.
    let saved = serde_json::to_string(&*q).unwrap_or_default();
    drop(q);
    if let Ok(st) = en.store.lock() {
        let _ = st.kv_set("queue", &saved);
    }
    QueueView {
        current: views(en, cur.into_iter()).into_iter().next(),
        user: views(en, user.into_iter()),
        autoplay: views(en, auto.into_iter()),
        repeat,
    }
}

/// Starts the engine: opens the database, makes sure yt-dlp is installed.
/// Safe to call again (does nothing the second time).
pub fn start_engine(cfg: EngineConfig) -> Result<(), String> {
    if ENGINE.get().is_some() {
        return Ok(());
    }
    let data = PathBuf::from(&cfg.data_dir);
    std::fs::create_dir_all(&data).map_err(e)?;
    let db = data.join("harp.db");
    let tools = data.join("tools");

    let mut proxy = None;
    let tool = if let Some(p) = &cfg.ytdlp_override {
        Tool::binary(p)
    } else if let Some(py) = &cfg.python {
        // Android: bundled static Python runs the verified yt-dlp zipapp, behind a proxy.
        // Download (and verify) the current yt-dlp; if that fails and there is no earlier
        // copy, fall back to the one shipped in the app.
        let zip = match install::ensure(&tools, install::asset_name("android", "aarch64", true), false) {
            Ok(p) => p,
            Err(err) => match cfg.bundled_ytdlp.as_deref().map(PathBuf::from).filter(|p| p.is_file()) {
                Some(p) => p,
                None => return Err(e(err)),
            },
        };
        let mut t = Tool::zipapp(py, zip);
        let px = Proxy::start().map_err(e)?;
        t.proxy = Some(px.url.clone());
        proxy = Some(px);
        if let Some(home) = &cfg.python_home {
            t.env.push(("PYTHONHOME".into(), home.clone()));
        }
        if let Some(ca) = &cfg.ca_dir {
            let out = data.join("ca-bundle.pem");
            if harp_ytdlp::android::write_ca_bundle(std::path::Path::new(ca), &out).is_ok() {
                t.env.push(("SSL_CERT_FILE".into(), out.to_string_lossy().into_owned()));
            }
        }
        t.env.push(("HOME".into(), cfg.data_dir.clone()));
        t.env.push(("TMPDIR".into(), data.join("tmp").to_string_lossy().into_owned()));
        let _ = std::fs::create_dir_all(data.join("tmp"));
        if let Some(q) = &cfg.quickjs {
            t.js_runtime = Some(format!("quickjs:{q}"));
        }
        t
    } else {
        let asset = install::asset_name(std::env::consts::OS, std::env::consts::ARCH, false);
        Tool::binary(install::ensure(&tools, asset, false).map_err(e)?)
    };

    let store = Store::open(&db).map_err(e)?;
    // The queue as it was when the app last closed.
    let queue: Queue = store.kv_get("queue").ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    let engine = Engine {
        downloads_dir: data.join("downloads"),
        cache_dir: data.join("cache"),
        thumbs_dir: data.join("thumbs"),
        yt: YtDlp::new(tool),
        store: Mutex::new(store),
        rec_store: Mutex::new(Store::open(&db).map_err(e)?),
        queue: Mutex::new(queue),
        rng: Mutex::new(Rng::from_time()),
        _proxy: proxy,
    };
    let _ = ENGINE.set(engine);
    Ok(())
}

pub fn ytdlp_version() -> Result<String, String> {
    eng()?.yt.version().map_err(e)
}

pub fn search(query: String) -> Result<Vec<SongView>, String> {
    let en = eng()?;
    // Not saved to history here: typing is not searching. The app saves a search when
    // you submit it or open one of its results (`search_history_add`).
    let found = en.yt.search_songs(&query, 20).map_err(e)?;
    Ok(views(en, found.into_iter()))
}

/// A stream for a song: its downloaded file if it has one, else a URL at the
/// chosen quality ("low", "medium" or "high").
pub fn resolve_stream(id: String, quality: String) -> Result<StreamView, String> {
    let en = eng()?;
    if let Some(p) = en.store.lock().unwrap().downloaded_path(&id).map_err(e)? {
        if std::path::Path::new(&p).is_file() {
            return Ok(StreamView { url: p, local: true, headers: vec![], expires_at: None });
        }
    }
    // A song fetched ahead of time plays from disk: instant, and no network needed.
    if let Some(p) = cache::find(&en.cache_dir, &id) {
        cache::touch(&p);
        return Ok(StreamView { url: p.to_string_lossy().into_owned(), local: true, headers: vec![], expires_at: None });
    }
    let s = en.yt.resolve(&id, Quality::parse(&quality)).map_err(e)?;
    Ok(StreamView { url: s.url, local: false, headers: s.headers.into_iter().collect(), expires_at: s.expires_at })
}

/// Drops a cached stream URL (after the player got a 403).
pub fn forget_stream(id: String) -> Result<(), String> {
    eng()?.yt.forget(&id);
    Ok(())
}

// --- queue ---

pub fn queue_state() -> Result<QueueView, String> {
    Ok(queue_view(eng()?))
}

pub fn queue_play_now(s: SongView) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().play_now(song(&s));
    Ok(queue_view(en))
}

pub fn queue_play_next(s: SongView) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().play_next(song(&s));
    Ok(queue_view(en))
}

pub fn queue_add(s: SongView) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().add_to_queue(song(&s));
    Ok(queue_view(en))
}

pub fn queue_next(auto_advance: bool) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().next(auto_advance);
    Ok(queue_view(en))
}

pub fn queue_previous() -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().previous();
    Ok(queue_view(en))
}

/// Jumps to an item: `section` is "user" or "auto". The songs before it are skipped.
pub fn queue_jump(section: String, index: u32) -> Result<QueueView, String> {
    let en = eng()?;
    {
        let mut q = en.queue.lock().unwrap();
        if section == "auto" {
            q.jump_auto(index as usize);
        } else {
            q.jump_user(index as usize);
        }
    }
    Ok(queue_view(en))
}

/// Replaces the autoplay section with these songs (used for offline mode: your downloads).
pub fn queue_set_autoplay(songs: Vec<SongView>) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().set_autoplay(songs.iter().map(song).collect());
    Ok(queue_view(en))
}

pub fn queue_remove(index: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().remove_user(index as usize);
    Ok(queue_view(en))
}

pub fn queue_move(from: u32, to: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().move_user(from as usize, to as usize);
    Ok(queue_view(en))
}

pub fn queue_set_repeat(mode: String) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().repeat = match mode.as_str() {
        "all" => Repeat::All,
        "one" => Repeat::One,
        _ => Repeat::Off,
    };
    Ok(queue_view(en))
}

pub fn queue_shuffle() -> Result<QueueView, String> {
    let en = eng()?;
    {
        let mut rng = en.rng.lock().unwrap();
        en.queue.lock().unwrap().shuffle_upcoming(&mut rng);
    }
    Ok(queue_view(en))
}

pub fn queue_clear() -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().clear_user();
    Ok(queue_view(en))
}

// --- taste and the recommender ---

/// Records how much of a song was heard. `source`: "recommended", "searched" or "queue".
pub fn record_play(s: SongView, completion: f64, superliked: bool, source: String) -> Result<(), String> {
    let src = match source.as_str() {
        "searched" => PlaySource::Searched,
        "queue" => PlaySource::Queue,
        _ => PlaySource::Recommended,
    };
    eng()?
        .store
        .lock()
        .unwrap()
        .record_play(&PlayRecord { song: song(&s), completion, superliked, source: src })
        .map_err(e)
}

/// `reason`: "wrong_genre", "not_artist", "not_mood" or "overplayed".
pub fn skip_reason(s: SongView, reason: String) -> Result<(), String> {
    let r = match reason.as_str() {
        "wrong_genre" => SkipReason::WrongGenre,
        "not_artist" => SkipReason::NotThisArtist,
        "overplayed" => SkipReason::Overplayed,
        _ => SkipReason::NotTheMood,
    };
    eng()?.store.lock().unwrap().skip_reason(&song(&s), r).map_err(e)
}

/// Toggles the heart; returns the new state.
pub fn toggle_like(s: SongView) -> Result<bool, String> {
    eng()?.store.lock().unwrap().toggle_like(&song(&s)).map_err(e)
}

pub fn liked_songs() -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let v = en.store.lock().unwrap().liked_songs().map_err(e)?;
    Ok(views(en, v.into_iter()))
}

pub fn history(limit: u32) -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let v = en.store.lock().unwrap().history(limit as usize).map_err(e)?;
    Ok(views(en, v.into_iter()))
}

pub fn taste_stats() -> Result<TasteStats, String> {
    let s = eng()?.store.lock().unwrap().stats().map_err(e)?;
    Ok(TasteStats { plays: s.plays, liked: s.liked, artists: s.artists })
}

pub fn genres() -> Vec<(String, String)> {
    recommend::GENRES.iter().map(|g| (g.0.to_string(), g.1.to_string())).collect()
}

fn request(genre: Option<String>, artist: Option<String>, queued: Vec<String>) -> Request {
    Request { requested: genre.is_some() || artist.is_some(), genre, artist, exclude: queued }
}

/// The next recommended song, with the reason it was picked.
pub fn recommend_next(genre: Option<String>, artist: Option<String>) -> Result<Option<Pick>, String> {
    let en = eng()?;
    let exclude = {
        let q = en.queue.lock().unwrap();
        q.current().into_iter().chain(q.user_queue()).chain(q.autoplay()).map(|s| s.id.clone()).collect()
    };
    let st = en.rec_store.lock().unwrap();
    let mut rng = en.rng.lock().unwrap().clone();
    let r = recommend::next(&st, &en.yt, &request(genre, artist, exclude), &mut rng).map_err(e)?;
    drop(st);
    Ok(r.map(|r| Pick {
        song: views(en, std::iter::once(r.song)).remove(0),
        reason: r.reason,
    }))
}

/// Fills the autoplay ("up next") section after the user's own queue.
pub fn fill_autoplay(count: u32, genre: Option<String>) -> Result<QueueView, String> {
    let en = eng()?;
    let exclude: Vec<String> = {
        let q = en.queue.lock().unwrap();
        q.current().into_iter().chain(q.user_queue()).map(|s| s.id.clone()).collect()
    };
    let picks = {
        let st = en.rec_store.lock().unwrap();
        let mut rng = en.rng.lock().unwrap().clone();
        recommend::batch(&st, &en.yt, &request(genre, None, exclude), count as usize, &mut rng).map_err(e)?
    };
    en.queue.lock().unwrap().set_autoplay(picks.into_iter().map(|p| p.song).collect());
    Ok(queue_view(en))
}

/// Remembers what was playing so a restart can offer "Resume".
pub fn save_last_played(s: Option<SongView>) -> Result<(), String> {
    let st = eng()?.store.lock().unwrap();
    match s {
        Some(s) => st.kv_set("last_played", &serde_json_string(&song(&s))),
        None => st.kv_set("last_played", ""),
    }
    .map_err(e)
}

pub fn last_played() -> Result<Option<SongView>, String> {
    let en = eng()?;
    let raw = en.store.lock().unwrap().kv_get("last_played").map_err(e)?.unwrap_or_default();
    Ok(song_from_json(&raw).map(|s| views(en, std::iter::once(s)).remove(0)))
}

fn serde_json_string(s: &Song) -> String {
    harp_core::model::to_json(s)
}

fn song_from_json(raw: &str) -> Option<Song> {
    harp_core::model::from_json(raw)
}


// --- settings ---

pub fn setting_get(key: String) -> Result<Option<String>, String> {
    eng()?.store.lock().unwrap().kv_get(&format!("setting:{key}")).map_err(e)
}

pub fn setting_set(key: String, value: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().kv_set(&format!("setting:{key}"), &value).map_err(e)
}

// --- playlists ---

#[derive(Clone, Debug)]
pub struct PlaylistView {
    pub id: i64,
    pub name: String,
    pub count: u32,
    pub pinned: bool,
    /// Artwork of the first songs, for a cover mosaic.
    pub covers: Vec<String>,
}

pub fn playlists() -> Result<Vec<PlaylistView>, String> {
    let v = eng()?.store.lock().unwrap().playlists().map_err(e)?;
    Ok(v.into_iter().map(|p| PlaylistView { id: p.id, name: p.name, count: p.count, pinned: p.pinned, covers: p.covers }).collect())
}

/// Pins a playlist to the top of the list (or unpins it).
pub fn playlist_pin(id: i64, pinned: bool) -> Result<(), String> {
    eng()?.store.lock().unwrap().pin_playlist(id, pinned).map_err(e)
}

/// Lyric lines in English letters ("Hinglish"). An empty string means the line
/// needs nothing (it is already in Latin letters).
#[flutter_rust_bridge::frb(sync)]
pub fn romanize_lines(lines: Vec<String>) -> Vec<String> {
    lines.iter().map(|l| harp_core::translit::romanize(l).unwrap_or_default()).collect()
}

pub fn playlist_create(name: String) -> Result<i64, String> {
    eng()?.store.lock().unwrap().create_playlist(&name).map_err(e)
}

pub fn playlist_rename(id: i64, name: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().rename_playlist(id, &name).map_err(e)
}

pub fn playlist_delete(id: i64) -> Result<(), String> {
    eng()?.store.lock().unwrap().delete_playlist(id).map_err(e)
}

pub fn playlist_songs(id: i64) -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let v = en.store.lock().unwrap().playlist_songs(id).map_err(e)?;
    Ok(views(en, v.into_iter()))
}

/// Adds songs (skipping ones already there); returns how many were added.
pub fn playlist_add(id: i64, songs: Vec<SongView>) -> Result<u32, String> {
    let s: Vec<Song> = songs.iter().map(song).collect();
    eng()?.store.lock().unwrap().add_to_playlist(id, &s).map_err(e)
}

pub fn playlist_remove(id: i64, index: u32) -> Result<(), String> {
    eng()?.store.lock().unwrap().remove_from_playlist(id, index as usize).map_err(e)
}

pub fn playlist_move(id: i64, from: u32, to: u32) -> Result<(), String> {
    eng()?.store.lock().unwrap().move_in_playlist(id, from as usize, to as usize).map_err(e)
}

/// Replaces the up-next queue with a playlist and starts it. `shuffle` mixes it first.
pub fn queue_play_playlist(id: i64, shuffle: bool) -> Result<QueueView, String> {
    let en = eng()?;
    let mut songs = en.store.lock().unwrap().playlist_songs(id).map_err(e)?;
    if shuffle {
        en.rng.lock().unwrap().shuffle(&mut songs);
    }
    {
        let mut q = en.queue.lock().unwrap();
        q.clear_user();
        let mut it = songs.into_iter();
        if let Some(first) = it.next() {
            q.play_now(first);
            for s in it {
                q.add_to_queue(s);
            }
        }
    }
    Ok(queue_view(en))
}

// --- downloads (offline) ---

#[derive(Clone, Debug)]
pub struct DownloadView {
    pub song: SongView,
    pub bytes: u64,
    pub quality: String,
}

/// Downloads a song's audio for offline playback.
pub fn download_song(s: SongView, quality: String) -> Result<SongView, String> {
    let en = eng()?;
    let (path, bytes) = en.yt.download(&s.id, &en.downloads_dir, Quality::parse(&quality)).map_err(e)?;
    en.store
        .lock()
        .unwrap()
        .record_download(&song(&s), &path.to_string_lossy(), bytes, &quality)
        .map_err(e)?;
    Ok(SongView { downloaded: true, ..s })
}

pub fn downloads() -> Result<Vec<DownloadView>, String> {
    let en = eng()?;
    let d = en.store.lock().unwrap().downloads().map_err(e)?;
    Ok(d
        .into_iter()
        .filter(|d| std::path::Path::new(&d.path).is_file())
        .map(|d| {
            let liked = en.store.lock().unwrap().is_liked(&d.song.id).unwrap_or(false);
            DownloadView { song: view(&d.song, liked, true), bytes: d.bytes, quality: d.quality }
        })
        .collect())
}

pub fn delete_download(id: String) -> Result<(), String> {
    let en = eng()?;
    if let Some(p) = en.store.lock().unwrap().delete_download(&id).map_err(e)? {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

// --- thumbnails ---

/// The cached file for a cover URL (downloaded on first use); empty if it can't be fetched.
pub fn cached_thumbnail(url: String) -> String {
    let Ok(en) = eng() else { return String::new() };
    thumbs::fetch(&en.thumbs_dir, &url).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Starts fetching covers in the background (4 at a time) and returns at once.
pub fn precache_thumbnails(urls: Vec<String>) {
    let Ok(en) = eng() else { return };
    let dir = en.thumbs_dir.clone();
    let queue = std::sync::Arc::new(Mutex::new(urls.into_iter()));
    for _ in 0..4 {
        let (q, dir) = (queue.clone(), dir.clone());
        std::thread::spawn(move || loop {
            let next = q.lock().unwrap().next();
            match next {
                Some(u) => {
                    let _ = thumbs::fetch(&dir, &u);
                }
                None => break,
            }
        });
    }
}

pub fn thumbnail_cache_bytes() -> u64 {
    eng().map(|en| thumbs::size(&en.thumbs_dir)).unwrap_or(0)
}

pub fn clear_thumbnail_cache() {
    if let Ok(en) = eng() {
        thumbs::clear(&en.thumbs_dir);
    }
}

// --- home page ---

#[derive(Clone, Debug)]
pub struct HomeFeed {
    pub recommended: Vec<SongView>,
    pub similar_to: Option<SongView>,
    pub similar: Vec<SongView>,
    pub recent: Vec<SongView>,
    pub liked: Vec<SongView>,
    pub downloads: Vec<SongView>,
    pub playlists: Vec<PlaylistView>,
    /// False when the recommendations came from the saved copy.
    pub fresh: bool,
}

/// Everything the home page shows. The recommendations are kept for three
/// hours so opening the app is instant (and works offline); `refresh` forces
/// new ones. With `offline`, nothing touches the network.
pub fn home_feed(refresh: bool, offline: bool) -> Result<HomeFeed, String> {
    let en = eng()?;
    let cached: Option<HomeCache> = en
        .store
        .lock()
        .unwrap()
        .kv_get("home_cache")
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok());
    let stale = cached.as_ref().map_or(true, |c| now_secs().saturating_sub(c.ts) > HOME_TTL_SECS);
    let mut fresh = false;
    let mut cache = cached.unwrap_or_default();
    if !offline && (refresh || stale) {
        let attempt = (|| -> Result<HomeCache, String> {
            let st = en.rec_store.lock().unwrap();
            let mut rng = en.rng.lock().unwrap().clone();
            let recs = recommend::batch(&st, &en.yt, &Request::default(), 14, &mut rng).map_err(e)?;
            let last = st.history(1).map_err(e)?.into_iter().next();
            drop(st);
            let similar = match &last {
                Some(s) => en.yt.related(&s.id, 12).unwrap_or_default(),
                None => vec![],
            };
            Ok(HomeCache { ts: now_secs(), recommended: recs.into_iter().map(|r| r.song).collect(), similar_to: last, similar })
        })();
        if let Ok(c) = attempt {
            if !c.recommended.is_empty() {
                let _ = en.store.lock().unwrap().kv_set("home_cache", &serde_json::to_string(&c).unwrap_or_default());
                cache = c;
                fresh = true;
            }
        }
    }
    let st_recent = en.store.lock().unwrap().history(14).map_err(e)?;
    let liked = en.store.lock().unwrap().liked_songs().map_err(e)?;
    let dls: Vec<Song> = downloads()?.into_iter().map(|d| song(&d.song)).collect();
    Ok(HomeFeed {
        recommended: views(en, cache.recommended.into_iter()),
        similar_to: cache.similar_to.map(|s| views(en, std::iter::once(s)).remove(0)),
        similar: views(en, cache.similar.into_iter()),
        recent: views(en, st_recent.into_iter()),
        liked: views(en, liked.into_iter().take(14)),
        downloads: views(en, dls.into_iter().take(14)),
        playlists: playlists()?,
        fresh,
    })
}

pub fn moods() -> Vec<(String, String)> {
    recommend::MOOD_KEYS
        .iter()
        .filter_map(|k| recommend::GENRES.iter().find(|g| g.0 == *k))
        .map(|g| (g.0.to_string(), g.1.to_string()))
        .collect()
}

// --- importing playlists ---

#[derive(Clone, Debug)]
pub struct ImportTrackView {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ImportPlaylistView {
    pub name: String,
    pub tracks: Vec<ImportTrackView>,
    /// When the source already gave playable songs (a YouTube playlist), no matching is needed.
    pub resolved: Vec<SongView>,
    /// Something worth telling the user (a source that cuts long lists short).
    pub notice: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MatchView {
    pub best: Option<SongView>,
    pub score: f64,
    /// "confident", "weak" or "none"
    pub confidence: String,
    pub alternatives: Vec<SongView>,
}

fn import_view(p: imp::ImportedPlaylist) -> ImportPlaylistView {
    ImportPlaylistView {
        name: p.name,
        tracks: p
            .tracks
            .into_iter()
            .map(|t| ImportTrackView { title: t.title, artist: t.artist, album: t.album, duration_secs: t.duration_secs })
            .collect(),
        resolved: vec![],
        notice: None,
    }
}

/// Parses exported or pasted playlists: Spotify's data export (JSON) or Exportify CSV,
/// Apple Music's exported text, M3U, or lines of "Artist - Title".
pub fn import_parse(text: String, name_hint: String) -> Vec<ImportPlaylistView> {
    imp::parse(&text, &name_hint).into_iter().map(import_view).collect()
}

fn spotify_embed_url(url: &str) -> Option<String> {
    let path = url.split("open.spotify.com/").nth(1)?;
    let mut parts = path.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let mut kind = parts.next()?;
    if kind.starts_with("intl-") {
        kind = parts.next()?; // open.spotify.com/intl-de/playlist/...
    }
    let id = parts.next()?;
    matches!(kind, "playlist" | "album").then(|| format!("https://open.spotify.com/embed/{kind}/{id}"))
}

/// The playlist id of a YouTube / YouTube Music link (`...?list=PL...`).
fn youtube_list_id(url: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    let id = q.split(['&', '#']).find_map(|kv| kv.strip_prefix("list="))?;
    (!id.is_empty()).then(|| id.to_string())
}

/// Spotify's public pages hand out at most this many songs of a playlist.
const SPOTIFY_PAGE_CAP: usize = 100;

/// Reads a public playlist link: Spotify, YouTube or YouTube Music.
pub fn import_link(url: String) -> Result<Vec<ImportPlaylistView>, String> {
    let en = eng()?;
    let url = url.trim().to_string();
    if url.contains("open.spotify.com") {
        let embed = spotify_embed_url(&url).ok_or("That Spotify link isn't a playlist or album.")?;
        let html = thumbs::get_text(&embed).map_err(e)?;
        let p = imp::parse_spotify_embed(&html).ok_or("Couldn't read that Spotify playlist (is it public?).")?;
        let capped = p.tracks.len() >= SPOTIFY_PAGE_CAP;
        let mut v = import_view(p);
        if capped {
            v.notice = Some(
                "Spotify's public page only shares the first 100 songs of a playlist, so this one may be longer. For the whole playlist, use Spotify's data export or an Exportify CSV (Import › From a file): they have no limit."
                    .into(),
            );
        }
        return Ok(vec![v]);
    }
    if url.contains("youtube.com") || url.contains("youtu.be") {
        // YouTube Music reads a playlist in full (100 at a time), with proper artists.
        let mut resolved: Vec<SongView> = vec![];
        let mut name = "YouTube playlist".to_string();
        if let Some(list) = youtube_list_id(&url) {
            if let Ok((c, songs)) = ytmusic::open(&format!("VL{list}")) {
                if !songs.is_empty() {
                    if !c.title.is_empty() {
                        name = c.title;
                    }
                    resolved = views(en, songs.into_iter());
                }
            }
        }
        if resolved.is_empty() {
            let songs = en.yt.playlist(&url, 5000).map_err(e)?;
            resolved = views(en, songs.into_iter());
        }
        if resolved.is_empty() {
            return Err("No songs found at that link (is the playlist public or unlisted?).".into());
        }
        return Ok(vec![ImportPlaylistView { name, tracks: vec![], resolved, notice: None }]);
    }
    if url.contains("music.apple.com") {
        let html = thumbs::get_text(&url).map_err(e)?;
        let p = imp::parse_apple_music_page(&html).ok_or("Couldn't read that Apple Music page (is the playlist public?).")?;
        let capped = p.tracks.len() >= 100;
        let mut v = import_view(p);
        if capped {
            v.notice = Some("Apple Music's public page may not list every song of a long playlist. For the whole playlist, export it from the Music app (File › Library › Export Playlist) and import the file.".into());
        }
        return Ok(vec![v]);
    }
    if url.contains("amazon.") {
        return Err("Amazon Music doesn't share playlist contents in links or to other apps. Paste the song list (Artist - Title per line), or import a CSV made by a converter.".into());
    }
    Err("Paste a Spotify, Apple Music, YouTube or YouTube Music playlist link.".into())
}

/// Finds the best match for a track on YouTube Music, with alternatives for the review screen.
pub fn import_match(t: ImportTrackView) -> Result<MatchView, String> {
    let en = eng()?;
    let track = imp::ImportTrack { title: t.title, artist: t.artist, album: t.album, duration_secs: t.duration_secs };
    let q = format!("{} {}", track.artist, track.title);
    let cands = en.yt.search_songs(q.trim(), 6).map_err(e)?;
    let ranked = imp::rank(&track, &cands);
    let score = ranked.first().map_or(0.0, |r| r.1);
    let confidence = match imp::confidence(score) {
        imp::Confidence::Confident => "confident",
        imp::Confidence::Weak => "weak",
        imp::Confidence::None => "none",
    };
    let mut v = views(en, ranked.into_iter().map(|r| r.0));
    // With no usable match the candidates still show up as alternatives.
    let best = if v.is_empty() || confidence == "none" { None } else { Some(v.remove(0)) };
    v.truncate(4);
    Ok(MatchView { best, score, confidence: confidence.to_string(), alternatives: v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_list_ids() {
        assert_eq!(youtube_list_id("https://music.youtube.com/playlist?list=PLabc123&si=x").as_deref(), Some("PLabc123"));
        assert_eq!(youtube_list_id("https://www.youtube.com/watch?v=abc&list=RDabc").as_deref(), Some("RDabc"));
        assert_eq!(youtube_list_id("https://www.youtube.com/watch?v=abc"), None);
    }

    #[test]
    fn spotify_links() {
        assert_eq!(
            spotify_embed_url("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc").as_deref(),
            Some("https://open.spotify.com/embed/playlist/37i9dQZF1DXcBWIGoYBM5M")
        );
        assert_eq!(
            spotify_embed_url("https://open.spotify.com/intl-de/album/1A2B").as_deref(),
            Some("https://open.spotify.com/embed/album/1A2B")
        );
        assert_eq!(spotify_embed_url("https://open.spotify.com/track/xyz"), None);
        assert_eq!(spotify_embed_url("https://example.com/playlist/1"), None);
    }
}


// --- search history ---

pub fn search_history(limit: u32) -> Result<Vec<String>, String> {
    eng()?.store.lock().unwrap().recent_searches(limit as usize).map_err(e)
}

/// Saves a search: called when you press search / enter, or open a result.
pub fn search_history_add(q: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().add_search(&q).map_err(e)
}

pub fn search_history_remove(q: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().remove_search(&q).map_err(e)
}

pub fn search_history_clear() -> Result<(), String> {
    eng()?.store.lock().unwrap().clear_searches().map_err(e)
}

// --- albums, playlists and podcasts ---

#[derive(Clone, Debug)]
pub struct CollectionView {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
    /// "album", "playlist" or "podcast"
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct CollectionDetail {
    pub info: CollectionView,
    pub songs: Vec<SongView>,
    /// For an artist: their albums, singles, playlists and similar artists.
    pub related: Vec<CollectionView>,
}

fn collection_view(c: ytmusic::Collection) -> CollectionView {
    CollectionView { id: c.id, title: c.title, subtitle: c.subtitle, thumbnail: c.thumbnail.unwrap_or_default(), kind: c.kind }
}

/// Search albums, playlists or podcasts (`kind`: "albums", "playlists", "podcasts").
pub fn search_collections(kind: String, query: String) -> Result<Vec<CollectionView>, String> {
    let en = eng()?;
    let k = ytmusic::Kind::parse(&kind);
    let _ = en;
    let v = ytmusic::search_collections(k, query.trim(), 24).map_err(e)?;
    Ok(v.into_iter().map(collection_view).collect())
}

/// An album, playlist, podcast show or artist, with its songs or episodes (a long
/// playlist is read in full) and, for an artist, their albums and more.
pub fn open_collection(id: String) -> Result<CollectionDetail, String> {
    let en = eng()?;
    if id.starts_with("UC") {
        let (a, songs, related) = ytmusic::open_artist(&id).map_err(e)?;
        return Ok(CollectionDetail {
            info: collection_view(a),
            songs: views(en, songs.into_iter()),
            related: related.into_iter().map(collection_view).collect(),
        });
    }
    let (c, songs) = ytmusic::open(&id).map_err(e)?;
    Ok(CollectionDetail { info: collection_view(c), songs: views(en, songs.into_iter()), related: vec![] })
}

/// Plays a list of songs: the first now, the rest queued after what you already added.
pub fn queue_play_list(songs: Vec<SongView>, shuffle: bool) -> Result<QueueView, String> {
    let en = eng()?;
    let mut list: Vec<Song> = songs.iter().map(song).collect();
    if shuffle {
        en.rng.lock().unwrap().shuffle(&mut list);
    }
    {
        let mut q = en.queue.lock().unwrap();
        q.clear_user();
        let mut it = list.into_iter();
        if let Some(first) = it.next() {
            q.play_now(first);
            for s in it {
                q.add_to_queue(s);
            }
        }
    }
    Ok(queue_view(en))
}

// --- lyrics ---

#[derive(Clone, Debug)]
pub struct LyricLineView {
    pub at_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct LyricsView {
    pub synced: bool,
    pub lines: Vec<LyricLineView>,
    pub source: String,
}

fn lyrics_view(l: harp_core::lyrics::Lyrics) -> LyricsView {
    LyricsView {
        synced: l.synced,
        lines: l.lines.into_iter().map(|x| LyricLineView { at_ms: x.at_ms, text: x.text }).collect(),
        source: l.source,
    }
}

/// Lyrics for a song (synced when available): from the saved copy, else LRCLIB.
/// `None` when there are none. A "none" answer is remembered for three days.
pub fn lyrics_for(s: SongView, allow_network: bool) -> Result<Option<LyricsView>, String> {
    let en = eng()?;
    if let Some((l, at)) = en.store.lock().unwrap().cached_lyrics(&s.id).map_err(e)? {
        if !l.is_empty() {
            return Ok(Some(lyrics_view(l)));
        }
        if now_secs().saturating_sub(at as u64) < 3 * 86400 {
            return Ok(None);
        }
    }
    if !allow_network {
        return Ok(None);
    }
    match lrc::fetch(&s.title, &s.artist, s.album.as_deref(), s.duration_secs) {
        Ok(found) => {
            let l = found.unwrap_or_default();
            let _ = en.store.lock().unwrap().save_lyrics(&s.id, &l);
            Ok((!l.is_empty()).then(|| lyrics_view(l)))
        }
        Err(err) => Err(e(err)),
    }
}

// --- play cache and prefetching ---

static PREFETCHING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
static PREFETCH_TURN: Mutex<()> = Mutex::new(());

/// Fetches songs into the play cache in the background, one at a time, so the
/// next songs start instantly (and still play if the connection drops). Songs
/// already downloaded or cached are skipped. `limit_mb` is the cache size cap.
pub fn prefetch_songs(ids: Vec<String>, quality: String, limit_mb: u32) {
    let Ok(en) = eng() else { return };
    if limit_mb == 0 {
        return;
    }
    let inflight = PREFETCHING.get_or_init(|| Mutex::new(Default::default()));
    for id in ids {
        if en.store.lock().unwrap().downloaded_path(&id).ok().flatten().is_some() || cache::find(&en.cache_dir, &id).is_some() {
            continue;
        }
        if !inflight.lock().unwrap().insert(id.clone()) {
            continue;
        }
        let q = Quality::parse(&quality);
        std::thread::spawn(move || {
            let Ok(en) = eng() else { return };
            {
                let _turn = PREFETCH_TURN.lock().unwrap();
                if cache::find(&en.cache_dir, &id).is_none() {
                    let _ = en.yt.download(&id, &en.cache_dir, q);
                }
                let keep: Vec<String> = vec![id.clone()];
                cache::trim(&en.cache_dir, limit_mb as u64 * 1_048_576, &keep);
            }
            PREFETCHING.get().unwrap().lock().unwrap().remove(&id);
        });
    }
}

pub fn cache_bytes() -> u64 {
    eng().map(|en| cache::total_bytes(&en.cache_dir)).unwrap_or(0)
}

pub fn clear_cache() {
    if let Ok(en) = eng() {
        cache::clear(&en.cache_dir);
    }
}


// --- "All" search ---

#[derive(Clone, Debug)]
pub struct SearchHitView {
    /// "song", "episode", "artist", "album" or "podcast"
    pub kind: String,
    pub song: Option<SongView>,
    pub collection: Option<CollectionView>,
}

/// Songs, episodes, artists, albums and podcasts, ranked by YouTube Music's relevance.
pub fn search_all(query: String) -> Result<Vec<SearchHitView>, String> {
    let en = eng()?;
    let hits = ytmusic::search_all(query.trim(), 40).map_err(e)?;
    Ok(hits
        .into_iter()
        .map(|h| match h {
            ytmusic::Hit::Song(s) => SearchHitView { kind: "song".into(), song: Some(views(en, std::iter::once(s)).remove(0)), collection: None },
            ytmusic::Hit::Episode(s) => SearchHitView { kind: "episode".into(), song: Some(views(en, std::iter::once(s)).remove(0)), collection: None },
            ytmusic::Hit::Collection(c) => SearchHitView { kind: c.kind.clone(), song: None, collection: Some(collection_view(c)) },
        })
        .collect())
}

// --- exporting playlists (Exportify-style files) ---

#[derive(Clone, Debug)]
pub struct ExportFile {
    pub file_name: String,
    pub contents: String,
}

fn export_songs(name: &str, songs: &[Song], format: &str) -> ExportFile {
    let (ext, contents) = match format {
        "m3u" => ("m3u", export::to_m3u(songs)),
        "json" => ("json", export::to_json(name, songs)),
        _ => ("csv", export::to_csv(songs)),
    };
    ExportFile { file_name: export::file_name(name, ext), contents }
}

/// A playlist as a file: "csv" (Exportify columns), "m3u" or "json".
pub fn playlist_export(id: i64, format: String) -> Result<ExportFile, String> {
    let st = eng()?.store.lock().unwrap();
    let name = st.playlists().map_err(e)?.into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_else(|| "playlist".into());
    let songs = st.playlist_songs(id).map_err(e)?;
    Ok(export_songs(&name, &songs, &format))
}

/// Liked songs as a file.
pub fn liked_export(format: String) -> Result<ExportFile, String> {
    let songs = eng()?.store.lock().unwrap().liked_songs().map_err(e)?;
    Ok(export_songs("Liked songs", &songs, &format))
}

// --- Spotify account (like Exportify) ---

#[derive(Clone, Debug)]
pub struct SpotifyPlaylistView {
    pub id: String,
    pub name: String,
    pub total: u32,
    pub owner: String,
}

/// The address to add to your Spotify app's Redirect URIs.
pub fn spotify_redirect_uri() -> String {
    spotify::redirect_uri()
}

pub fn spotify_client_id() -> Result<Option<String>, String> {
    eng()?.store.lock().unwrap().kv_get("spotify:client_id").map_err(e)
}

pub fn spotify_connected() -> bool {
    eng().ok().and_then(|en| en.store.lock().unwrap().kv_get("spotify:tokens").ok().flatten()).is_some()
}

/// Starts signing in: returns the address to open in the browser.
pub fn spotify_connect(client_id: String) -> Result<String, String> {
    let client_id = client_id.trim().to_string();
    if client_id.len() < 16 || !client_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("That doesn't look like a Spotify Client ID (32 letters and digits from your app's dashboard).".into());
    }
    // A previous unfinished sign-in holds the port: drop it first.
    if let Some(c) = SPOTIFY_CANCEL.lock().unwrap().take() {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    *SPOTIFY_PENDING.lock().unwrap() = None;
    std::thread::sleep(std::time::Duration::from_millis(250));
    let callback = spotify::Callback::listen().map_err(e)?;
    let verifier = spotify::new_verifier();
    let state = spotify::new_state();
    let url = spotify::authorize_url(&client_id, &spotify::challenge(&verifier), &state);
    *SPOTIFY_CANCEL.lock().unwrap() = Some(callback.canceller());
    *SPOTIFY_PENDING.lock().unwrap() = Some(PendingSignIn { client_id, verifier, state, callback });
    Ok(url)
}

/// Waits (up to 5 minutes) for the browser sign-in to finish, then saves the tokens.
pub fn spotify_wait() -> Result<(), String> {
    let p = SPOTIFY_PENDING.lock().unwrap().take().ok_or("Start the Spotify sign-in first.")?;
    let code = p.callback.wait_for_code(&p.state, std::time::Duration::from_secs(300)).map_err(e)?;
    let tokens = spotify::Api::default().exchange(&p.client_id, &code, &p.verifier).map_err(e)?;
    let st = eng()?.store.lock().unwrap();
    st.kv_set("spotify:client_id", &p.client_id).map_err(e)?;
    st.kv_set("spotify:tokens", &tokens.to_json()).map_err(e)?;
    Ok(())
}

pub fn spotify_cancel() {
    if let Some(c) = SPOTIFY_CANCEL.lock().unwrap().take() {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn spotify_disconnect() -> Result<(), String> {
    let st = eng()?.store.lock().unwrap();
    st.kv_set("spotify:tokens", "").map_err(e)?;
    Ok(())
}

/// A valid access token, refreshed if it is about to expire.
fn spotify_token() -> Result<String, String> {
    let en = eng()?;
    let (raw, cid) = {
        let st = en.store.lock().unwrap();
        (st.kv_get("spotify:tokens").map_err(e)?.unwrap_or_default(), st.kv_get("spotify:client_id").map_err(e)?.unwrap_or_default())
    };
    let t = spotify::Tokens::from_json(&raw).ok_or("Not connected to Spotify.")?;
    if t.is_fresh(now_secs()) {
        return Ok(t.access);
    }
    let refresh = t.refresh.ok_or("Spotify sign-in expired: connect again.")?;
    let fresh = spotify::Api::default().refresh(&cid, &refresh).map_err(e)?;
    en.store.lock().unwrap().kv_set("spotify:tokens", &fresh.to_json()).map_err(e)?;
    Ok(fresh.access)
}

/// Your Spotify playlists (Liked Songs first), with how many songs each has.
pub fn spotify_playlists() -> Result<Vec<SpotifyPlaylistView>, String> {
    let token = spotify_token()?;
    let v = spotify::Api::default().playlists(&token).map_err(e)?;
    Ok(v.into_iter().map(|p| SpotifyPlaylistView { id: p.id, name: p.name, total: p.total, owner: p.owner }).collect())
}

/// Every song of one of your Spotify playlists ("liked" for Liked Songs), ready to match.
pub fn spotify_fetch(id: String, name: String) -> Result<ImportPlaylistView, String> {
    let token = spotify_token()?;
    let p = spotify::Api::default().tracks(&token, &id, &name).map_err(e)?;
    Ok(import_view(p))
}
