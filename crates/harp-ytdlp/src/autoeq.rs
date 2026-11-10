//! Headphone correction from AutoEq (https://github.com/jaakkopasanen/AutoEq,
//! MIT): measured equalizer settings for thousands of headphones and earbuds,
//! each a short list of filters that bring that model to a neutral sound.
//! No account or key: the index and the profiles are plain files on GitHub.

use crate::{Error, Result};
use std::path::Path;
use std::time::Duration;

const BASE: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results";

/// One measured profile ("Sony WH-1000XM4, by oratory1990").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// Who measured it (and on what rig).
    pub source: String,
    /// Path under `results/`, already URL-encoded.
    pub path: String,
}

/// The kind of filter: a peak, or a low / high shelf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub kind: Kind,
    pub freq: f64,
    pub gain_db: f64,
    pub q: f64,
}

/// A profile's correction: turn the volume down by `preamp_db` (negative), then the filters.
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    pub preamp_db: f64,
    pub filters: Vec<Filter>,
}

/// Reads the index (`results/INDEX.md`): `- [Name](./path) by Source on Rig`.
pub fn parse_index(md: &str) -> Vec<Profile> {
    md.lines()
        .filter_map(|l| {
            let l = l.strip_prefix("- [")?;
            let (name, rest) = l.split_once("](")?;
            let (path, rest) = rest.split_once(')')?;
            let source = rest.trim().strip_prefix("by ").unwrap_or(rest.trim()).to_string();
            Some(Profile { name: name.to_string(), source, path: path.trim_start_matches("./").to_string() })
        })
        .collect()
}

/// Profiles whose name has every word of `query` (any case), best matches first.
pub fn search(all: &[Profile], query: &str, limit: usize) -> Vec<Profile> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return vec![];
    }
    let mut hits: Vec<(usize, &Profile)> = all
        .iter()
        .filter(|p| {
            let n = p.name.to_lowercase();
            words.iter().all(|w| n.contains(w.as_str()))
        })
        .map(|p| {
            // Shorter names that start with the query rank first.
            let n = p.name.to_lowercase();
            let starts = if n.starts_with(&words[0]) { 0 } else { 1000 };
            (starts + p.name.len(), p)
        })
        .collect();
    hits.sort_by_key(|(k, _)| *k);
    hits.into_iter().take(limit).map(|(_, p)| p.clone()).collect()
}

/// Reads a `… ParametricEQ.txt` file.
pub fn parse_parametric(text: &str) -> Correction {
    let mut c = Correction { preamp_db: 0.0, filters: vec![] };
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.first() == Some(&"Preamp:") {
            c.preamp_db = t.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            continue;
        }
        // Filter 1: ON PK Fc 105 Hz Gain 2.6 dB Q 0.70
        if t.first() != Some(&"Filter") || t.get(2) != Some(&"ON") {
            continue;
        }
        let kind = match t.get(3) {
            Some(&"PK") | Some(&"PEQ") => Kind::Peak,
            Some(&"LSC") | Some(&"LS") => Kind::LowShelf,
            Some(&"HSC") | Some(&"HS") => Kind::HighShelf,
            _ => continue,
        };
        let after = |key: &str| t.iter().position(|x| *x == key).and_then(|i| t.get(i + 1)).and_then(|v| v.parse::<f64>().ok());
        if let (Some(freq), Some(gain_db)) = (after("Fc"), after("Gain")) {
            c.filters.push(Filter { kind, freq, gain_db, q: after("Q").unwrap_or(0.7) });
        }
    }
    c
}

fn get(url: &str) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into();
    let mut r = agent.get(url).header("User-Agent", "Harp (music player)").call().map_err(|e| Error::Download(e.to_string()))?;
    r.body_mut().with_config().limit(8 * 1024 * 1024).read_to_string().map_err(|e| Error::Download(e.to_string()))
}

/// The index, kept in `cache` for a week (it's ~850 KB).
pub fn index(cache: &Path) -> Result<Vec<Profile>> {
    let file = cache.join("autoeq_index.md");
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(7 * 86400));
    let text = if fresh {
        std::fs::read_to_string(&file).map_err(|e| Error::Download(e.to_string()))?
    } else {
        match get(&format!("{BASE}/INDEX.md")) {
            Ok(t) => {
                let _ = std::fs::create_dir_all(cache);
                let _ = std::fs::write(&file, &t);
                t
            }
            // Offline: an older copy is better than nothing.
            Err(e) => std::fs::read_to_string(&file).map_err(|_| e)?,
        }
    };
    Ok(parse_index(&text))
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'(' | b')' | b'&' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// Downloads one profile's correction.
pub fn load(p: &Profile) -> Result<Correction> {
    let url = format!("{BASE}/{}/{}%20ParametricEQ.txt", p.path, enc(&p.name));
    let c = parse_parametric(&get(&url)?);
    if c.filters.is_empty() {
        return Err(Error::Parse("that profile has no filters".into()));
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_search() {
        let all = parse_index(include_str!("../tests/fixtures/autoeq_index.md"));
        assert!(all.len() >= 5);
        let a = &all.iter().find(|p| p.source.starts_with("crinacle")).unwrap();
        assert_eq!(a.name, "Apple AirPods Pro");
        assert_eq!(a.path, "crinacle/711%20in-ear/Apple%20AirPods%20Pro");
        assert_eq!(a.source, "crinacle on 711");
        let hits = search(&all, "airpods pro 2", 10);
        assert!(hits.iter().all(|p| p.name.to_lowercase().contains("pro 2")), "{hits:?}");
        assert!(search(&all, "airpods", 3).len() == 3);
        assert!(search(&all, "   ", 3).is_empty());
    }

    #[test]
    fn parametric_file() {
        let c = parse_parametric(include_str!("../tests/fixtures/autoeq_airpods_pro.txt"));
        assert_eq!(c.preamp_db, -6.0);
        assert_eq!(c.filters.len(), 10);
        assert_eq!(c.filters[0], Filter { kind: Kind::LowShelf, freq: 105.0, gain_db: 2.6, q: 0.7 });
        assert_eq!(c.filters[1].kind, Kind::Peak);
        assert_eq!(c.filters[5].kind, Kind::HighShelf);
        assert!(parse_parametric("Filter 1: OFF PK Fc 100 Hz Gain 3 dB Q 1").filters.is_empty());
    }

    /// `cargo test -p harp-ytdlp -- --ignored live_autoeq`
    #[test]
    #[ignore]
    fn live_autoeq() {
        let dir = std::env::temp_dir().join("harp-autoeq-test");
        let all = index(&dir).unwrap();
        assert!(all.len() > 1000);
        let p = search(&all, "Sony WH-1000XM4", 1).pop().unwrap();
        let c = load(&p).unwrap();
        eprintln!("{} by {}: preamp {} dB, {} filters", p.name, p.source, c.preamp_db, c.filters.len());
        assert!(!c.filters.is_empty());
    }
}
