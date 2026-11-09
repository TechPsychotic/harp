//! Lyrics: parsing LRC (timestamped) text and finding the line to show.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    pub at_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Lyrics {
    /// True when every line has a real timestamp, so it can follow the song.
    pub synced: bool,
    pub lines: Vec<LyricLine>,
    pub source: String,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// "mm:ss.xx", "mm:ss:xx" or "mm:ss" to milliseconds.
fn parse_stamp(s: &str) -> Option<u32> {
    let (m, rest) = s.split_once(':')?;
    let m: u32 = m.trim().parse().ok()?;
    let (sec, frac) = match rest.split_once(['.', ':']) {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let sec: u32 = sec.trim().parse().ok()?;
    if sec >= 60 {
        return None;
    }
    let ms = match frac.trim() {
        "" => 0,
        f => {
            // ".5" is 500 ms, ".05" is 50 ms, ".055" is 55 ms.
            let digits: String = f.chars().take(3).collect();
            let n: u32 = digits.parse().ok()?;
            n * 10u32.pow(3 - digits.len() as u32)
        }
    };
    Some((m * 60 + sec) * 1000 + ms)
}

/// Parses LRC text. A line may carry several timestamps (a repeated chorus).
/// Tags like `[ar:Artist]` and word timings like `<00:12.30>` are ignored.
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            match parse_stamp(&stripped[..end]) {
                Some(ms) => stamps.push(ms),
                None => {
                    // A metadata tag such as [ar:...]: skip the whole line if it has no stamps.
                    if stamps.is_empty() {
                        rest = "";
                    }
                    break;
                }
            }
            rest = stripped[end + 1..].trim_start();
        }
        if stamps.is_empty() {
            continue;
        }
        // Drop enhanced-LRC word timings like <00:12.30>.
        let mut clean = String::new();
        let mut in_tag = false;
        for c in rest.chars() {
            match c {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => clean.push(c),
                _ => {}
            }
        }
        let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
        for at in stamps {
            out.push(LyricLine { at_ms: at, text: clean.clone() });
        }
    }
    out.sort_by_key(|l| l.at_ms);
    out
}

/// Unsynced lyrics: one entry per line, no timing.
pub fn plain_lines(text: &str) -> Vec<LyricLine> {
    text.lines().map(|l| LyricLine { at_ms: 0, text: l.trim().to_string() }).collect()
}

/// The line being sung at `pos_ms`: the last line that has started.
pub fn active_index(lines: &[LyricLine], pos_ms: u32) -> Option<usize> {
    if lines.is_empty() || pos_ms < lines[0].at_ms {
        return None;
    }
    Some(lines.partition_point(|l| l.at_ms <= pos_ms) - 1)
}

/// Builds lyrics from the two forms a service may return, preferring synced.
pub fn from_texts(synced: Option<&str>, plain: Option<&str>, source: &str) -> Lyrics {
    if let Some(s) = synced {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Lyrics { synced: true, lines, source: source.into() };
        }
    }
    match plain {
        Some(p) if !p.trim().is_empty() => Lyrics { synced: false, lines: plain_lines(p), source: source.into() },
        _ => Lyrics::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stamps_in_all_common_forms() {
        assert_eq!(parse_stamp("00:30.64"), Some(30_640));
        assert_eq!(parse_stamp("01:02:50"), Some(62_500));
        assert_eq!(parse_stamp("2:05"), Some(125_000));
        assert_eq!(parse_stamp("00:01.5"), Some(1_500));
        assert_eq!(parse_stamp("00:01.055"), Some(1_055));
        assert_eq!(parse_stamp("ar:Daft Punk"), None);
        assert_eq!(parse_stamp("00:75.00"), None);
    }

    #[test]
    fn parses_lrc_lines_tags_repeats_and_word_timings() {
        let lrc = "[ar:Daft Punk]\n[ti:Get Lucky]\n[00:30.64] Like the legend\n[01:00.00][02:00.00]Chorus line\n[00:45.10]<00:45.10>Our <00:45.60>ends\nnot a lyric line\n";
        let l = parse_lrc(lrc);
        let texts: Vec<_> = l.iter().map(|x| (x.at_ms, x.text.as_str())).collect();
        assert_eq!(texts, [(30_640, "Like the legend"), (45_100, "Our ends"), (60_000, "Chorus line"), (120_000, "Chorus line")]);
    }

    #[test]
    fn finds_the_active_line() {
        let l = parse_lrc("[00:10.00]a\n[00:20.00]b\n[00:30.00]c");
        assert_eq!(active_index(&l, 5_000), None);
        assert_eq!(active_index(&l, 10_000), Some(0));
        assert_eq!(active_index(&l, 29_999), Some(1));
        assert_eq!(active_index(&l, 999_999), Some(2));
        assert_eq!(active_index(&[], 1), None);
    }

    #[test]
    fn prefers_synced_and_falls_back_to_plain() {
        let a = from_texts(Some("[00:01.00]hi"), Some("hi"), "lrclib");
        assert!(a.synced && a.lines.len() == 1);
        let b = from_texts(Some("garbage"), Some("one\ntwo"), "lrclib");
        assert!(!b.synced && b.lines.len() == 2);
        assert!(from_texts(None, Some("   "), "x").is_empty());
    }
}
