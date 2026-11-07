//! Tidies YouTube titles: "Artist - Title (Official Video)" -> ("Title", "Artist").

const NOISE: &[&str] = &[
    "official", "video", "audio", "lyric", "lyrics", "visualizer", "visualiser", "hd", "4k",
    "mv", "m/v", "music video", "remaster", "remastered", "explicit", "full song", "hq",
    "topic",
];

fn is_noise(inner: &str) -> bool {
    let l = inner.to_lowercase();
    NOISE.iter().any(|n| l.contains(n))
}

/// Removes trailing "(...)" / "[...]" groups that are only video noise.
fn strip_noise_groups(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[i] == b'(' { ')' } else { ']' };
        match rest[i..].find(close) {
            Some(j) => {
                let inner = &rest[i + 1..i + j];
                out.push_str(&rest[..i]);
                if !is_noise(inner) {
                    out.push_str(&rest[i..=i + j]);
                }
                rest = &rest[i + j + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_uploader(u: &str) -> String {
    let mut u = u.trim().to_string();
    for suf in [" - Topic", "VEVO", "Vevo", " Official"] {
        if let Some(s) = u.strip_suffix(suf) {
            u = s.trim().to_string();
        }
    }
    u
}

/// Returns `(title, artist)`. `uploader` is the channel name; used when the
/// title carries no "Artist - " prefix.
pub fn tidy(raw_title: &str, uploader: &str) -> (String, String) {
    let t = strip_noise_groups(raw_title);
    for sep in [" - ", " – ", " — ", " | "] {
        if let Some((a, b)) = t.split_once(sep) {
            let (a, b) = (a.trim(), b.trim());
            if !a.is_empty() && !b.is_empty() {
                return (b.to_string(), a.to_string());
            }
        }
    }
    (t, clean_uploader(uploader))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_dash_title() {
        assert_eq!(
            tidy("Daft Punk - Get Lucky (Official Audio)", "Daft Punk"),
            ("Get Lucky".into(), "Daft Punk".into())
        );
    }

    #[test]
    fn keeps_meaningful_brackets() {
        assert_eq!(
            tidy("Nirvana - Come as You Are (Live at Reading) [Official Video HD]", "x").0,
            "Come as You Are (Live at Reading)"
        );
    }

    #[test]
    fn falls_back_to_uploader() {
        assert_eq!(
            tidy("Blinding Lights", "The Weeknd - Topic"),
            ("Blinding Lights".into(), "The Weeknd".into())
        );
        assert_eq!(tidy("Hello", "AdeleVEVO").1, "Adele");
    }

    #[test]
    fn unbalanced_bracket_is_left_alone() {
        assert_eq!(tidy("Song (unfinished", "A").0, "Song (unfinished");
    }
}
