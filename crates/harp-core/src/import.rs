//! Bringing playlists over from other services: parsers for what those
//! services let you export (or paste), and a matcher that scores search
//! results against the original track. No accounts, no API keys.

use crate::model::Song;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ImportTrack {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedPlaylist {
    pub name: String,
    pub tracks: Vec<ImportTrack>,
}

fn track(title: &str, artist: &str, album: Option<&str>, secs: Option<u32>) -> Option<ImportTrack> {
    let (title, artist) = (title.trim(), artist.trim());
    if title.is_empty() {
        return None;
    }
    Some(ImportTrack {
        title: title.into(),
        artist: artist.into(),
        album: album.map(str::trim).filter(|a| !a.is_empty()).map(String::from),
        duration_secs: secs,
    })
}

/// Detects the format and parses it. `name_hint` names playlists that have no
/// name of their own (usually the file name).
pub fn parse(text: &str, name_hint: &str) -> Vec<ImportedPlaylist> {
    let t = text.trim_start_matches('\u{feff}').trim();
    let hint = if name_hint.trim().is_empty() { "Imported playlist" } else { name_hint.trim() };
    let out = if t.starts_with('{') || t.starts_with('[') {
        parse_json(t, hint)
    } else if t.starts_with("#EXTM3U") || t.contains("#EXTINF") {
        vec![ImportedPlaylist { name: hint.into(), tracks: parse_m3u(t) }]
    } else {
        parse_table_or_lines(t, hint)
    };
    out.into_iter().filter(|p| !p.tracks.is_empty()).collect()
}

fn parse_json(t: &str, hint: &str) -> Vec<ImportedPlaylist> {
    let Ok(v) = serde_json::from_str::<Value>(t) else { return vec![] };
    // Spotify "download your data": {"playlists":[{"name","items":[{"track":{trackName,artistName,albumName}}]}]}
    if let Some(pls) = v.get("playlists").and_then(Value::as_array) {
        return pls.iter().map(|p| spotify_playlist(p, hint)).collect();
    }
    // A single playlist object, or the Web API shape.
    if v.get("items").is_some() || v.get("tracks").is_some() {
        return vec![spotify_playlist(&v, hint)];
    }
    if let Some(a) = v.as_array() {
        // A bare list of tracks or of playlists.
        if a.iter().all(|x| x.get("items").is_some() || x.get("tracks").is_some()) && !a.is_empty() {
            return a.iter().map(|p| spotify_playlist(p, hint)).collect();
        }
        return vec![ImportedPlaylist { name: hint.into(), tracks: a.iter().filter_map(json_track).collect() }];
    }
    vec![]
}

fn spotify_playlist(p: &Value, hint: &str) -> ImportedPlaylist {
    let name = p.get("name").and_then(Value::as_str).unwrap_or(hint).to_string();
    let items = p
        .get("items")
        .or_else(|| p.get("tracks").and_then(|t| t.get("items").or(Some(t))))
        .and_then(Value::as_array);
    let tracks = items
        .map(|a| a.iter().filter_map(|i| json_track(i.get("track").unwrap_or(i))).collect())
        .unwrap_or_default();
    ImportedPlaylist { name, tracks }
}

fn json_track(v: &Value) -> Option<ImportTrack> {
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    let title = s("trackName").or_else(|| s("name")).or_else(|| s("title"))?;
    let artist = s("artistName")
        .map(String::from)
        .or_else(|| s("artist").map(String::from))
        .or_else(|| {
            v.get("artists").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|x| x.get("name").and_then(Value::as_str).or_else(|| x.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
        })
        .unwrap_or_default();
    let album = s("albumName").or_else(|| v.get("album").and_then(|a| a.get("name")).and_then(Value::as_str));
    let secs = v.get("duration_ms").and_then(Value::as_u64).map(|m| (m / 1000) as u32);
    track(title, &artist, album, secs)
}

fn parse_m3u(t: &str) -> Vec<ImportTrack> {
    let mut out = vec![];
    let mut pending: Option<(Option<u32>, String)> = None;
    for line in t.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (secs, name) = rest.split_once(',').unwrap_or(("-1", rest));
            let secs = secs.split_whitespace().next().and_then(|s| s.parse::<i64>().ok()).filter(|s| *s > 0);
            pending = Some((secs.map(|s| s as u32), name.trim().to_string()));
        } else if !line.is_empty() && !line.starts_with('#') {
            // The file line: use the EXTINF name, else the file name.
            let (secs, name) = pending.take().unwrap_or_else(|| {
                let stem = line.rsplit(['/', '\\']).next().unwrap_or(line);
                (None, stem.rsplit_once('.').map_or(stem, |(a, _)| a).to_string())
            });
            if let Some(t) = line_track(&name, secs) {
                out.push(t);
            }
        }
    }
    out
}

/// "Artist - Title" (or just a title).
fn line_track(line: &str, secs: Option<u32>) -> Option<ImportTrack> {
    for sep in [" - ", " – ", " — "] {
        if let Some((a, b)) = line.split_once(sep) {
            return track(b, a, None, secs);
        }
    }
    track(line, "", None, secs)
}

/// Splits one CSV or TSV line, honoring "quoted, fields".
fn split_row(line: &str, delim: char) -> Vec<String> {
    let (mut out, mut cur, mut q) = (vec![], String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => q = !q,
            c if c == delim && !q => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn parse_table_or_lines(t: &str, hint: &str) -> Vec<ImportedPlaylist> {
    let mut lines = t.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else { return vec![] };
    let delim = if first.contains('\t') { '\t' } else { ',' };
    let head: Vec<String> = split_row(first, delim).iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |names: &[&str]| head.iter().position(|h| names.iter().any(|n| h == n || h.starts_with(n)));
    // Exportify (Spotify CSV), Apple Music's text export, TuneMyMusic-style CSVs...
    let title_col = col(&["track name", "name", "title", "song"]);
    let artist_col = col(&["artist name", "artist", "artists"]);
    if let (Some(tc), Some(ac)) = (title_col, artist_col) {
        let album_col = col(&["album name", "album"]);
        let dur_ms = col(&["duration (ms)", "duration_ms"]);
        let dur_s = col(&["time", "duration"]);
        let tracks = lines
            .filter_map(|l| {
                let r = split_row(l, delim);
                let get = |i: usize| r.get(i).map(String::as_str).unwrap_or("");
                // Spotify lists several artists separated by ';'.
                let artist = get(ac).replace(';', ", ");
                let secs = dur_ms
                    .and_then(|i| get(i).trim().parse::<u64>().ok())
                    .map(|m| (m / 1000) as u32)
                    .or_else(|| dur_s.and_then(|i| get(i).trim().parse::<u32>().ok()));
                track(get(tc), &artist, album_col.map(get), secs)
            })
            .collect();
        return vec![ImportedPlaylist { name: hint.into(), tracks }];
    }
    // Plain pasted lines: "Artist - Title", one per line (Amazon Music, notes...).
    let tracks = std::iter::once(first).chain(lines).filter_map(|l| {
        let l = l.trim().trim_start_matches(|c: char| c.is_ascii_digit() || ".) ".contains(c)).trim();
        line_track(l, None)
    });
    vec![ImportedPlaylist { name: hint.into(), tracks: tracks.collect() }]
}

/// Tracks of a Spotify playlist from its public embed page
/// (`open.spotify.com/embed/playlist/<id>`), which carries them as JSON.
pub fn parse_spotify_embed(html: &str) -> Option<ImportedPlaylist> {
    let start = html.find("__NEXT_DATA__")?;
    let open = html[start..].find('>')? + start + 1;
    let close = html[open..].find("</script>")? + open;
    let v: Value = serde_json::from_str(&html[open..close]).ok()?;
    let entity = v.pointer("/props/pageProps/state/data/entity")?;
    let name = entity.get("name").or_else(|| entity.get("title")).and_then(Value::as_str).unwrap_or("Spotify playlist");
    let tracks = entity
        .get("trackList")?
        .as_array()?
        .iter()
        .filter_map(|t| {
            let title = t.get("title").and_then(Value::as_str)?;
            let artist = t.get("subtitle").and_then(Value::as_str).unwrap_or("").replace('\u{a0}', " ");
            let secs = t.get("duration").and_then(Value::as_u64).map(|ms| (ms / 1000) as u32);
            track(title, &artist, None, secs)
        })
        .collect();
    Some(ImportedPlaylist { name: name.into(), tracks })
}

/// Tracks of a public Apple Music playlist or album page. The page carries its tracks
/// in a JSON block (`serialized-server-data`), and its name in structured data.
pub fn parse_apple_music_page(html: &str) -> Option<ImportedPlaylist> {
    let block = |marker: &str| -> Option<&str> {
        let at = html.find(marker)?;
        let open = html[at..].find('>')? + at + 1;
        let close = html[open..].find("</script>")? + open;
        Some(&html[open..close])
    };
    let data: Value = serde_json::from_str(block("id=\"serialized-server-data\"")?).ok()?;
    let name = block("application/ld+json")
        .and_then(|j| serde_json::from_str::<Value>(j).ok())
        .and_then(|v| v.get("name").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "Apple Music playlist".into());
    let mut tracks = Vec::new();
    fn walk(v: &Value, out: &mut Vec<ImportTrack>) {
        match v {
            Value::Object(m) => {
                // A track row has a title, an artist name and a length in milliseconds.
                if let (Some(title), Some(artist), Some(ms)) = (
                    m.get("title").and_then(Value::as_str),
                    m.get("artistName").and_then(Value::as_str),
                    m.get("duration").and_then(Value::as_u64),
                ) {
                    let album = m
                        .get("tertiaryLinks")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(|l| l.get("title"))
                        .and_then(Value::as_str);
                    if let Some(t) = track(title, artist, album, Some((ms / 1000) as u32)) {
                        out.push(t);
                    }
                    return;
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(&data, &mut tracks);
    (!tracks.is_empty()).then(|| ImportedPlaylist { name, tracks })
}

// --- matching ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    Confident,
    Weak,
    None,
}

pub const CONFIDENT: f64 = 0.78;
pub const WEAK: f64 = 0.50;

pub fn confidence(score: f64) -> Confidence {
    if score >= CONFIDENT {
        Confidence::Confident
    } else if score >= WEAK {
        Confidence::Weak
    } else {
        Confidence::None
    }
}

/// Lowercase words without brackets, "feat." credits, punctuation or accents of the common kind.
fn words(s: &str) -> Vec<String> {
    let mut depth = 0;
    let mut cleaned = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => cleaned.push(c),
            _ => {}
        }
    }
    let lower = cleaned.to_lowercase();
    let lower = lower
        .split(" feat")
        .next()
        .unwrap_or("")
        .split(" ft.")
        .next()
        .unwrap_or("")
        .split(" - remaster")
        .next()
        .unwrap_or("")
        .to_string();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "the" | "a" | "an" | "and" | "feat" | "ft" | "official"))
        .map(String::from)
        .collect()
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let hit = a.iter().filter(|w| b.contains(w)).count() as f64;
    // Dice coefficient: forgiving of extra words on either side.
    2.0 * hit / (a.len() + b.len()) as f64
}

/// 0..=1 how well a search result matches the original track.
pub fn score(t: &ImportTrack, c: &Song) -> f64 {
    let title = overlap(&words(&t.title), &words(&c.title));
    // The artist can live in the candidate's artist field or, for loose results, its title.
    let ta = words(&t.artist);
    let artist = if ta.is_empty() {
        0.5
    } else {
        overlap(&ta, &words(&c.artist)).max(overlap(&ta, &words(&format!("{} {}", c.artist, c.title))) * 0.9)
    };
    let (mut w_title, mut w_artist, mut w_dur) = (0.55, 0.30, 0.15);
    let dur = match (t.duration_secs, c.duration_secs) {
        (Some(a), Some(b)) => {
            let d = (a as i64 - b as i64).unsigned_abs();
            if d <= 4 { 1.0 } else if d <= 12 { 0.6 } else if d <= 30 { 0.25 } else { 0.0 }
        }
        _ => {
            // Unknown length: spread its weight over title and artist.
            w_title += 0.10;
            w_artist += 0.05;
            w_dur = 0.0;
            0.0
        }
    };
    (w_title * title + w_artist * artist + w_dur * dur).clamp(0.0, 1.0)
}

/// Candidates best first, with scores.
pub fn rank(t: &ImportTrack, cands: &[Song]) -> Vec<(Song, f64)> {
    let mut v: Vec<(Song, f64)> = cands.iter().map(|c| (c.clone(), score(t, c))).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::song;

    #[test]
    fn spotify_data_export() {
        let j = r#"{"playlists":[{"name":"Gym","items":[
          {"track":{"trackName":"Blinding Lights","artistName":"The Weeknd","albumName":"After Hours"},"episode":null},
          {"track":null},
          {"track":{"trackName":"Levitating","artistName":"Dua Lipa","albumName":"Future Nostalgia"}}]}]}"#;
        let p = parse(j, "x");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "Gym");
        assert_eq!(p[0].tracks.len(), 2);
        assert_eq!(p[0].tracks[0].artist, "The Weeknd");
    }

    #[test]
    fn spotify_web_api_shape() {
        let j = r#"{"name":"Mix","tracks":{"items":[{"track":{"name":"Hello","artists":[{"name":"Adele"}],"album":{"name":"25"},"duration_ms":295000}}]}}"#;
        let p = parse(j, "x");
        assert_eq!(p[0].name, "Mix");
        assert_eq!(p[0].tracks[0].duration_secs, Some(295));
        assert_eq!(p[0].tracks[0].album.as_deref(), Some("25"));
    }

    #[test]
    fn exportify_csv_with_quotes() {
        let c = "Track URI,Track Name,Artist Name(s),Album Name,Duration (ms)\n\
                 spotify:track:1,\"Hello, Goodbye\",The Beatles;Other,Magical Mystery Tour,207000\n\
                 spotify:track:2,Yesterday,The Beatles,Help!,125000\n";
        let p = parse(c, "My CSV");
        assert_eq!(p[0].name, "My CSV");
        assert_eq!(p[0].tracks[0].title, "Hello, Goodbye");
        assert_eq!(p[0].tracks[0].artist, "The Beatles, Other");
        assert_eq!(p[0].tracks[1].duration_secs, Some(125));
    }

    #[test]
    fn apple_music_tab_export() {
        let c = "Name\tArtist\tComposer\tAlbum\tGenre\tSize\tTime\n\
                 Shape of You\tEd Sheeran\t\t÷\tPop\t1\t234\n";
        let p = parse(c, "Apple");
        assert_eq!(p[0].tracks[0].title, "Shape of You");
        assert_eq!(p[0].tracks[0].artist, "Ed Sheeran");
        assert_eq!(p[0].tracks[0].duration_secs, Some(234));
    }

    #[test]
    fn m3u_and_pasted_lines() {
        let m = "#EXTM3U\n#EXTINF:215,Coldplay - Yellow\n/music/yellow.mp3\n/music/Song Two.flac\n";
        let p = parse(m, "pl");
        assert_eq!(p[0].tracks[0], ImportTrack { title: "Yellow".into(), artist: "Coldplay".into(), album: None, duration_secs: Some(215) });
        assert_eq!(p[0].tracks[1].title, "Song Two");
        let lines = "1. Queen - Bohemian Rhapsody\n2) ABBA – Dancing Queen\n\nJust A Title";
        let p = parse(lines, "pasted");
        assert_eq!(p[0].tracks.len(), 3);
        assert_eq!(p[0].tracks[1].artist, "ABBA");
        assert_eq!(p[0].tracks[2].artist, "");
    }

    #[test]
    fn empty_or_garbage_gives_nothing() {
        assert!(parse("", "x").is_empty());
        assert!(parse("{not json", "x").is_empty());
    }

    #[test]
    fn spotify_embed_page() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"name":"Road","trackList":[{"title":"Africa","subtitle":"Toto","duration":295000},{"title":"Karma Police","subtitle":"Radiohead "}]}}}}}}</script></html>"#;
        let p = parse_spotify_embed(html).unwrap();
        assert_eq!(p.name, "Road");
        assert_eq!(p.tracks[0].duration_secs, Some(295));
        assert_eq!(p.tracks[1].artist, "Radiohead");
        assert!(parse_spotify_embed("<html></html>").is_none());
    }

    #[test]
    fn apple_music_playlist_page() {
        let p = parse_apple_music_page(include_str!("../tests/apple_playlist.html")).unwrap();
        assert_eq!(p.name, "Today’s Hits");
        assert_eq!(p.tracks.len(), 3);
        assert_eq!(p.tracks[0].title, "Choosin' Texas");
        assert_eq!(p.tracks[0].artist, "Ella Langley");
        assert_eq!(p.tracks[0].duration_secs, Some(232));
        assert!(parse_apple_music_page("<html></html>").is_none());
    }

    fn it(title: &str, artist: &str, secs: Option<u32>) -> ImportTrack {
        ImportTrack { title: title.into(), artist: artist.into(), album: None, duration_secs: secs }
    }
    fn cand(title: &str, artist: &str, secs: u32) -> Song {
        let mut s = song("x", title, artist);
        s.duration_secs = Some(secs);
        s
    }

    #[test]
    fn matcher_prefers_the_right_song() {
        let t = it("Get Lucky", "Daft Punk", Some(248));
        let good = cand("Get Lucky (feat. Pharrell Williams and Nile Rodgers)", "Daft Punk, Pharrell Williams & Nile Rodgers", 369);
        let radio = cand("Get Lucky - Radio Edit", "Daft Punk", 249);
        let wrong = cand("Get Lucky", "Los del Fuego", 179);
        let (sg, sr, sw) = (score(&t, &good), score(&t, &radio), score(&t, &wrong));
        assert!(sr > sg, "length matches the radio edit: {sr} {sg}");
        assert!(sg > sw && sr > sw);
        assert_eq!(confidence(sr), Confidence::Confident);
        assert_ne!(confidence(sw), Confidence::Confident, "wrong artist is not confident: {sw}");
        let r = rank(&t, &[wrong.clone(), good.clone(), radio.clone()]);
        assert_eq!(r[0].0, radio);
    }

    #[test]
    fn unrelated_is_none_and_missing_length_still_works() {
        let t = it("Yellow", "Coldplay", None);
        assert_eq!(confidence(score(&t, &cand("Bohemian Rhapsody", "Queen", 355))), Confidence::None);
        assert_eq!(confidence(score(&t, &cand("Yellow", "Coldplay", 266))), Confidence::Confident);
        assert!(score(&it("x", "", None), &cand("x", "Y", 1)) <= 1.0);
    }

    #[test]
    fn words_ignore_brackets_and_credits() {
        assert_eq!(words("Song (Live) [HD] ft. Someone"), ["song"]);
        assert_eq!(words("The Beatles"), ["beatles"]);
    }
}
