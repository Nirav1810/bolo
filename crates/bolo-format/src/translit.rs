//! Devanagari → Roman Hinglish, the way people type it in chat ("kal milte hain",
//! "mujhe jaldi nikalna hai"), not academic ISO-15919.
//!
//! Engines like Nemotron and Qwen3-ASR write Hindi (and often English loanwords) in
//! Devanagari. Users who want Roman output get it through this pass.
//!
//! Rules:
//! - consonants carry an inherent "a" unless a matra or virama follows;
//! - schwa deletion: word-final schwa is dropped, and medial schwa is dropped in the
//!   V C(a) C V context (चलते → chalte, निकलना → nikalna) unless the next syllable is
//!   the word-final schwa (समझ → samajh);
//! - the word-final long vowels आ/ई/ऊ are written short (देना → dena, जल्दी → jaldi);
//! - anusvara and chandrabindu → "n", nukta forms → q/z/f.
//!
//! English loanwords come out phonetically (मीटिंग → meeting, but ऑफिस → ofis). A
//! loanword lexicon is a later improvement.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Vowel {
    /// Inherent schwa (not written).
    Schwa,
    /// Explicit vowel sign or independent vowel.
    Explicit(&'static str),
    /// Virama: no vowel.
    None,
}

#[derive(Debug)]
struct Unit {
    /// Consonant romanization ("" for an independent vowel).
    cons: &'static str,
    vowel: Vowel,
    nasal: bool,
    visarga: bool,
}

fn consonant(c: char) -> Option<&'static str> {
    Some(match c {
        'क' => "k",
        'ख' => "kh",
        'ग' => "g",
        'घ' => "gh",
        'ङ' => "n",
        'च' => "ch",
        'छ' => "chh",
        'ज' => "j",
        'झ' => "jh",
        'ञ' => "n",
        'ट' => "t",
        'ठ' => "th",
        'ड' => "d",
        'ढ' => "dh",
        'ण' => "n",
        'त' => "t",
        'थ' => "th",
        'द' => "d",
        'ध' => "dh",
        'न' => "n",
        'प' => "p",
        'फ' => "ph",
        'ब' => "b",
        'भ' => "bh",
        'म' => "m",
        'य' => "y",
        'र' => "r",
        'ल' => "l",
        'ळ' => "l",
        'व' => "v",
        'श' => "sh",
        'ष' => "sh",
        'स' => "s",
        'ह' => "h",
        // precomposed nukta forms (U+0958..U+095F); decomposed ones go through `with_nukta`
        '\u{0958}' => "q",
        '\u{0959}' => "kh",
        '\u{095A}' => "gh",
        '\u{095B}' => "z",
        '\u{095C}' => "d",
        '\u{095D}' => "dh",
        '\u{095E}' => "f",
        '\u{095F}' => "y",
        _ => return None,
    })
}

/// Apply a combining nukta (U+093C) to a consonant romanization.
fn with_nukta(cons: &'static str) -> &'static str {
    match cons {
        "k" => "q",
        "j" => "z",
        "ph" => "f",
        other => other,
    }
}

fn independent_vowel(c: char) -> Option<&'static str> {
    Some(match c {
        'अ' => "a",
        'आ' => "aa",
        'इ' => "i",
        'ई' => "ee",
        'उ' => "u",
        'ऊ' => "oo",
        'ऋ' => "ri",
        'ए' => "e",
        'ऐ' => "ai",
        'ओ' => "o",
        'औ' => "au",
        'ऑ' => "o",
        'ऍ' => "e",
        _ => return None,
    })
}

fn matra(c: char) -> Option<&'static str> {
    Some(match c {
        'ा' => "aa",
        'ि' => "i",
        'ी' => "ee",
        'ु' => "u",
        'ू' => "oo",
        'ृ' => "ri",
        'े' => "e",
        'ै' => "ai",
        'ो' => "o",
        'ौ' => "au",
        'ॉ' => "o",
        'ॅ' => "e",
        _ => return None,
    })
}

fn is_devanagari_letter(c: char) -> bool {
    ('\u{0900}'..='\u{0963}').contains(&c) || ('\u{0971}'..='\u{097F}').contains(&c)
}

/// True if `text` contains any Devanagari letters.
pub fn has_devanagari(text: &str) -> bool {
    text.chars().any(|c| ('\u{0900}'..='\u{097F}').contains(&c))
}

/// Transliterate all Devanagari words in `text`; everything else passes through.
pub fn devanagari_to_roman(text: &str) -> String {
    devanagari_to_roman_with(text, |_| None)
}

/// Like [`devanagari_to_roman`], but `replace` may supply a whole-word replacement for a
/// Devanagari word (used to restore English loanword spellings).
pub fn devanagari_to_roman_with(text: &str, replace: impl Fn(&str) -> Option<String>) -> String {
    let word_to_roman = |w: &str| replace(w).unwrap_or_else(|| word_to_roman(w));
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for c in text.chars() {
        if is_devanagari_letter(c) {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push_str(&word_to_roman(&word));
            word.clear();
        }
        match c {
            '।' | '॥' => out.push('.'),
            '०'..='९' => out.push(char::from(b'0' + (c as u32 - '०' as u32) as u8)),
            _ => out.push(c),
        }
    }
    if !word.is_empty() {
        out.push_str(&word_to_roman(&word));
    }
    out
}

fn parse(word: &str) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    for c in word.chars() {
        if let Some(cons) = consonant(c) {
            units.push(Unit { cons, vowel: Vowel::Schwa, nasal: false, visarga: false });
        } else if let Some(v) = independent_vowel(c) {
            units.push(Unit { cons: "", vowel: Vowel::Explicit(v), nasal: false, visarga: false });
        } else if let Some(u) = units.last_mut() {
            match c {
                '\u{093C}' => u.cons = with_nukta(u.cons),
                '्' => u.vowel = Vowel::None,
                'ं' | 'ँ' => u.nasal = true,
                'ः' => u.visarga = true,
                _ => {
                    if let Some(m) = matra(c) {
                        u.vowel = Vowel::Explicit(m);
                    }
                }
            }
        }
    }
    units
}

/// Decide which inherent schwas are pronounced. Returns `(keep_schwa, has_vowel)` per
/// unit.
fn schwa_analysis(units: &[Unit]) -> (Vec<bool>, Vec<bool>) {
    let n = units.len();
    // Decide which inherent schwas are pronounced. Hindi schwa syncope applies right to
    // left: निकलना drops ल's schwa first, which then protects क's (nikalna, not niklana).
    let mut has_vowel: Vec<bool> = units.iter().map(|u| u.vowel != Vowel::None).collect();
    let mut keep_schwa = vec![true; n];
    for i in (0..n).rev() {
        if units[i].vowel != Vowel::Schwa || units[i].nasal {
            continue;
        }
        let is_last = i + 1 == n;
        if is_last {
            // Single-consonant words keep their vowel ("na" = न as a particle).
            if n > 1 {
                keep_schwa[i] = false;
                has_vowel[i] = false;
            }
            continue;
        }
        let prev_vowel = i > 0 && has_vowel[i - 1];
        let next = &units[i + 1];
        let next_has_vowel = has_vowel[i + 1];
        let next_is_final_schwa = i + 2 == n && next.vowel == Vowel::Schwa && !next.nasal;
        if prev_vowel && next_has_vowel && !next_is_final_schwa && !next.cons.is_empty() {
            keep_schwa[i] = false;
            has_vowel[i] = false;
        }
    }
    (keep_schwa, has_vowel)
}

/// Coarse phonetic key of one Devanagari word, in the alphabet used by
/// `scripts/build_loanword_lexicon.py`: consonants `p b t d k g c j f w s h m n l r y`
/// (aspiration, dental/retroflex, s/sh, v/w, z/j merged), vowels `A E I O U`.
pub(crate) fn phonetic_key(word: &str) -> String {
    let units = parse(word);
    let (keep_schwa, _) = schwa_analysis(&units);
    let mut k = String::new();
    for (i, u) in units.iter().enumerate() {
        k.push_str(match u.cons {
            "k" | "kh" | "q" => "k",
            "g" | "gh" => "g",
            "ch" | "chh" => "c",
            "j" | "jh" | "z" => "j",
            "t" | "th" => "t",
            "d" | "dh" => "d",
            "p" => "p",
            "ph" | "f" => "f",
            "b" | "bh" => "b",
            "v" => "w",
            "s" | "sh" => "s",
            other => other, // n m y r l h, "" for independent vowels
        });
        let v = match u.vowel {
            Vowel::Schwa if keep_schwa[i] => "A",
            Vowel::Explicit("a" | "aa") => "A",
            Vowel::Explicit("i" | "ee") => "I",
            Vowel::Explicit("u" | "oo") => "U",
            Vowel::Explicit("e" | "ai") => "E",
            Vowel::Explicit("o" | "au") => "O",
            Vowel::Explicit("ri") => "rI",
            _ => "",
        };
        k.push_str(v);
        if u.nasal {
            k.push('n');
        }
        if u.visarga {
            k.push('h');
        }
    }
    k
}

fn word_to_roman(word: &str) -> String {
    let units = parse(word);
    let n = units.len();
    let (keep_schwa, has_vowel) = schwa_analysis(&units);

    // Index of the last unit that carries a written vowel (for final-vowel shortening).
    let last_vowel_idx = (0..n)
        .rev()
        .find(|&i| matches!(units[i].vowel, Vowel::Explicit(_)) || keep_schwa[i] && units[i].vowel == Vowel::Schwa);

    let mut s = String::new();
    for (i, u) in units.iter().enumerate() {
        // व: "w" in wala/wo/wait/WhatsApp, "v" before i-sounds (video, vishay).
        let cons = match (u.cons, u.vowel) {
            ("v", Vowel::Explicit("i" | "ee" | "ri")) => "v",
            ("v", _) => "w",
            (c, _) => c,
        };
        s.push_str(cons);
        match u.vowel {
            Vowel::Schwa if keep_schwa[i] => s.push('a'),
            Vowel::Explicit(v) => {
                let trailing_only_nasal = units[i + 1..].is_empty();
                // Shorten the final long vowel (देना → dena, ही → hi), but keep a lone
                // independent vowel as written (आ → aa).
                let v = if Some(i) == last_vowel_idx && (i > 0 || !u.cons.is_empty()) && trailing_only_nasal {
                    match v {
                        "aa" if !u.nasal => "a",
                        "ee" => "i",
                        "oo" if !u.nasal => "u",
                        other => other,
                    }
                } else if v == "aa" && !u.nasal && i + 1 < n && has_vowel[i + 1] && !units[i + 1].cons.is_empty() {
                    // Open syllable: typists write one "a" (वाला → wala, ज़्यादा → zyada,
                    // खाना → khana) but keep "aa" when closed (बात → baat, आज → aaj).
                    "a"
                } else {
                    v
                };
                s.push_str(v);
            }
            _ => {}
        }
        if u.nasal {
            s.push('n');
        }
        if u.visarga {
            s.push('h');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(d: &str) -> String {
        devanagari_to_roman(d)
    }

    #[test]
    fn common_hinglish_words() {
        let cases = [
            ("कल", "kal"),
            ("मिलते", "milte"),
            ("चलते", "chalte"),
            ("हैं", "hain"),
            ("है", "hai"),
            ("ठीक", "theek"),
            ("मुझे", "mujhe"),
            ("आज", "aaj"),
            ("जल्दी", "jaldi"),
            ("निकलना", "nikalna"),
            ("देना", "dena"),
            ("भेज", "bhej"),
            ("दिया", "diya"),
            ("क्या", "kya"),
            ("नहीं", "nahin"),
            ("समझ", "samajh"),
            ("मीटिंग", "meeting"),
            ("ज़्यादा", "zyada"),
            ("फ़ोन", "fon"),
            ("मैं", "main"),
            ("ना", "na"),
            ("ही", "hi"),
            ("आ", "aa"),
            ("वाला", "wala"),
            ("खाना", "khana"),
            ("बात", "baat"),
            ("वो", "wo"),
            ("वीडियो", "veediyo"),
            ("अपॉइंटमेंट", "apointment"),
            ("न", "na"),
        ];
        for (dev, roman) in cases {
            assert_eq!(t(dev), roman, "{dev}");
        }
    }

    #[test]
    fn phonetic_keys() {
        assert_eq!(phonetic_key("ऑफिस"), "OfIs");
        assert_eq!(phonetic_key("डॉक्टर"), "dOktAr");
        assert_eq!(phonetic_key("रिपोर्ट"), "rIpOrt");
        assert_eq!(phonetic_key("सर्वर"), "sArwAr");
        assert_eq!(phonetic_key("फाइनल"), "fAInAl");
        assert_eq!(phonetic_key("मीटिंग"), "mItIng");
    }

    #[test]
    fn sentences_and_mixed_script() {
        assert_eq!(t("हाँ, ठीक है, कल मिलते हैं।"), "haan, theek hai, kal milte hain.");
        assert_eq!(t("कल meeting है ना"), "kal meeting hai na");
        assert_eq!(t("१२३"), "123");
        assert!(has_devanagari("कल meeting"));
        assert!(!has_devanagari("kal meeting"));
    }
}
