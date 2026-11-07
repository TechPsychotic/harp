use harp_core::queue::{Queue, Repeat};
use harp_core::recommend::{self, Request};
use harp_core::rng::Rng;
use harp_core::taste::{PlayRecord, PlaySource, SkipReason, Store};
use harp_core::Song;
use harp_ytdlp::{install, proxy::Proxy, Tool, YtDlp};
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
    pub url: String,
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
}

struct Engine {
    yt: YtDlp,
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

fn view(s: &Song, liked: bool) -> SongView {
    SongView {
        id: s.id.clone(),
        title: s.title.clone(),
        artist: s.artist.clone(),
        artist_id: s.artist_id.clone(),
        album: s.album.clone(),
        duration_secs: s.duration_secs,
        thumbnail: s.thumbnail_or_default(),
        liked,
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
        view(&s, liked)
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
    drop(q);
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
        let zip = install::ensure(&tools, install::asset_name("android", "aarch64", true), false).map_err(e)?;
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

    let engine = Engine {
        yt: YtDlp::new(tool),
        store: Mutex::new(Store::open(&db).map_err(e)?),
        rec_store: Mutex::new(Store::open(&db).map_err(e)?),
        queue: Mutex::new(Queue::new()),
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
    let found = en.yt.search_songs(&query, 20).map_err(e)?;
    Ok(views(en, found.into_iter()))
}

pub fn resolve_stream(id: String) -> Result<StreamView, String> {
    let s = eng()?.yt.resolve(&id).map_err(e)?;
    Ok(StreamView { url: s.url, headers: s.headers.into_iter().collect(), expires_at: s.expires_at })
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
    Ok(v.iter().map(|s| view(s, true)).collect())
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
    Ok(r.map(|r| {
        let liked = en.store.lock().unwrap().is_liked(&r.song.id).unwrap_or(false);
        Pick { song: view(&r.song, liked), reason: r.reason }
    }))
}

/// Fills the autoplay ("up next") section after the user's own queue.
pub fn fill_autoplay(count: u32) -> Result<QueueView, String> {
    let en = eng()?;
    let exclude: Vec<String> = {
        let q = en.queue.lock().unwrap();
        q.current().into_iter().chain(q.user_queue()).map(|s| s.id.clone()).collect()
    };
    let picks = {
        let st = en.rec_store.lock().unwrap();
        let mut rng = en.rng.lock().unwrap().clone();
        recommend::batch(&st, &en.yt, &request(None, None, exclude), count as usize, &mut rng).map_err(e)?
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
    Ok(song_from_json(&raw).map(|s| {
        let liked = en.store.lock().unwrap().is_liked(&s.id).unwrap_or(false);
        view(&s, liked)
    }))
}

fn serde_json_string(s: &Song) -> String {
    harp_core::model::to_json(s)
}

fn song_from_json(raw: &str) -> Option<Song> {
    harp_core::model::from_json(raw)
}
