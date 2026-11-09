//! Lyrics in English letters ("Hinglish"): Hindi and other Indian scripts, and Urdu,
//! written the way people type them in chats, not academic transliteration.
//!
//! * Devanagari, Gurmukhi, Bengali, Gujarati, Odia, Tamil, Telugu, Kannada and Malayalam
//!   share one layout (they all descend from ISCII), so one table serves them all.
//! * Hindi drops the inherent "a" at the end of words and in some middles ("कमल" is
//!   "kamal", not "kamala"; "धड़कन" is "dhadkan"). That is done with rules that get
//!   most lyric words right; it is not perfect.
//! * Urdu script usually leaves out short vowels, so its result is rougher.

/// The Indian scripts' blocks; the same offset means the same letter in each.
const INDIC_BLOCKS: &[u32] = &[0x0900, 0x0980, 0x0A00, 0x0A80, 0x0B00, 0x0B80, 0x0C00, 0x0C80, 0x0D00];

fn indic_offset(c: char) -> Option<u32> {
    let u = c as u32;
    INDIC_BLOCKS.iter().find(|&&b| u >= b && u < b + 0x80).map(|b| u - b)
}

fn is_urdu(c: char) -> bool {
    matches!(c as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF)
}

fn consonant(o: u32) -> Option<&'static str> {
    Some(match o {
        0x15 => "k", 0x16 => "kh", 0x17 => "g", 0x18 => "gh", 0x19 => "n",
        0x1A => "ch", 0x1B => "chh", 0x1C => "j", 0x1D => "jh", 0x1E => "n",
        0x1F => "t", 0x20 => "th", 0x21 => "d", 0x22 => "dh", 0x23 => "n",
        0x24 => "t", 0x25 => "th", 0x26 => "d", 0x27 => "dh", 0x28 | 0x29 => "n",
        0x2A => "p", 0x2B => "ph", 0x2C => "b", 0x2D => "bh", 0x2E => "m",
        0x2F => "y", 0x30 | 0x31 => "r", 0x32..=0x34 => "l", 0x35 => "v",
        0x36 | 0x37 => "sh", 0x38 => "s", 0x39 => "h",
        // Letters with a nukta, as single characters.
        0x58 => "q", 0x59 => "kh", 0x5A => "gh", 0x5B => "z", 0x5C => "d", 0x5D => "dh", 0x5E => "f", 0x5F => "y",
        _ => return None,
    })
}

fn independent_vowel(o: u32) -> Option<&'static str> {
    Some(match o {
        0x05 => "a", 0x06 => "aa", 0x07 => "i", 0x08 => "ee", 0x09 => "u", 0x0A => "oo",
        0x0B => "ri", 0x0D | 0x0E | 0x0F => "e", 0x10 => "ai", 0x11 | 0x12 | 0x13 => "o", 0x14 => "au",
        0x60 => "ri", 0x72 => "e",
        _ => return None,
    })
}

fn vowel_sign(o: u32) -> Option<&'static str> {
    Some(match o {
        0x3E => "aa", 0x3F => "i", 0x40 => "ee", 0x41 => "u", 0x42 => "oo", 0x43 | 0x44 => "ri",
        0x45..=0x47 => "e", 0x48 => "ai", 0x49..=0x4B => "o", 0x4C => "au",
        _ => return None,
    })
}

/// One consonant (or a vowel on its own) and what follows it.
#[derive(Debug, Clone, Default)]
struct Seg {
    cons: String,
    vowel: Option<&'static str>,
    /// The consonant still carries its built-in "a".
    inherent: bool,
    nasal: bool,
    tail: &'static str,
    /// Second (or later) letter of a cluster (joined by a virama).
    in_cluster: bool,
    /// Has a virama after it (first letter of a cluster).
    halant: bool,
}

impl Seg {
    fn has_vowel(&self) -> bool {
        self.vowel.is_some() || self.inherent
    }
    fn short_vowel(&self) -> bool {
        self.inherent || matches!(self.vowel, Some("i" | "u" | "ri"))
    }
}

fn nukta(cons: &str) -> &'static str {
    match cons {
        "k" => "q", "kh" => "kh", "g" => "gh", "j" => "z", "d" => "d", "dh" => "dh", "ph" => "f",
        _ => "",
    }
}

/// Hindi's schwa deletion, approximately.
fn drop_schwas(segs: &mut [Seg]) {
    let n = segs.len();
    if n == 0 {
        return;
    }
    // At the end of a word ("kamal"), unless after a cluster ("mitra") or the word is one letter.
    if n > 1 && segs[n - 1].inherent && !segs[n - 1].in_cluster {
        segs[n - 1].inherent = false;
    }
    // In the middle, right to left: V C(a) C V  ->  V C C V, when it would not pile up
    // three consonants and the neighbours make Hindi drop it.
    for i in (1..n.saturating_sub(1)).rev() {
        let (prev, next) = (&segs[i - 1], &segs[i + 1]);
        if !segs[i].inherent || segs[i].cons.is_empty() || segs[i].in_cluster {
            continue;
        }
        if !prev.has_vowel() || prev.nasal || prev.cons.is_empty() && prev.vowel.is_none() {
            continue;
        }
        if next.cons.is_empty() || !next.has_vowel() || next.in_cluster || next.halant {
            continue;
        }
        let verb_ending = i + 1 == n - 1 && matches!(next.cons.as_str(), "n" | "t") && matches!(next.vowel, Some("aa" | "e" | "ee" | "i"));
        let prev_short_not_a = matches!(prev.vowel, Some("i" | "u"));
        if next.short_vowel() || prev_short_not_a || verb_ending {
            segs[i].inherent = false;
        }
    }
}

fn render(segs: &[Seg], out: &mut String) {
    let n = segs.len();
    for (i, s) in segs.iter().enumerate() {
        out.push_str(&s.cons);
        let last = i == n - 1;
        let v = if s.inherent { Some("a") } else { s.vowel };
        if let Some(v) = v {
            // Chat spelling: long vowels at the end of a word are written short.
            let v = match (v, last) {
                ("aa", true) => "a",
                ("ee", true) => "i",
                ("oo", true) => "u",
                (v, _) => v,
            };
            out.push_str(v);
        }
        if s.nasal {
            let next = segs.get(i + 1).map(|x| x.cons.as_str()).unwrap_or("");
            if last && v == Some("e") {
                out.push_str("in"); // "aankhein", "baatein"
            } else if next.starts_with('p') || next.starts_with('b') || next.starts_with('m') {
                out.push('m');
            } else {
                out.push('n');
            }
        }
        out.push_str(s.tail);
    }
}

fn indic_word(chars: &[char], out: &mut String) {
    let mut segs: Vec<Seg> = Vec::new();
    let mut geminate = false;
    for &c in chars {
        let Some(o) = indic_offset(c) else { continue };
        // Gurmukhi's tippi (nasal) and addak (doubles the next consonant).
        if c as u32 == 0x0A70 {
            if let Some(l) = segs.last_mut() {
                l.nasal = true;
            }
            continue;
        }
        if c as u32 == 0x0A71 {
            geminate = true;
            continue;
        }
        if let Some(cons) = consonant(o) {
            let in_cluster = segs.last().is_some_and(|l| l.halant);
            let mut cons = cons.to_string();
            if geminate {
                cons = format!("{}{}", &cons[..1], cons);
                geminate = false;
            }
            segs.push(Seg { cons, inherent: true, in_cluster, ..Default::default() });
        } else if o == 0x3C {
            if let Some(l) = segs.last_mut() {
                let n = nukta(&l.cons);
                if !n.is_empty() {
                    l.cons = n.to_string();
                }
            }
        } else if let Some(v) = vowel_sign(o) {
            if let Some(l) = segs.last_mut() {
                l.vowel = Some(v);
                l.inherent = false;
            }
        } else if o == 0x4D {
            if let Some(l) = segs.last_mut() {
                l.inherent = false;
                l.halant = true;
            }
        } else if let Some(v) = independent_vowel(o) {
            segs.push(Seg { vowel: Some(v), ..Default::default() });
        } else if o == 0x01 || o == 0x02 {
            match segs.last_mut() {
                Some(l) => l.nasal = true,
                None => segs.push(Seg { cons: "n".into(), ..Default::default() }),
            }
        } else if o == 0x03 {
            if let Some(l) = segs.last_mut() {
                l.tail = "h";
            }
        } else if o == 0x50 {
            segs.push(Seg { vowel: Some("om"), ..Default::default() });
        } else if (0x66..=0x6F).contains(&o) {
            drop_schwas(&mut segs);
            render(&segs, out);
            segs.clear();
            out.push(char::from(b'0' + (o - 0x66) as u8));
        } else if o == 0x64 || o == 0x65 {
            drop_schwas(&mut segs);
            render(&segs, out);
            segs.clear();
            out.push('.');
        }
    }
    drop_schwas(&mut segs);
    render(&segs, out);
}

/// Urdu (Arabic script). Short vowels are usually not written, so an "a" is guessed
/// between consonants that have nothing between them.
fn urdu_word(chars: &[char], out: &mut String) {
    let mut res = String::new();
    let mut prev_cons = false;
    let n = chars.len();
    for (i, &c) in chars.iter().enumerate() {
        let first = i == 0;
        let last = i + 1 == n;
        let (piece, is_cons): (&str, bool) = match c {
            'ا' | 'آ' => (if first { "a" } else { "aa" }, false),
            'ب' => ("b", true), 'پ' => ("p", true), 'ت' | 'ٹ' | 'ط' => ("t", true), 'ث' | 'س' | 'ص' => ("s", true),
            'ج' => ("j", true), 'چ' => ("ch", true), 'ح' | 'ہ' | 'ھ' | 'ۃ' | 'ه' => ("h", true), 'خ' => ("kh", true),
            'د' | 'ڈ' => ("d", true), 'ذ' | 'ز' | 'ض' | 'ظ' => ("z", true), 'ر' | 'ڑ' => ("r", true), 'ژ' => ("zh", true),
            'ش' => ("sh", true), 'ع' => ("", false), 'غ' => ("gh", true), 'ف' => ("f", true), 'ق' => ("q", true),
            'ک' | 'ك' => ("k", true), 'گ' => ("g", true), 'ل' => ("l", true), 'م' => ("m", true), 'ن' | 'ں' => ("n", true),
            'و' => (if first { "w" } else if prev_cons && !last { "o" } else { "o" }, first),
            'ی' | 'ي' => (if first { "y" } else if last { "i" } else { "ee" }, first),
            'ے' => ("e", false),
            'َ' => ("a", false), 'ِ' => ("i", false), 'ُ' => ("u", false), 'ْ' | 'ّ' | 'ٔ' | 'ء' => ("", false),
            _ => ("", false),
        };
        if piece.is_empty() {
            continue;
        }
        // Two consonants in a row with no vowel letter between: guess an "a" (not at the end).
        if is_cons && prev_cons && !res.is_empty() && !last {
            res.push('a');
        }
        res.push_str(piece);
        prev_cons = is_cons;
    }
    out.push_str(&res);
}

/// The text in English letters, or `None` when it has no Indian or Urdu script to convert.
pub fn romanize(text: &str) -> Option<String> {
    if !text.chars().any(|c| indic_offset(c).is_some() || is_urdu(c)) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if indic_offset(c).is_some() {
            let start = i;
            while i < chars.len() && (indic_offset(chars[i]).is_some() || chars[i] == '\u{200D}' || chars[i] == '\u{200C}') {
                i += 1;
            }
            indic_word(&chars[start..i], &mut out);
        } else if is_urdu(c) && c.is_alphabetic() || matches!(c, '\u{064B}'..='\u{0652}') {
            let start = i;
            while i < chars.len() && (is_urdu(chars[i]) && (chars[i].is_alphabetic() || matches!(chars[i], '\u{064B}'..='\u{0652}' | '\u{0670}'))) {
                i += 1;
            }
            urdu_word(&chars[start..i], &mut out);
        } else {
            // Urdu punctuation to its Latin look; everything else unchanged.
            out.push(match c {
                '،' => ',',
                '؟' => '?',
                '۔' => '.',
                c => c,
            });
            i += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::romanize;

    fn r(s: &str) -> String {
        romanize(s).unwrap()
    }

    #[test]
    fn common_hindi_lyric_words() {
        let mut wrong = vec![];
        for (hindi, hinglish) in [
            ("तुम ही हो", "tum hi ho"),
            ("दिल", "dil"),
            ("प्यार", "pyaar"),
            ("कमल", "kamal"),
            ("धड़कन", "dhadkan"),
            ("ज़िंदगी", "zindagi"),
            ("हमारा", "hamaara"),
            ("करना", "karna"),
            ("समझना", "samajhna"),
            ("दिलदार", "dildaar"),
            ("बेवफ़ा", "bevafa"), // व is always "v" (chat spelling varies: "bewafa" too)
            ("जीवन", "jeevan"),
            ("चलते", "chalte"),
            ("मोहब्बत", "mohabbat"),
            ("सनम", "sanam"),
            ("मेरी", "meri"),
            ("तेरे", "tere"),
            ("मैं", "main"),
            ("हैं", "hain"),
            ("आँखें", "aankhein"),
            ("बारिश", "baarish"),
            ("मित्र", "mitra"),
            ("अंबर", "ambar"),
        ] {
            let got = r(hindi);
            if got != hinglish {
                wrong.push(format!("{hindi}: got {got}, want {hinglish}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn whole_lines_keep_spaces_and_punctuation() {
        assert_eq!(r("तुम ही हो, अब तुम ही हो।"), "tum hi ho, ab tum hi ho.");
        assert_eq!(r("Love you ज़िंदगी"), "Love you zindagi");
    }

    #[test]
    fn other_indian_scripts_share_the_rules() {
        assert_eq!(r("ਦਿਲ"), "dil", "Punjabi (Gurmukhi)");
        assert_eq!(r("ਪਿਆਰ"), "piaar", "Punjabi writes the vowel separately");
        assert_eq!(r("ভালো"), "bhaalo", "Bengali");
        assert_eq!(r("દિલ"), "dil", "Gujarati");
    }

    #[test]
    fn urdu_is_approximate_but_readable() {
        assert_eq!(r("دل"), "dil".replace("i", ""), "no short vowel written: a bare skeleton");
        assert_eq!(r("پیار"), "peeaar");
        assert!(!r("محبت").is_empty());
    }

    #[test]
    fn latin_text_needs_nothing() {
        assert!(romanize("Tum hi ho").is_none());
        assert!(romanize("").is_none());
    }
}
