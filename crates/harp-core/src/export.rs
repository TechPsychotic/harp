//! Saving playlists as files. The CSV uses Exportify's column names, so it opens in
//! the same tools and Harp can read it back; the JSON follows Spotify's own data
//! export; the M3U works in nearly any player.

use crate::model::Song;

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn watch_url(id: &str) -> String {
    format!("https://music.youtube.com/watch?v={id}")
}

/// Exportify-style CSV: one row per song, every field quoted.
pub fn to_csv(songs: &[Song]) -> String {
    let mut out = String::from("\"Track URI\",\"Track Name\",\"Album Name\",\"Artist Name(s)\",\"Duration (ms)\"\n");
    for s in songs {
        let row = [
            watch_url(&s.id),
            s.title.clone(),
            s.album.clone().unwrap_or_default(),
            s.artist.clone(),
            s.duration_secs.map(|d| (d as u64 * 1000).to_string()).unwrap_or_default(),
        ];
        out.push_str(&row.iter().map(|f| quote(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

pub fn to_m3u(songs: &[Song]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for s in songs {
        let secs = s.duration_secs.map(|d| d as i64).unwrap_or(-1);
        let name = if s.artist.is_empty() { s.title.clone() } else { format!("{} - {}", s.artist, s.title) };
        out.push_str(&format!("#EXTINF:{secs},{name}\n{}\n", watch_url(&s.id)));
    }
    out
}

/// Shaped like Spotify's "download your data" playlist file.
pub fn to_json(name: &str, songs: &[Song]) -> String {
    let items: Vec<serde_json::Value> = songs
        .iter()
        .map(|s| {
            serde_json::json!({"track": {
                "trackName": s.title, "artistName": s.artist,
                "albumName": s.album.clone().unwrap_or_default(),
                "trackUri": watch_url(&s.id)}})
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({"playlists": [{"name": name, "items": items}]})).unwrap_or_default()
}

/// A safe file name for a playlist.
pub fn file_name(playlist: &str, ext: &str) -> String {
    let clean: String = playlist.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let clean = clean.trim();
    // A name that was only punctuation ("..", "///") becomes a plain one.
    let nothing_left = clean.chars().all(|c| c == '_' || c == ' ');
    format!("{}.{ext}", if nothing_left { "playlist" } else { clean })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::parse;
    use crate::model::song;

    fn sample() -> Vec<Song> {
        let mut a = song("AAAAAAAAAAA", "Hello, \"Goodbye\"", "The Beatles");
        a.album = Some("Magical, Mystery".into());
        a.duration_secs = Some(207);
        let b = song("BBBBBBBBBBB", "Plain", "Solo");
        vec![a, b]
    }

    #[test]
    fn csv_quotes_commas_and_quotes_and_reads_back() {
        let csv = to_csv(&sample());
        assert!(csv.starts_with("\"Track URI\",\"Track Name\",\"Album Name\",\"Artist Name(s)\",\"Duration (ms)\"\n"));
        assert!(csv.contains("\"Hello, \"\"Goodbye\"\"\""));
        let back = parse(&csv, "x");
        let t = &back[0].tracks;
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].title.as_str(), t[0].artist.as_str()), ("Hello, \"Goodbye\"", "The Beatles"));
        assert_eq!(t[0].album.as_deref(), Some("Magical, Mystery"));
        assert_eq!(t[0].duration_secs, Some(207));
    }

    #[test]
    fn json_and_m3u_read_back() {
        let j = to_json("Road trip", &sample());
        let back = parse(&j, "x");
        assert_eq!(back[0].name, "Road trip");
        assert_eq!(back[0].tracks.len(), 2);
        assert_eq!(back[0].tracks[1].artist, "Solo");
        let m = to_m3u(&sample());
        let back = parse(&m, "x");
        assert_eq!(back[0].tracks[0].title, "Hello, \"Goodbye\"");
        assert_eq!(back[0].tracks[0].duration_secs, Some(207));
        assert!(m.contains("https://music.youtube.com/watch?v=AAAAAAAAAAA"));
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Road/trip: 2026?", "csv"), "Road_trip_ 2026_.csv");
        assert_eq!(file_name("  ", "m3u"), "playlist.m3u");
        assert_eq!(file_name("..", "json"), "playlist.json");
    }
}
