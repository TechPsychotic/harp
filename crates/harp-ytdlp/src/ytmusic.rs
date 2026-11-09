//! YouTube Music's public web API (the same calls the `ytmusicapi` library
//! makes, with no sign-in and no key), in Rust so it works on every platform
//! without Python. It gives what yt-dlp's flat listings lack: artists, album,
//! length and cover art, and the same "song radio" YouTube Music plays next.
//!
//! The JSON is deeply nested and changes shape now and then, so parsing finds
//! renderers by name instead of by path.

use crate::{Error, Result};
use harp_core::Song;
use serde_json::{json, Value};
use std::time::Duration;

const BASE: &str = "https://music.youtube.com/youtubei/v1";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";
/// The "Songs" filter of YouTube Music's search page.
const SONGS_FILTER: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";

fn context() -> Value {
    json!({"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20241127.01.00", "hl": "en", "gl": "US"}})
}

fn post(endpoint: &str, mut body: Value) -> Result<Value> {
    body["context"] = context();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut resp = agent
        .post(&format!("{BASE}/{endpoint}?prettyPrint=false"))
        .header("Content-Type", "application/json")
        .header("Origin", "https://music.youtube.com")
        .header("User-Agent", UA)
        .send(body.to_string())
        .map_err(|e| Error::Download(e.to_string()))?;
    let text = resp.body_mut().read_to_string().map_err(|e| Error::Download(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()))
}

/// Songs matching a query.
pub fn search(query: &str, limit: usize) -> Result<Vec<Song>> {
    let v = post("search", json!({"query": query, "params": SONGS_FILTER}))?;
    let mut songs = parse_search(&v);
    songs.truncate(limit);
    Ok(songs)
}

/// The song radio for a video: what YouTube Music plays after it.
pub fn radio(video_id: &str, limit: usize) -> Result<Vec<Song>> {
    let v = post(
        "next",
        json!({
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        }),
    )?;
    let mut songs = parse_radio(&v);
    songs.retain(|s| s.id != video_id);
    songs.truncate(limit);
    Ok(songs)
}

/// Every value stored under `key`, anywhere in the tree.
fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == key {
                    out.push(x);
                } else {
                    find_all(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, out)),
        _ => {}
    }
}

fn text(v: &Value) -> Option<&str> {
    v.get("text").and_then(Value::as_str)
}

/// "6:10" or "1:02:03" -> seconds.
fn parse_clock(s: &str) -> Option<u32> {
    let mut total = 0u32;
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    for p in parts {
        total = total.checked_mul(60)?.checked_add(p.trim().parse::<u32>().ok()?)?;
    }
    Some(total)
}

struct Byline {
    artist: String,
    artist_id: Option<String>,
    album: Option<String>,
    secs: Option<u32>,
}

/// Parses "Artist, Artist & Artist • Album • 6:10" (runs, split on " • ").
fn byline(runs: &[Value]) -> Byline {
    let mut groups: Vec<Vec<&Value>> = vec![vec![]];
    for r in runs {
        if text(r) == Some(" • ") {
            groups.push(vec![]);
        } else {
            groups.last_mut().unwrap().push(r);
        }
    }
    let join = |g: &Vec<&Value>| g.iter().filter_map(|r| text(r)).collect::<String>();
    let artist = groups.first().map(join).unwrap_or_default();
    let artist_id = groups
        .first()
        .and_then(|g| g.iter().find_map(|r| r.pointer("/navigationEndpoint/browseEndpoint/browseId")))
        .and_then(Value::as_str)
        .map(String::from);
    // The last group that looks like a clock is the length; an album is a
    // group linking to an MPRE... page.
    let secs = groups.iter().skip(1).rev().find_map(|g| parse_clock(&join(g)));
    let album = groups.iter().skip(1).find_map(|g| {
        let is_album = g.iter().any(|r| {
            r.pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(Value::as_str)
                .map_or(false, |b| b.starts_with("MPRE"))
        });
        is_album.then(|| join(g))
    });
    Byline { artist, artist_id, album, secs }
}

/// A larger version of a Google image URL (they end in `=w60-h60-...`).
fn big_cover(url: &str) -> String {
    match url.rsplit_once('=') {
        Some((base, size)) if size.starts_with('w') && size.contains("-h") => format!("{base}=w544-h544-l90-rj"),
        _ => url.to_string(),
    }
}

fn thumb(v: &Value) -> Option<String> {
    let mut found = vec![];
    find_all(v, "thumbnails", &mut found);
    found
        .first()
        .and_then(|t| t.as_array())
        .and_then(|a| a.last())
        .and_then(|t| t.get("url"))
        .and_then(Value::as_str)
        .map(big_cover)
}

pub fn parse_search(v: &Value) -> Vec<Song> {
    let mut items = vec![];
    find_all(v, "musicResponsiveListItemRenderer", &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter_map(|it| {
            let id = it.pointer("/playlistItemData/videoId").and_then(Value::as_str)?;
            let cols = it.get("flexColumns")?.as_array()?;
            let col = |i: usize| {
                cols.get(i)
                    .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
                    .and_then(Value::as_array)
            };
            let title = col(0)?.first().and_then(text)?.to_string();
            let b = byline(col(1)?);
            if b.artist.is_empty() || !seen.insert(id.to_string()) {
                return None;
            }
            Some(Song {
                id: id.to_string(),
                title,
                artist: b.artist,
                artist_id: b.artist_id,
                album: b.album,
                duration_secs: b.secs,
                thumbnail: thumb(it),
            })
        })
        .collect()
}

pub fn parse_radio(v: &Value) -> Vec<Song> {
    let mut items = vec![];
    find_all(v, "playlistPanelVideoRenderer", &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter_map(|it| {
            let id = it.get("videoId").and_then(Value::as_str)?;
            let title = it.pointer("/title/runs/0/text").and_then(Value::as_str)?.to_string();
            let runs = it.pointer("/longBylineText/runs").and_then(Value::as_array)?;
            let b = byline(runs);
            let secs = it
                .pointer("/lengthText/runs/0/text")
                .and_then(Value::as_str)
                .and_then(parse_clock)
                .or(b.secs);
            if b.artist.is_empty() || !seen.insert(id.to_string()) {
                return None;
            }
            Some(Song {
                id: id.to_string(),
                title,
                artist: b.artist,
                artist_id: b.artist_id,
                album: b.album,
                duration_secs: secs,
                thumbnail: thumb(it),
            })
        })
        .collect()
}


// --- albums, playlists and podcasts ---

/// What a search can look for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Songs,
    Albums,
    Playlists,
    Podcasts,
    Artists,
}

impl Kind {
    pub fn parse(s: &str) -> Kind {
        match s {
            "albums" => Kind::Albums,
            "playlists" => Kind::Playlists,
            "podcasts" => Kind::Podcasts,
            "artists" => Kind::Artists,
            _ => Kind::Songs,
        }
    }

    /// YouTube Music's search filter (the `params` of its search page).
    fn params(self) -> String {
        let p2 = match self {
            Kind::Songs => "II",
            Kind::Albums => "IY",
            Kind::Playlists => "Io",
            Kind::Podcasts => "JQ",
            Kind::Artists => "Ig",
        };
        format!("EgWKAQ{p2}AWoKEAkQBRAKEAMQBA%3D%3D")
    }
}

/// An album, playlist or podcast show that holds songs or episodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    /// The browse id; pass it to [`open`].
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: Option<String>,
    /// "album", "playlist" or "podcast"
    pub kind: String,
}

fn kind_of_id(id: &str) -> Option<&'static str> {
    if id.starts_with("MPRE") {
        Some("album")
    } else if id.starts_with("MPSP") {
        Some("podcast")
    } else if id.starts_with("VL") {
        Some("playlist")
    } else if id.starts_with("UC") {
        Some("artist")
    } else {
        None
    }
}

pub fn search_collections(kind: Kind, query: &str, limit: usize) -> Result<Vec<Collection>> {
    let v = post("search", json!({"query": query, "params": kind.params()}))?;
    let mut c = parse_collections(&v);
    c.truncate(limit);
    Ok(c)
}

pub fn parse_collections(v: &Value) -> Vec<Collection> {
    let mut items = vec![];
    find_all(v, "musicResponsiveListItemRenderer", &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter_map(|it| {
            let id = it.pointer("/navigationEndpoint/browseEndpoint/browseId").and_then(Value::as_str)?;
            let kind = kind_of_id(id)?;
            let cols = it.get("flexColumns")?.as_array()?;
            let col = |i: usize| {
                cols.get(i)
                    .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(text).collect::<String>())
            };
            let title = col(0).filter(|t| !t.is_empty())?;
            if !seen.insert(id.to_string()) {
                return None;
            }
            Some(Collection { id: id.into(), title, subtitle: col(1).unwrap_or_default(), thumbnail: thumb(it), kind: kind.into() })
        })
        .collect()
}

/// The token for the next page of a long playlist, if there is one.
pub fn next_token(v: &Value) -> Option<String> {
    let mut cmds = vec![];
    find_all(v, "continuationCommand", &mut cmds);
    if let Some(tok) = cmds.iter().find_map(|c| c.get("token").and_then(Value::as_str)) {
        return Some(tok.to_string());
    }
    let mut old = vec![];
    find_all(v, "nextContinuationData", &mut old);
    old.iter().find_map(|c| c.get("continuation").and_then(Value::as_str)).map(String::from)
}

/// The most songs read from one playlist.
pub const MAX_SONGS: usize = 5000;

/// An album, playlist or show's page: its details and ALL its songs or
/// episodes (a playlist comes 100 at a time, so the next pages are followed).
pub fn open(browse_id: &str) -> Result<(Collection, Vec<Song>)> {
    let v = post("browse", json!({"browseId": browse_id}))?;
    let (collection, mut songs) = parse_page(&v, browse_id);
    let ctx = PageCtx {
        title: collection.title.clone(),
        owner: collection.subtitle.clone(),
        owner_id: None,
        cover: collection.thumbnail.clone(),
        kind: kind_of_id(browse_id).unwrap_or("playlist"),
    };
    let mut seen: std::collections::HashSet<String> = songs.iter().map(|s| s.id.clone()).collect();
    let mut token = next_token(&v);
    let mut pages = 0;
    while let Some(t) = token.take() {
        if pages >= 60 || songs.len() >= MAX_SONGS {
            break;
        }
        pages += 1;
        // A failed later page keeps what was already read.
        let Ok(page) = post("browse", json!({"continuation": t})) else { break };
        let before = songs.len();
        collect_songs(&page, &ctx, &mut seen, &mut songs);
        if songs.len() == before {
            break;
        }
        token = next_token(&page);
    }
    Ok((collection, songs))
}

/// An artist's page: their songs, and their albums, singles, playlists and similar artists.
pub fn open_artist(browse_id: &str) -> Result<(Collection, Vec<Song>, Vec<Collection>)> {
    let v = post("browse", json!({"browseId": browse_id}))?;
    let (artist, mut songs, related, all_songs_id) = parse_artist(&v, browse_id);
    // The page shows five top songs; the shelf's title links to the full list.
    if let Some(id) = all_songs_id {
        if let Ok((_, all)) = open(&id) {
            if all.len() > songs.len() {
                songs = all;
                songs.truncate(300);
            }
        }
    }
    Ok((artist, songs, related))
}

pub fn parse_artist(v: &Value, browse_id: &str) -> (Collection, Vec<Song>, Vec<Collection>, Option<String>) {
    let mut headers = vec![];
    find_all(v, "musicImmersiveHeaderRenderer", &mut headers);
    let header = headers.first().copied();
    let name = header.and_then(|h| h.pointer("/title/runs/0/text")).and_then(Value::as_str).unwrap_or("").to_string();
    let cover = header.and_then(thumb);
    let artist = Collection { id: browse_id.into(), title: name.clone(), subtitle: "Artist".into(), thumbnail: cover.clone(), kind: "artist".into() };

    // The first shelf of song rows is "Top songs".
    let mut shelves = vec![];
    find_all(v, "musicShelfRenderer", &mut shelves);
    let mut songs = vec![];
    let mut all_songs_id = None;
    if let Some(shelf) = shelves.first() {
        let ctx = PageCtx { title: name.clone(), owner: name.clone(), owner_id: Some(browse_id.into()), cover: cover.clone(), kind: "playlist" };
        let mut seen = std::collections::HashSet::new();
        collect_songs(shelf, &ctx, &mut seen, &mut songs);
        all_songs_id = shelf
            .pointer("/bottomEndpoint/browseEndpoint/browseId")
            .or_else(|| shelf.pointer("/title/runs/0/navigationEndpoint/browseEndpoint/browseId"))
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("VL"))
            .map(String::from);
    }

    // Albums, singles, playlists, similar artists: the carousels of cards.
    let mut cards = vec![];
    find_all(v, "musicTwoRowItemRenderer", &mut cards);
    let mut seen = std::collections::HashSet::new();
    let related = cards
        .into_iter()
        .filter_map(|c| {
            let id = c.pointer("/navigationEndpoint/browseEndpoint/browseId").and_then(Value::as_str)?;
            let kind = kind_of_id(id)?;
            if id == browse_id || !seen.insert(id.to_string()) {
                return None;
            }
            let title = c.pointer("/title/runs/0/text").and_then(Value::as_str)?.to_string();
            let subtitle = c.pointer("/subtitle/runs").and_then(Value::as_array).map(|r| r.iter().filter_map(text).collect::<String>()).unwrap_or_default();
            Some(Collection { id: id.into(), title, subtitle, thumbnail: thumb(c), kind: kind.into() })
        })
        .collect();
    (artist, songs, related, all_songs_id)
}


/// What a page's rows need to fill in what a row itself leaves out.
struct PageCtx {
    title: String,
    owner: String,
    owner_id: Option<String>,
    cover: Option<String>,
    kind: &'static str,
}

/// Reads every song row (and podcast episode) in a page or a continuation.
fn collect_songs(v: &Value, ctx: &PageCtx, seen: &mut std::collections::HashSet<String>, songs: &mut Vec<Song>) {
    let (title, owner, owner_id, cover, kind) = (&ctx.title, &ctx.owner, &ctx.owner_id, &ctx.cover, ctx.kind);
    // Albums and playlists: rows of songs.
    let mut rows = vec![];
    find_all(v, "musicResponsiveListItemRenderer", &mut rows);
    for it in rows {
        let Some(id) = it.pointer("/playlistItemData/videoId").and_then(Value::as_str) else { continue };
        let Some(cols) = it.get("flexColumns").and_then(Value::as_array) else { continue };
        let col = |i: usize| -> Option<String> {
            cols.get(i)
                .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(text).collect::<String>())
        };
        let Some(song_title) = col(0).filter(|t| !t.is_empty()) else { continue };
        // "Artist" or "Artist • 1.2B plays": keep the artist part.
        let artist = col(1).map(|a| a.split(" • ").next().unwrap_or("").trim().to_string()).filter(|a| !a.is_empty());
        let artist_id = cols
            .get(1)
            .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/browseEndpoint/browseId"))
            .and_then(Value::as_str)
            .map(String::from);
        let secs = it
            .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
            .and_then(Value::as_str)
            .and_then(parse_clock);
        if !seen.insert(id.to_string()) {
            continue;
        }
        songs.push(Song {
            id: id.into(),
            title: song_title,
            artist: artist.unwrap_or_else(|| if owner.is_empty() { "Unknown".into() } else { owner.clone() }),
            artist_id: artist_id.or_else(|| owner_id.clone()),
            album: if kind == "album" { Some(title.clone()) } else { col(2).filter(|a| !a.is_empty()) },
            duration_secs: secs,
            thumbnail: thumb(it).or_else(|| cover.clone()),
        });
    }

    // Podcast shows: episodes.
    let mut eps = vec![];
    find_all(v, "musicMultiRowListItemRenderer", &mut eps);
    for it in eps {
        let Some(id) = it.pointer("/onTap/watchEndpoint/videoId").and_then(Value::as_str) else { continue };
        let Some(ep_title) = it.pointer("/title/runs/0/text").and_then(Value::as_str) else { continue };
        if !seen.insert(id.to_string()) {
            continue;
        }
        songs.push(Song {
            id: id.into(),
            title: ep_title.into(),
            artist: if title.is_empty() { "Podcast".into() } else { title.clone() },
            artist_id: None,
            album: None,
            duration_secs: None,
            thumbnail: thumb(it).or_else(|| cover.clone()),
        });
    }
}

pub fn parse_page(v: &Value, browse_id: &str) -> (Collection, Vec<Song>) {
    let mut headers = vec![];
    find_all(v, "musicResponsiveHeaderRenderer", &mut headers);
    let header = headers.first().copied();
    let title = header
        .and_then(|h| h.pointer("/title/runs/0/text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let strap: Option<&Vec<Value>> = header
        .and_then(|h| h.pointer("/straplineTextOne/runs").or_else(|| h.pointer("/subtitle/runs")))
        .and_then(Value::as_array);
    let owner = strap.and_then(|r| r.first()).and_then(text).unwrap_or("").to_string();
    let owner_id = strap
        .and_then(|r| r.first())
        .and_then(|r| r.pointer("/navigationEndpoint/browseEndpoint/browseId"))
        .and_then(Value::as_str)
        .map(String::from);
    let cover = header.and_then(thumb);
    let kind = kind_of_id(browse_id).unwrap_or("playlist");
    let collection = Collection { id: browse_id.into(), title: title.clone(), subtitle: owner.clone(), thumbnail: cover.clone(), kind: kind.into() };

    let ctx = PageCtx { title: title.clone(), owner: owner.clone(), owner_id: owner_id.clone(), cover: cover.clone(), kind };
    let mut songs = vec![];
    let mut seen = std::collections::HashSet::new();
    collect_songs(v, &ctx, &mut seen, &mut songs);
    (collection, songs)
}


// --- "All" search: everything, in YouTube Music's own relevance order ---

/// One result of an "All" search.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Song(Song),
    /// A podcast episode (playable like a song).
    Episode(Song),
    /// An artist, album (or single / EP) or podcast show.
    Collection(Collection),
}

impl Hit {
    fn id(&self) -> &str {
        match self {
            Hit::Song(s) | Hit::Episode(s) => &s.id,
            Hit::Collection(c) => &c.id,
        }
    }
}

/// Songs, episodes, artists, albums and podcasts matching a query, ranked by YouTube
/// Music. Videos, playlists and profiles are left out.
pub fn search_all(query: &str, limit: usize) -> Result<Vec<Hit>> {
    let v = post("search", json!({"query": query}))?;
    let mut hits = parse_all(&v);
    hits.truncate(limit);
    Ok(hits)
}

/// Splits "Song • Artist • Album • 3:45" into the kind label and the runs after it.
fn label_and_rest(runs: &[Value]) -> (String, Vec<Value>) {
    match runs.iter().position(|r| text(r) == Some(" • ")) {
        Some(i) => (runs[..i].iter().filter_map(text).collect::<String>(), runs[i + 1..].to_vec()),
        None => (runs.iter().filter_map(text).collect::<String>(), vec![]),
    }
}

fn runs_text(runs: &[Value]) -> String {
    runs.iter().filter_map(text).collect()
}

/// Makes a hit from a title, the subtitle runs, where it leads, and its picture.
fn hit_from(title: String, sub: &[Value], video: Option<&str>, browse: Option<&str>, thumbnail: Option<String>) -> Option<Hit> {
    let (label, rest) = label_and_rest(sub);
    let label = label.trim().to_lowercase();
    // A song page id is "MPED" + the video id.
    let video = video.map(String::from).or_else(|| browse.and_then(|b| b.strip_prefix("MPED")).filter(|v| v.len() == 11).map(String::from));
    match label.as_str() {
        "song" | "episode" => {
            let id = video?;
            let b = byline(&rest);
            if label == "song" {
                Some(Hit::Song(Song { id, title, artist: if b.artist.is_empty() { "Unknown".into() } else { b.artist }, artist_id: b.artist_id, album: b.album, duration_secs: b.secs, thumbnail }))
            } else {
                // "Mar 23, 2013 • The Show": the last part names the show.
                let show = rest.split(|r| text(r) == Some(" • ")).last().map(runs_text).unwrap_or_default();
                Some(Hit::Episode(Song { id, title, artist: if show.is_empty() { "Podcast".into() } else { show }, artist_id: None, album: None, duration_secs: b.secs, thumbnail }))
            }
        }
        "artist" | "album" | "single" | "ep" | "podcast" => {
            let id = browse?.to_string();
            let kind = match label.as_str() {
                "artist" => "artist",
                "podcast" => "podcast",
                _ => "album",
            };
            if kind_of_id(&id) != Some(kind) {
                return None;
            }
            let subtitle = if kind == "album" { runs_text(sub) } else { runs_text(&rest) };
            Some(Hit::Collection(Collection { id, title, subtitle, thumbnail, kind: kind.into() }))
        }
        _ => None, // videos, playlists, profiles
    }
}

fn hit_from_row(it: &Value) -> Option<Hit> {
    let cols = it.get("flexColumns")?.as_array()?;
    let col = |i: usize| cols.get(i).and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")).and_then(Value::as_array);
    let title = runs_text(col(0)?);
    if title.is_empty() {
        return None;
    }
    let sub = col(1).cloned().unwrap_or_default();
    let video = it.pointer("/playlistItemData/videoId").and_then(Value::as_str);
    let browse = it.pointer("/navigationEndpoint/browseEndpoint/browseId").and_then(Value::as_str);
    let mut h = hit_from(title, &sub, video, browse, thumb(it))?;
    // Rows show the length in a fixed column for some kinds.
    if let Hit::Song(s) | Hit::Episode(s) = &mut h {
        if s.duration_secs.is_none() {
            s.duration_secs = it
                .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
                .and_then(Value::as_str)
                .and_then(parse_clock);
        }
    }
    Some(h)
}

pub fn parse_all(v: &Value) -> Vec<Hit> {
    let mut sections = vec![];
    find_all(v, "sectionListRenderer", &mut sections);
    let Some(list) = sections.first().and_then(|s| s.get("contents")).and_then(Value::as_array) else { return vec![] };
    let mut out: Vec<Hit> = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut push = |h: Hit, out: &mut Vec<Hit>| {
        if seen.insert(h.id().to_string()) {
            out.push(h);
        }
    };
    for sec in list {
        if let Some(card) = sec.get("musicCardShelfRenderer") {
            // The top result, then the songs shown inside it.
            let runs = card.pointer("/title/runs").and_then(Value::as_array).cloned().unwrap_or_default();
            let nav = runs.first().and_then(|r| r.get("navigationEndpoint")).or_else(|| card.get("onTap"));
            let video = nav.and_then(|n| n.pointer("/watchEndpoint/videoId")).and_then(Value::as_str);
            let browse = nav.and_then(|n| n.pointer("/browseEndpoint/browseId")).and_then(Value::as_str);
            let sub = card.pointer("/subtitle/runs").and_then(Value::as_array).cloned().unwrap_or_default();
            if let Some(h) = hit_from(runs_text(&runs), &sub, video, browse, thumb(card)) {
                push(h, &mut out);
            }
            for row in card.get("contents").and_then(Value::as_array).into_iter().flatten() {
                if let Some(h) = row.get("musicResponsiveListItemRenderer").and_then(hit_from_row) {
                    push(h, &mut out);
                }
            }
        }
        let rows = sec
            .pointer("/itemSectionRenderer/contents")
            .or_else(|| sec.pointer("/musicShelfRenderer/contents"))
            .and_then(Value::as_array);
        for row in rows.into_iter().flatten() {
            if let Some(h) = row.get("musicResponsiveListItemRenderer").and_then(hit_from_row) {
                push(h, &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn search_has_artists_album_length_and_cover() {
        let songs = parse_search(&fixture(include_str!("../tests/fixtures/ytm_search.json")));
        assert_eq!(songs.len(), 5, "the row without a video id is skipped");
        let s = &songs[0];
        assert_eq!(s.id, "4D7u5KF7SP8");
        assert_eq!(s.title, "Get Lucky (feat. Pharrell Williams and Nile Rodgers)");
        assert_eq!(s.artist, "Daft Punk, Pharrell Williams & Nile Rodgers");
        assert_eq!(s.artist_id.as_deref(), Some("UCRr1xG_2WIDs18a6cIiCxeA"));
        assert_eq!(s.album.as_deref(), Some("Random Access Memories"));
        assert_eq!(s.duration_secs, Some(370));
        assert!(s.thumbnail.as_deref().unwrap().contains("w544-h544"));
        assert_eq!(songs[2].artist, "Los del Fuego");
    }

    #[test]
    fn radio_parses_and_skips_the_seed() {
        let songs = parse_radio(&fixture(include_str!("../tests/fixtures/ytm_next.json")));
        assert_eq!(songs.len(), 5);
        assert_eq!(songs[1].artist, "The Weeknd");
        assert_eq!(songs[1].album.as_deref(), Some("Starboy"));
        assert_eq!(songs[1].duration_secs, Some(270));
    }

    #[test]
    fn clocks_and_covers() {
        assert_eq!(parse_clock("6:10"), Some(370));
        assert_eq!(parse_clock("1:02:03"), Some(3723));
        assert_eq!(parse_clock("1.9B plays"), None);
        assert_eq!(parse_clock("2024"), None);
        assert_eq!(big_cover("https://x/y=w60-h60-l90-rj"), "https://x/y=w544-h544-l90-rj");
        assert_eq!(big_cover("https://i.ytimg.com/vi/a/hq.jpg"), "https://i.ytimg.com/vi/a/hq.jpg");
    }

    #[test]
    fn album_search_results() {
        let c = parse_collections(&fixture(include_str!("../tests/fixtures/ytm_search_albums.json")));
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].kind, "album");
        assert!(c[0].id.starts_with("MPRE"));
        assert_eq!(c[0].title, "The Arijit Singh Collection");
        assert!(c[0].subtitle.contains("Arijit Singh"));
        assert!(c[0].thumbnail.is_some());
    }

    #[test]
    fn album_page_gives_songs_with_the_album_artist_and_cover() {
        let (c, songs) = parse_page(&fixture(include_str!("../tests/fixtures/ytm_album.json")), "MPREb_E4GfUXfDfhy");
        assert_eq!(c.title, "Aashiqui 2");
        assert_eq!(c.kind, "album");
        assert_eq!(songs.len(), 3);
        assert_eq!(songs[0].title, "Tum Hi Ho");
        assert_eq!(songs[0].artist, "Arijit Singh", "the plays count is dropped from the artist");
        assert_eq!(songs[0].album.as_deref(), Some("Aashiqui 2"));
        assert!(songs[0].duration_secs.is_some());
        assert!(songs[0].thumbnail.is_some(), "falls back to the album cover");
    }

    #[test]
    fn playlist_page() {
        let (c, songs) = parse_page(&fixture(include_str!("../tests/fixtures/ytm_playlist.json")), "VLPLBWZKm-dhAC7ZZ0EI-XWhwQNsvKi06hDw");
        assert_eq!(c.kind, "playlist");
        assert_eq!(c.title, "Bollywood Gym/Exercise/Workout");
        assert_eq!(songs.len(), 3);
        assert!(songs.iter().all(|s| !s.artist.is_empty() && s.id.len() == 11));
    }

    #[test]
    fn podcast_page_has_episodes_named_after_the_show() {
        let (c, eps) = parse_page(&fixture(include_str!("../tests/fixtures/ytm_podcast.json")), "MPSPPLk1Sqn_f33KuWf3tW9BBe_4TP7x8l0m3T");
        assert_eq!(c.kind, "podcast");
        assert_eq!(eps.len(), 3);
        assert!(eps[0].title.contains("Joe Rogan Experience"));
        assert_eq!(eps[0].artist, c.title);
        assert_eq!(eps[0].id.len(), 11);
    }

    #[test]
    fn long_playlists_are_followed_page_by_page() {
        // The first page ends in a token; the next page has rows and another token.
        let first = fixture(include_str!("../tests/fixtures/ytm_playlist_paged.json"));
        assert_eq!(next_token(&first).as_deref(), Some("FIRSTTOKEN"));
        let more = fixture(include_str!("../tests/fixtures/ytm_continuation.json"));
        let ctx = PageCtx { title: "T".into(), owner: "Owner".into(), owner_id: None, cover: None, kind: "playlist" };
        let (mut seen, mut songs) = (std::collections::HashSet::new(), vec![]);
        collect_songs(&more, &ctx, &mut seen, &mut songs);
        assert_eq!(songs.len(), 2);
        assert!(songs[0].id.len() == 11 && !songs[0].artist.is_empty());
        assert!(next_token(&more).is_some(), "and it points on to the page after");
        // The same rows twice add nothing, which is what stops the loop.
        collect_songs(&more, &ctx, &mut seen, &mut songs);
        assert_eq!(songs.len(), 2);
        assert_eq!(next_token(&json!({"a": 1})), None);
        let old = json!({"x": {"nextContinuationData": {"continuation": "OLD"}}});
        assert_eq!(next_token(&old).as_deref(), Some("OLD"));
    }

    #[test]
    fn artist_page_has_songs_albums_and_a_link_to_all_songs() {
        let (a, songs, related, all) = parse_artist(&fixture(include_str!("../tests/fixtures/ytm_artist.json")), "UCDxKh1gFWeYsqePvgVzmPoQ");
        assert_eq!((a.title.as_str(), a.kind.as_str()), ("Arijit Singh", "artist"));
        assert!(a.thumbnail.is_some());
        assert_eq!(songs.len(), 3);
        assert!(all.as_deref().unwrap().starts_with("VLOLAK5uy_"), "the shelf links to the full song list");
        let kinds: std::collections::HashSet<_> = related.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains("album") && kinds.contains("playlist") && kinds.contains("artist"), "{kinds:?}");
        assert!(related.iter().all(|c| !c.title.is_empty() && c.id != "UCDxKh1gFWeYsqePvgVzmPoQ"));
    }

    fn kinds(h: &[Hit]) -> Vec<String> {
        h.iter()
            .map(|h| match h {
                Hit::Song(s) => format!("song:{}", s.title),
                Hit::Episode(s) => format!("episode:{}", s.artist),
                Hit::Collection(c) => format!("{}:{}", c.kind, c.title),
            })
            .collect()
    }

    #[test]
    fn all_search_keeps_youtube_music_order_and_drops_videos_and_playlists() {
        let h = parse_all(&fixture(include_str!("../tests/fixtures/ytm_all_artist.json")));
        let k = kinds(&h);
        assert_eq!(k[0], "artist:Arijit Singh", "the top result comes first");
        assert!(k[1].starts_with("song:"), "then the songs inside the top-result card: {k:?}");
        assert!(!k.iter().any(|x| x.contains("Spotlight") || x.contains("Lofi")), "no playlists or videos: {k:?}");
        let pos = |p: &str| k.iter().position(|x| x == p).unwrap();
        assert!(pos("artist:Atif Aslam") < pos("album:The Arijit Singh Collection"), "relevance order is kept");
        let ids: std::collections::HashSet<_> = h.iter().map(|x| x.id().to_string()).collect();
        assert_eq!(ids.len(), h.len(), "no duplicates");
        if let Hit::Song(s) = &h[1] {
            assert_eq!(s.id.len(), 11);
            assert!(!s.artist.is_empty() && s.artist != "Unknown");
        }
    }

    #[test]
    fn all_search_for_a_song_and_for_a_podcast() {
        let h = parse_all(&fixture(include_str!("../tests/fixtures/ytm_all_song.json")));
        let k = kinds(&h);
        // YouTube Music itself labels this top result an "Episode" (dated, by the singer).
        assert_eq!(k[0], "episode:Arijit Singh", "{k:?}");
        assert!(k[1].starts_with("song:Tum Hi Ho"), "{k:?}");
        assert!(k.iter().any(|x| x.starts_with("album:")), "singles count as albums: {k:?}");
        assert!(k.iter().any(|x| x == "artist:Arijit Singh"));
        let p = kinds(&parse_all(&fixture(include_str!("../tests/fixtures/ytm_all_podcast.json"))));
        assert!(p[0].starts_with("episode:"), "{p:?}");
        assert!(p.iter().any(|x| x == "artist:Joe Rogan"));
        assert!(parse_all(&json!({"x": 1})).is_empty());
    }

    #[test]
    fn kinds_and_filters() {
        assert_eq!(Kind::parse("artists"), Kind::Artists);
        assert!(Kind::Artists.params().starts_with("EgWKAQIg"));
        assert_eq!(kind_of_id("UCabc"), Some("artist"));
        assert_eq!(Kind::parse("podcasts"), Kind::Podcasts);
        assert_eq!(Kind::parse("x"), Kind::Songs);
        assert!(Kind::Albums.params().starts_with("EgWKAQIY"));
        assert_eq!(kind_of_id("VLPL1"), Some("playlist"));
        assert_eq!(kind_of_id("MPED1"), None, "music videos are not collections");
    }

    #[test]
    fn unknown_shapes_give_nothing_not_a_panic() {
        assert!(parse_collections(&json!([1, {"a": 2}])).is_empty());
        assert!(parse_page(&json!("x"), "MPRE1").1.is_empty());
        assert!(parse_search(&json!({"contents": [1, 2, {"a": null}]})).is_empty());
        assert!(parse_radio(&json!("nope")).is_empty());
    }
}

/// Live check against the real service. Run with
/// `cargo test -p harp-ytdlp -- --ignored live`.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_long_playlist_and_artist() {
        // A public playlist with several hundred songs: all of them, not the first 100.
        let (c, songs) = open("VLPLwNv9Hhd8gZjNoQdpd2kBa3fwXNeJjzDX").unwrap();
        assert_eq!(c.kind, "playlist");
        assert!(songs.len() > 150, "got {}", songs.len());
        let ids: std::collections::HashSet<_> = songs.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), songs.len(), "no duplicates across pages");
        // An artist found by search, opened with their full song list and albums.
        let found = search_collections(Kind::Artists, "arijit singh", 5).unwrap();
        assert_eq!(found[0].kind, "artist");
        let (a, songs, related) = open_artist(&found[0].id).unwrap();
        assert!(a.title.contains("Arijit") && songs.len() > 5 && !related.is_empty());
    }

    #[test]
    #[ignore]
    fn live_search_and_radio() {
        let s = search("daft punk get lucky", 5).unwrap();
        assert!(s.iter().any(|x| x.artist.contains("Daft Punk")), "{s:?}");
        let r = radio(&s[0].id, 10).unwrap();
        assert!(!r.is_empty());
    }
}
