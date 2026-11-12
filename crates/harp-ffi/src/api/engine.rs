use crate::home_cache::CachedCollection;
use crate::home_cache::{now_secs, HomeCache, HOME_TTL_SECS};
use crate::spotify_state::{PendingSignIn, SPOTIFY_CANCEL, SPOTIFY_PENDING};
use harp_core::import as imp;
use harp_core::queue::{Queue, Repeat};
use harp_core::recommend::{self, Request};
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

/// "Playing from": kind ("playlist", "liked", "downloads", "album", "artist",
/// "podcast", "local", "mood", "smart", "search", "radio", "party"), an id, a title.
#[derive(Clone, Debug)]
pub struct SourceView {
    pub kind: String,
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct QueueView {
    pub current: Option<SongView>,
    /// Songs you queued yourself.
    pub user: Vec<SongView>,
    /// The rest of the playlist / album / list being played.
    pub context: Vec<SongView>,
    pub autoplay: Vec<SongView>,
    /// Where it's playing from (None for a single song).
    pub source: Option<SourceView>,
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
    db_path: PathBuf,
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

pub(crate) fn view(s: &Song, liked: bool, downloaded: bool) -> SongView {
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

pub(crate) fn song(v: &SongView) -> Song {
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
        // Your own files are on the device already: they count as downloaded (they play offline).
        let dl = harp_core::local::is_local(&s.id) || st.downloaded_path(&s.id).ok().flatten().is_some();
        view(&s, liked, dl)
    })
    .collect()
}

fn queue_view(en: &Engine) -> QueueView {
    let q = en.queue.lock().unwrap();
    let cur: Vec<Song> = q.current().cloned().into_iter().collect();
    let user: Vec<Song> = q.user_queue().cloned().collect();
    let context: Vec<Song> = q.context().cloned().collect();
    let auto: Vec<Song> = q.autoplay().cloned().collect();
    let source = q.source.clone().map(|s| SourceView { kind: s.kind, id: s.id, title: s.title });
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
        context: views(en, context.into_iter()),
        autoplay: views(en, auto.into_iter()),
        source,
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

    let mut tool = tool;
    tool.cache_dir = Some(data.join("ytdlp-cache"));
    // A backup an older build left staged is merged in (never swapped over this library).
    let _ = harp_core::backup::apply_pending(&db);
    let store = Store::open(&db).map_err(e)?;
    // The queue as it was when the app last closed.
    let mut queue: Queue = store.kv_get("queue").ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    queue.upgrade();
    let engine = Engine {
        db_path: db.clone(),
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
    if harp_core::local::is_local(&id) {
        return match en.store.lock().unwrap().local_path(&id).map_err(e)? {
            Some(p) if std::path::Path::new(&p).is_file() => Ok(StreamView { url: p, local: true, headers: vec![], expires_at: None }),
            _ => Err("this file is no longer on the device (rescan your folders in Library › On This Device)".into()),
        };
    }
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
    let s = en.yt.resolve(&id, Quality::parse(&quality)).map_err(|err| {
        let msg = err.to_string();
        note_bot_check(&msg);
        msg
    })?;
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
        match section.as_str() {
            "auto" => q.jump_auto(index as usize),
            "context" => q.jump_context(index as usize),
            _ => q.jump_user(index as usize),
        };
    }
    Ok(queue_view(en))
}

/// Replaces the autoplay section with these songs (used for offline mode: your downloads).
pub fn queue_set_autoplay(songs: Vec<SongView>) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().set_autoplay(songs.iter().map(song).collect());
    Ok(queue_view(en))
}

fn section_of(s: &str) -> Result<harp_core::queue::Section, String> {
    harp_core::queue::Section::parse(s).ok_or_else(|| format!("no queue section {s}"))
}

/// Removes an upcoming song (`section`: "user", "context" or "auto").
pub fn queue_remove(section: String, index: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().remove_in(section_of(&section)?, index as usize);
    Ok(queue_view(en))
}

/// Moves an upcoming song within its section.
pub fn queue_move(section: String, from: u32, to: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().move_in(section_of(&section)?, from as usize, to as usize);
    Ok(queue_view(en))
}

/// "Play next" for an upcoming song; the others keep their order.
pub fn queue_promote(section: String, index: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().promote(section_of(&section)?, index as usize);
    Ok(queue_view(en))
}

/// Marks where the music plays from (for a single song started by a mood, Smart Shuffle…).
pub fn queue_set_source(kind: String, id: String, title: String) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().source = Some(harp_core::queue::Source { kind, id, title });
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
    Request { requested: genre.is_some() || artist.is_some(), genre, artist, exclude: queued, now: vec![] }
}

/// What you're listening to right now, newest first: the current song, then
/// what you played in the last 40 minutes.
fn listening_now(en: &Engine) -> Vec<Song> {
    let mut now: Vec<Song> = en.queue.lock().unwrap().current().cloned().into_iter().collect();
    let since = now_secs() as i64 - 40 * 60;
    for s in en.store.lock().unwrap().played_since(since, 6).unwrap_or_default() {
        if !now.iter().any(|x| x.id == s.id) {
            now.push(s);
        }
    }
    now
}

/// The next recommended song, with the reason it was picked.
pub fn recommend_next(genre: Option<String>, artist: Option<String>) -> Result<Option<Pick>, String> {
    let en = eng()?;
    let exclude = {
        let q = en.queue.lock().unwrap();
        q.current().into_iter().chain(q.user_queue()).chain(q.autoplay()).map(|s| s.id.clone()).collect()
    };
    let mut req = request(genre, artist, exclude);
    req.now = listening_now(en);
    let st = en.rec_store.lock().unwrap();
    let mut rng = en.rng.lock().unwrap().clone();
    let r = recommend::next(&st, &en.yt, &req, &mut rng).map_err(e)?;
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
        q.current().into_iter().chain(q.user_queue()).chain(q.context()).map(|s| s.id.clone()).collect()
    };
    let picks = {
        let st = en.rec_store.lock().unwrap();
        let mut rng = en.rng.lock().unwrap().clone();
        let mut req = request(genre, None, exclude);
        req.now = listening_now(en);
        recommend::batch(&st, &en.yt, &req, count as usize, &mut rng).map_err(e)?
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
    /// Songs added later get downloaded ("Download All" was chosen).
    pub auto_download: bool,
    /// Your own cover picture (a file), or "".
    pub cover: String,
    /// The folder it's in, or "".
    pub folder: String,
    /// The shared link it follows (a friend's playlist), or "".
    pub remote: String,
    pub pinned: bool,
    /// Artwork of the first songs, for a cover mosaic.
    pub covers: Vec<String>,
}

pub fn playlists() -> Result<Vec<PlaylistView>, String> {
    let v = eng()?.store.lock().unwrap().playlists().map_err(e)?;
    Ok(v.into_iter().map(|p| PlaylistView { id: p.id, name: p.name, count: p.count, auto_download: p.auto_download, cover: p.cover, folder: p.folder, remote: p.remote, pinned: p.pinned, covers: p.covers }).collect())
}

/// Pins a playlist to the top of the list (or unpins it).
pub fn playlist_auto_download(id: i64, on: bool) -> Result<(), String> {
    eng()?.store.lock().unwrap().set_playlist_auto_download(id, on).map_err(e)
}

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
/// Plays a saved playlist as the context ("Playing from" its name). Songs you
/// queued yourself stay ahead of it.
pub fn queue_play_playlist(id: i64, shuffle: bool) -> Result<QueueView, String> {
    let en = eng()?;
    let (mut songs, name) = {
        let st = en.store.lock().unwrap();
        let name = st.playlists().map_err(e)?.into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_default();
        (st.playlist_songs(id).map_err(e)?, name)
    };
    if shuffle {
        en.rng.lock().unwrap().shuffle(&mut songs);
    }
    let source = harp_core::queue::Source { kind: "playlist".into(), id: id.to_string(), title: name };
    en.queue.lock().unwrap().play_context(songs, Some(source));
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
    // Already fetched ahead into the play cache: move that file over instead of
    // asking YouTube for the song again.
    let cached = cache::find(&en.cache_dir, &s.id).and_then(|c| {
        std::fs::create_dir_all(&en.downloads_dir).ok()?;
        let dest = en.downloads_dir.join(c.file_name()?);
        if std::fs::rename(&c, &dest).is_err() {
            std::fs::copy(&c, &dest).ok()?;
            let _ = std::fs::remove_file(&c);
        }
        let bytes = std::fs::metadata(&dest).ok()?.len();
        Some((dest, bytes))
    });
    let (path, bytes) = match cached {
        Some(x) => x,
        None => en.yt.download(&s.id, &en.downloads_dir, Quality::parse(&quality)).map_err(|err| {
            let msg = err.to_string();
            note_bot_check(&msg);
            msg
        })?,
    };
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
    /// Artists you play most.
    pub artists: Vec<CollectionView>,
    /// Albums by them.
    pub albums: Vec<CollectionView>,
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
                Some(s) => recommend::related_to(&en.yt, s, 12).unwrap_or_default(),
                None => vec![],
            };
            Ok(HomeCache { ts: now_secs(), recommended: recs.into_iter().map(|r| r.song).collect(), similar_to: last, similar, ..Default::default() })
        })();
        if let Ok(mut c) = attempt {
            if !c.recommended.is_empty() {
                c.artists = std::mem::take(&mut cache.artists);
                c.albums = std::mem::take(&mut cache.albums);
                cache = c;
                fresh = true;
            }
        }
    }
    // Artists and albums: with new picks, or once for a cache saved before they existed.
    if !offline && (fresh || cache.artists.is_empty()) {
        let (artists, albums) = home_artists(en);
        if !artists.is_empty() {
            cache.artists = artists;
            cache.albums = albums;
            if cache.ts == 0 {
                cache.ts = now_secs();
            }
        }
    }
    if fresh || !cache.artists.is_empty() {
        let _ = en.store.lock().unwrap().kv_set("home_cache", &serde_json::to_string(&cache).unwrap_or_default());
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
        artists: cache.artists.into_iter().map(cached_view).collect(),
        albums: cache.albums.into_iter().map(cached_view).collect(),
        fresh,
    })
}

fn cached_view(c: CachedCollection) -> CollectionView {
    CollectionView { id: c.id, title: c.title, subtitle: c.subtitle, thumbnail: c.thumbnail, kind: c.kind }
}

/// The artists you play most (looked up on YouTube Music for their pictures)
/// and a couple of albums by each of the top few. Lookups run side by side.
fn home_artists(en: &Engine) -> (Vec<CachedCollection>, Vec<CachedCollection>) {
    let names = {
        let st = en.store.lock().unwrap();
        let history = st.history(150).unwrap_or_default();
        let liked = st.liked_songs().unwrap_or_default();
        recommend::top_artists(&history, &liked, 8)
    };
    if names.is_empty() {
        return (vec![], vec![]);
    }
    let keep = |c: ytmusic::Collection| CachedCollection { id: c.id, title: c.title, subtitle: c.subtitle, thumbnail: c.thumbnail.unwrap_or_default(), kind: c.kind };
    std::thread::scope(|sc| {
        let artist_jobs: Vec<_> = names
            .iter()
            .map(|n| {
                sc.spawn(move || {
                    let hits = ytmusic::search_collections(ytmusic::Kind::Artists, n, 3).ok()?;
                    let exact = hits.iter().position(|h| h.title.eq_ignore_ascii_case(n)).unwrap_or(0);
                    hits.into_iter().nth(exact)
                })
            })
            .collect();
        let album_jobs: Vec<_> = names
            .iter()
            .take(4)
            .map(|n| {
                sc.spawn(move || {
                    let low = n.to_lowercase();
                    ytmusic::search_collections(ytmusic::Kind::Albums, n, 6)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|a| a.subtitle.to_lowercase().contains(&low))
                        .take(2)
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        let artists = artist_jobs.into_iter().filter_map(|j| j.join().ok().flatten()).filter(|a| seen.insert(a.id.clone())).map(keep).collect();
        let albums = album_jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).filter(|a| seen.insert(a.id.clone())).map(keep).collect();
        (artists, albums)
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
        let mut name = "Imported playlist".to_string();
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
    Err("Paste a public playlist link (Spotify, Apple Music and others).".into())
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
/// Plays a list (album, liked songs, downloads, your files…) as the context.
/// `source_kind` / `source_id` / `source_title` say where it's from ("" kind: none).
pub fn queue_play_list(songs: Vec<SongView>, shuffle: bool, source_kind: String, source_id: String, source_title: String) -> Result<QueueView, String> {
    let en = eng()?;
    let mut list: Vec<Song> = songs.iter().map(song).collect();
    if shuffle {
        en.rng.lock().unwrap().shuffle(&mut list);
    }
    let source = (!source_kind.is_empty()).then(|| harp_core::queue::Source { kind: source_kind, id: source_id, title: source_title });
    en.queue.lock().unwrap().play_context(list, source);
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
    match lrc::fetch(&s.id, &s.title, &s.artist, s.album.as_deref(), s.duration_secs) {
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
    // YouTube asked us to slow down: no background fetching for a while.
    if limit_mb == 0 || backing_off() {
        return;
    }
    let inflight = PREFETCHING.get_or_init(|| Mutex::new(Default::default()));
    let all = ids.clone();
    for id in ids {
        if harp_core::local::is_local(&id) {
            continue; // already on the device
        }
        if en.store.lock().unwrap().downloaded_path(&id).ok().flatten().is_some() || cache::find(&en.cache_dir, &id).is_some() {
            continue;
        }
        if !inflight.lock().unwrap().insert(id.clone()) {
            continue;
        }
        let q = Quality::parse(&quality);
        let keep = all.clone();
        std::thread::spawn(move || {
            let Ok(en) = eng() else { return };
            {
                let _turn = PREFETCH_TURN.lock().unwrap();
                if cache::find(&en.cache_dir, &id).is_none() && !backing_off() {
                    if let Err(e) = en.yt.download(&id, &en.cache_dir, q) {
                        note_bot_check(&e.to_string());
                    }
                    // One at a time, a few seconds apart: a burst of requests is
                    // what YouTube answers with "confirm you're not a bot".
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
                cache::trim(&en.cache_dir, limit_mb as u64 * 1_048_576, &keep);
            }
            PREFETCHING.get().unwrap().lock().unwrap().remove(&id);
        });
    }
}

/// Keeps only these songs in the play cache (the one just played, the one
/// playing and the ones coming up); the rest are deleted. Downloads live
/// elsewhere and are never touched.
pub fn cache_keep_only(keep: Vec<String>) {
    if let Ok(en) = eng() {
        cache::retain(&en.cache_dir, &keep);
    }
}

/// Whether a song plays from the device (downloaded, your own file, or fetched ahead).
#[flutter_rust_bridge::frb(sync)]
pub fn song_on_device(id: String) -> bool {
    let Ok(en) = eng() else { return false };
    if harp_core::local::is_local(&id) {
        return true;
    }
    if en.store.lock().unwrap().downloaded_path(&id).ok().flatten().is_some_and(|p| std::path::Path::new(&p).is_file()) {
        return true;
    }
    cache::find(&en.cache_dir, &id).is_some()
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

// --- backup and restore ---


/// The whole library (likes, playlists, history, taste, settings) as one file's bytes.
pub fn library_backup() -> Result<Vec<u8>, String> {
    let en = eng()?;
    std::fs::create_dir_all(&en.cache_dir).map_err(e)?;
    let tmp = en.cache_dir.join(format!("backup-{}.harp", now_secs()));
    let _ = std::fs::remove_file(&tmp);
    en.store.lock().unwrap().backup_to(&tmp).map_err(e)?;
    let bytes = std::fs::read(&tmp).map_err(e);
    let _ = std::fs::remove_file(&tmp);
    bytes
}

/// What a restore added to your library.
#[derive(Clone, Debug)]
pub struct MergeInfoView {
    pub liked: u32,
    pub playlists_new: u32,
    pub playlists_grown: u32,
    pub plays: u32,
}

/// Adds a backup to your library (nothing already here is replaced or lost).
pub fn library_restore(bytes: Vec<u8>) -> Result<MergeInfoView, String> {
    let en = eng()?;
    std::fs::create_dir_all(&en.cache_dir).map_err(e)?;
    let tmp = en.cache_dir.join("restore.harp");
    std::fs::write(&tmp, &bytes).map_err(e)?;
    let r = en.store.lock().unwrap().merge_from(&tmp);
    let _ = std::fs::remove_file(&tmp);
    let i = r.map_err(e)?;
    Ok(MergeInfoView { liked: i.liked, playlists_new: i.playlists_new, playlists_grown: i.playlists_grown, plays: i.plays })
}

// --- followed artists, saved albums, new releases ---

/// Follow an artist, or save an album, playlist or podcast to the library (`on`), or undo it.
pub fn collection_save(c: CollectionView, on: bool) -> Result<(), String> {
    let item = harp_core::library::Saved { id: c.id, kind: c.kind, title: c.title, subtitle: c.subtitle, thumbnail: c.thumbnail };
    let en = eng()?;
    en.store.lock().unwrap().set_saved(&item, on).map_err(e)?;
    if item.kind == "artist" {
        // The new-releases shelf changes with who you follow.
        let _ = en.store.lock().unwrap().kv_set("releases", "");
    }
    Ok(())
}

#[flutter_rust_bridge::frb(sync)]
pub fn collection_is_saved(id: String) -> bool {
    eng().ok().and_then(|en| en.store.lock().unwrap().is_saved(&id).ok()).unwrap_or(false)
}

/// Followed artists (`kind` "artist") or saved albums, playlists and podcasts (`kind` "other").
pub fn saved_collections(kind: String) -> Result<Vec<CollectionView>, String> {
    let kinds: &[&str] = if kind == "artist" { &["artist"] } else { &["album", "playlist", "podcast"] };
    let v = eng()?.store.lock().unwrap().saved(kinds).map_err(e)?;
    Ok(v.into_iter().map(|s| CollectionView { id: s.id, title: s.title, subtitle: s.subtitle, thumbnail: s.thumbnail, kind: s.kind }).collect())
}

/// New albums and singles from the artists you follow (this year and last),
/// newest first. Kept for 12 hours; `offline` uses only the saved copy.
pub fn new_releases(refresh: bool, offline: bool) -> Result<Vec<CollectionView>, String> {
    let en = eng()?;
    let cached: Option<(u64, Vec<CachedCollection>)> =
        en.store.lock().unwrap().kv_get("releases").ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
    let fresh_enough = cached.as_ref().is_some_and(|(ts, _)| now_secs().saturating_sub(*ts) < 12 * 3600);
    if offline || (fresh_enough && !refresh) {
        return Ok(cached.map(|(_, v)| v.into_iter().map(cached_view).collect()).unwrap_or_default());
    }
    let artists = en.store.lock().unwrap().saved(&["artist"]).map_err(e)?;
    if artists.is_empty() {
        return Ok(vec![]);
    }
    let year = this_year();
    let per_artist: Vec<Vec<CachedCollection>> = std::thread::scope(|sc| {
        let jobs: Vec<_> = artists
            .iter()
            .take(16)
            .map(|a| {
                sc.spawn(move || {
                    let Ok((_, _, related)) = ytmusic::open_artist(&a.id) else { return vec![] };
                    ytmusic::latest_releases(&related, year, 3)
                        .into_iter()
                        .map(|c| CachedCollection {
                            id: c.id,
                            title: c.title,
                            subtitle: format!("{} · {}", a.title, c.subtitle.replace(" • ", " · ")),
                            thumbnail: c.thumbnail.unwrap_or_default(),
                            kind: c.kind,
                        })
                        .collect()
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect()
    });
    // Newest first across artists; round-robin within a year so one artist doesn't fill the shelf.
    let mut all: Vec<(u32, usize, CachedCollection)> = Vec::new();
    for list in per_artist {
        for (rank, c) in list.into_iter().enumerate() {
            let y = ytmusic::release_year(&c.subtitle).unwrap_or(0);
            all.push((y, rank, c));
        }
    }
    all.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut seen = std::collections::HashSet::new();
    let out: Vec<CachedCollection> = all.into_iter().map(|(_, _, c)| c).filter(|c| seen.insert(c.id.clone())).take(20).collect();
    let _ = en.store.lock().unwrap().kv_set("releases", &serde_json::to_string(&(now_secs(), &out)).unwrap_or_default());
    Ok(out.into_iter().map(cached_view).collect())
}

fn this_year() -> u32 {
    // Days since 1970 to a year is close enough for "this year or last".
    (1970 + now_secs() / 31_556_952) as u32
}

// --- browse: moods, genres, charts ---

#[derive(Clone, Debug)]
pub struct BrowseTileView {
    pub title: String,
    pub params: String,
    /// ARGB
    pub color: u32,
}

#[derive(Clone, Debug)]
pub struct BrowseGroupView {
    pub title: String,
    pub tiles: Vec<BrowseTileView>,
}

#[derive(Clone, Debug)]
pub struct SectionView {
    pub title: String,
    pub items: Vec<CollectionView>,
}

fn sections(v: Vec<ytmusic::Section>) -> Vec<SectionView> {
    v.into_iter().map(|s| SectionView { title: s.title, items: s.items.into_iter().map(collection_view).collect() }).collect()
}

/// Moods and genres to browse (`country`: ISO code like "IN").
pub fn browse_explore(country: String) -> Result<Vec<BrowseGroupView>, String> {
    let v = ytmusic::explore(&country).map_err(e)?;
    Ok(v.into_iter()
        .map(|(title, tiles)| BrowseGroupView {
            title,
            tiles: tiles.into_iter().map(|t| BrowseTileView { title: t.title, params: t.params, color: t.color }).collect(),
        })
        .collect())
}

/// One mood or genre: rows of playlists.
pub fn browse_category(params: String, country: String) -> Result<Vec<SectionView>, String> {
    Ok(sections(ytmusic::category(&params, &country).map_err(e)?))
}

/// The charts for a country.
pub fn browse_charts(country: String) -> Result<Vec<SectionView>, String> {
    Ok(sections(ytmusic::charts(&country).map_err(e)?))
}

/// Looks lyrics up again under a title and artist you typed (for songs whose
/// names are messy), and keeps what it finds for this song.
pub fn lyrics_find(s: SongView, title: String, artist: String) -> Result<Option<LyricsView>, String> {
    let en = eng()?;
    let found = lrc::fetch(&s.id, &title, &artist, None, s.duration_secs).map_err(e)?;
    Ok(found.map(|l| {
        let _ = en.store.lock().unwrap().save_lyrics(&s.id, &l);
        lyrics_view(l)
    }))
}

// --- your own music files ---

/// The folders Harp reads your music files from.
pub fn local_folders() -> Result<Vec<String>, String> {
    eng()?.store.lock().unwrap().local_folders().map_err(e)
}

/// Adds (`on`) or removes a folder, then rescans. Returns how many songs there are.
pub fn local_folder_set(path: String, on: bool) -> Result<u32, String> {
    let en = eng()?;
    {
        let st = en.store.lock().unwrap();
        let mut dirs = st.local_folders().map_err(e)?;
        dirs.retain(|d| d != &path);
        if on {
            dirs.push(path);
        }
        st.set_local_folders(&dirs).map_err(e)?;
    }
    local_rescan()
}

/// Reads the folders again (new, changed and removed files).
pub fn local_rescan() -> Result<u32, String> {
    let en = eng()?;
    let covers = en.thumbs_dir.join("local");
    en.store.lock().unwrap().local_rescan(Some(&covers)).map_err(e)
}

pub fn local_songs() -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let v = en.store.lock().unwrap().local_songs().map_err(e)?;
    Ok(views(en, v.into_iter()))
}

/// Smart Shuffle without a connection: your downloads and your own files,
/// ranked by your taste. `exclude`: ids already queued.
pub fn recommend_offline(count: u32, exclude: Vec<String>) -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let pool: Vec<Song> = {
        let st = en.store.lock().unwrap();
        let mut p: Vec<Song> = st.downloads().map_err(e)?.into_iter().filter(|d| std::path::Path::new(&d.path).is_file()).map(|d| d.song).collect();
        p.extend(st.local_songs().map_err(e)?);
        p
    };
    let picks = {
        let st = en.store.lock().unwrap();
        let mut rng = en.rng.lock().unwrap();
        recommend::rank_pool(&st, pool, count as usize, &exclude, &mut rng).map_err(e)?
    };
    Ok(views(en, picks.into_iter()))
}


// --- playlists: membership, smart shuffle ---

/// Name and songs of a playlist (for sharing).
pub(crate) fn playlist_contents(id: i64) -> Result<(String, Vec<Song>), String> {
    let en = eng()?;
    let st = en.store.lock().unwrap();
    let name = st.playlists().map_err(e)?.into_iter().find(|p| p.id == id).map(|p| p.name).ok_or("no such playlist")?;
    Ok((name, st.playlist_songs(id).map_err(e)?))
}

/// The playlists a song is in.
pub fn playlists_containing(song_id: String) -> Result<Vec<i64>, String> {
    eng()?.store.lock().unwrap().playlists_with_song(&song_id).map_err(e)
}

/// Every song that's in some playlist (rows show a ✓).
pub fn playlisted_song_ids() -> Result<Vec<String>, String> {
    eng()?.store.lock().unwrap().playlisted_ids().map_err(e)
}

/// Takes a song out of a playlist.
pub fn playlist_remove_song(id: i64, song_id: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().remove_song_from_playlist(id, &song_id).map_err(e)
}

/// Songs like the ones in a playlist but not in it: for smart shuffle and the
/// "recommended for this playlist" list. Uses a few of its songs as seeds.
pub fn playlist_smart_picks(id: i64, count: u32) -> Result<Vec<SongView>, String> {
    let en = eng()?;
    let songs = en.store.lock().unwrap().playlist_songs(id).map_err(e)?;
    if songs.is_empty() {
        return Ok(vec![]);
    }
    let have: std::collections::HashSet<String> = songs.iter().map(|s| s.id.clone()).collect();
    let seeds: Vec<Song> = {
        let mut rng = en.rng.lock().unwrap();
        let mut v = songs.clone();
        rng.shuffle(&mut v);
        v.into_iter().take(4).collect()
    };
    // Related songs for each seed, side by side.
    let lists: Vec<Vec<Song>> = std::thread::scope(|sc| {
        let jobs: Vec<_> = seeds.iter().map(|s| sc.spawn(|| recommend::related_to(&en.yt, s, 12).unwrap_or_default())).collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect()
    });
    // Round-robin across seeds so picks are varied.
    let mut out: Vec<Song> = Vec::new();
    let mut seen = have.clone();
    for i in 0..12 {
        for l in &lists {
            if let Some(s) = l.get(i) {
                if seen.insert(s.id.clone()) {
                    out.push(s.clone());
                }
            }
        }
        if out.len() >= count as usize {
            break;
        }
    }
    out.truncate(count as usize);
    Ok(views(en, out.into_iter()))
}

/// Smart shuffle: mixes picks into the list playing now, one after every `every` songs.
pub fn queue_mix_in(songs: Vec<SongView>, every: u32) -> Result<QueueView, String> {
    let en = eng()?;
    en.queue.lock().unwrap().mix_into_context(songs.iter().map(song).collect(), every as usize);
    Ok(queue_view(en))
}

// --- headphone correction (AutoEq) ---

#[derive(Clone, Debug)]
pub struct AutoEqProfileView {
    pub name: String,
    pub source: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct EqFilterView {
    /// "peak", "low_shelf" or "high_shelf"
    pub kind: String,
    pub freq: f64,
    pub gain_db: f64,
    pub q: f64,
}

#[derive(Clone, Debug)]
pub struct AutoEqView {
    pub name: String,
    pub preamp_db: f64,
    pub filters: Vec<EqFilterView>,
}

/// Headphones and earbuds matching `query` (AutoEq has ~9,000 measured models).
pub fn autoeq_search(query: String) -> Result<Vec<AutoEqProfileView>, String> {
    let en = eng()?;
    let all = harp_ytdlp::autoeq::index(&en.cache_dir).map_err(e)?;
    Ok(harp_ytdlp::autoeq::search(&all, &query, 40).into_iter().map(|p| AutoEqProfileView { name: p.name, source: p.source, path: p.path }).collect())
}

/// One model's correction filters.
pub fn autoeq_load(name: String, source: String, path: String) -> Result<AutoEqView, String> {
    use harp_ytdlp::autoeq::{Kind, Profile};
    let c = harp_ytdlp::autoeq::load(&Profile { name: name.clone(), source, path }).map_err(e)?;
    Ok(AutoEqView {
        name,
        preamp_db: c.preamp_db,
        filters: c
            .filters
            .into_iter()
            .map(|f| EqFilterView {
                kind: match f.kind {
                    Kind::Peak => "peak",
                    Kind::LowShelf => "low_shelf",
                    Kind::HighShelf => "high_shelf",
                }
                .into(),
                freq: f.freq,
                gain_db: f.gain_db,
                q: f.q,
            })
            .collect(),
    })
}


/// Sets a playlist's own cover: copies the picture into Harp's data, so it stays
/// even if the original moves ("" goes back to the mosaic).
pub fn playlist_set_cover(id: i64, image_path: String) -> Result<String, String> {
    let en = eng()?;
    let stored = if image_path.is_empty() {
        String::new()
    } else {
        let dir = en.thumbs_dir.join("playlists");
        std::fs::create_dir_all(&dir).map_err(e)?;
        let ext = std::path::Path::new(&image_path).extension().and_then(|x| x.to_str()).unwrap_or("jpg").to_lowercase();
        let dest = dir.join(format!("pl{id}-{}.{ext}", now_secs()));
        std::fs::copy(&image_path, &dest).map_err(e)?;
        dest.to_string_lossy().into_owned()
    };
    en.store.lock().unwrap().set_playlist_cover(id, &stored).map_err(e)?;
    Ok(stored)
}

/// Puts a playlist in a folder ("" takes it out).
pub fn playlist_set_folder(id: i64, folder: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().set_playlist_folder(id, &folder).map_err(e)
}

pub fn folder_rename(old: String, new: String) -> Result<(), String> {
    eng()?.store.lock().unwrap().rename_folder(&old, &new).map_err(e)
}


/// A followed playlist's songs and link (true when its songs changed).
pub(crate) fn playlist_follow_set(id: i64, link: &str, songs: &[Song]) -> Result<bool, String> {
    let st = eng()?.store.lock().unwrap();
    st.set_playlist_remote(id, link).map_err(e)?;
    st.replace_playlist_songs(id, songs).map_err(e)
}

/// Playlists that follow a friend's shared link: (id, link).
pub(crate) fn followed_playlists() -> Result<Vec<(i64, String)>, String> {
    Ok(eng()?.store.lock().unwrap().playlists().map_err(e)?.into_iter().filter(|p| !p.remote.is_empty()).map(|p| (p.id, p.remote)).collect())
}


// --- YouTube's "confirm you're not a bot" ---

static BOT_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// After YouTube says "Sign in to confirm you're not a bot", Harp stops all
/// background fetching (prefetch, lining up the next song) for two minutes;
/// songs you ask for still try. Asking less often is what makes it go away.
fn note_bot_check(msg: &str) {
    let m = msg.to_lowercase();
    if m.contains("not a bot") || m.contains("confirm you") || m.contains("http error 429") || m.contains("too many requests") {
        BOT_UNTIL.store(now_secs() + 120, std::sync::atomic::Ordering::Relaxed);
    }
}

fn backing_off() -> bool {
    now_secs() < BOT_UNTIL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether background fetching is paused because YouTube asked us to slow down.
#[flutter_rust_bridge::frb(sync)]
pub fn youtube_backing_off() -> bool {
    backing_off()
}

// --- Your Rhythm and Harp Encore: listening stats ---

pub struct ArtistStatView {
    pub name: String,
    pub minutes: f64,
    pub plays: u32,
    pub thumbnail: Option<String>,
}

pub struct SongStatView {
    pub song: SongView,
    pub plays: u32,
    pub minutes: f64,
}

pub struct DayMinutes {
    pub day: String,
    pub minutes: f64,
}

pub struct StatsView {
    pub minutes: f64,
    pub plays: u32,
    pub songs: u32,
    pub artists: u32,
    pub top_artists: Vec<ArtistStatView>,
    pub top_songs: Vec<SongStatView>,
    pub by_hour: Vec<f64>,
    pub by_day: Vec<DayMinutes>,
    pub streak: u32,
    pub peak_hour: Option<u32>,
    pub first: Option<SongView>,
}

/// Listening between `from` and `to` (unix seconds), `top` entries per list.
pub fn listening_stats(from: i64, to: i64, top: u32) -> Result<StatsView, String> {
    let en = eng()?;
    let s = en.store.lock().unwrap().listening_stats(from, to, top as usize).map_err(e)?;
    let peak_hour = s.peak_hour();
    let songs: Vec<Song> = s.top_songs.iter().map(|x| x.song.clone()).collect();
    let song_views = views(en, songs.into_iter());
    let first = s.first.clone().map(|f| views(en, std::iter::once(f)).remove(0));
    Ok(StatsView {
        minutes: s.minutes,
        plays: s.plays,
        songs: s.songs,
        artists: s.artists,
        top_artists: s.top_artists.into_iter().map(|a| ArtistStatView { name: a.name, minutes: a.minutes, plays: a.plays, thumbnail: a.thumbnail }).collect(),
        top_songs: s.top_songs.iter().zip(song_views).map(|(x, v)| SongStatView { song: v, plays: x.plays, minutes: x.minutes }).collect(),
        by_hour: s.by_hour,
        by_day: s.by_day.into_iter().map(|(day, minutes)| DayMinutes { day, minutes }).collect(),
        streak: s.streak,
        peak_hour,
        first,
    })
}
