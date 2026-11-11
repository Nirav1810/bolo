//! Other Indic scripts → Devanagari, so Hindi that an engine wrote in Bengali, Gurmukhi,
//! Gujarati, Oriya, Tamil, Telugu, Kannada or Malayalam script goes through the same
//! Devanagari → Roman romanizer and loanword lexicon as everything else.
//!
//! Deepgram `multi` sometimes picks a third script for a Hinglish dictation (hinglish/09
//! came back entirely in Telugu). Unicode lays these eight blocks out like Devanagari:
//! the same letter sits at the same offset from the start of its 128-codepoint block. So
//! a letter maps onto Devanagari by that offset, when the Devanagari codepoint at the
//! offset is one the romanizer reads. Letters that sit at an offset with no or a
//! different Devanagari meaning (Dravidian short e and o, Tamil ழ, Telugu ౘ, Malayalam
//! chillu letters, Bengali ৎ, Gurmukhi tippi and addak) come from a small explicit
//! table. Decomposed vowel signs (Tamil ெ + ா for ொ) are folded back into one sign.
//!
//! Every character in the eight blocks comes out as Devanagari or a digit, or is dropped.
//! None survive, so Roman-script output never mixes scripts.

/// First codepoint of each script block this module maps. Each block is 0x80 long.
const BLOCKS: [u32; 8] = [
    0x0980, // Bengali (and Assamese)
    0x0A00, // Gurmukhi
    0x0A80, // Gujarati
    0x0B00, // Oriya
    0x0B80, // Tamil
    0x0C00, // Telugu
    0x0C80, // Kannada
    0x0D00, // Malayalam
];

const DEVANAGARI: u32 = 0x0900;
const VIRAMA: char = '्';

/// The block start of `c`, if it is in one of the mapped scripts.
fn block_of(c: char) -> Option<u32> {
    let cp = c as u32;
    BLOCKS.iter().copied().find(|&b| (b..b + 0x80).contains(&cp))
}

/// True if `text` contains any character from the Bengali, Gurmukhi, Gujarati, Oriya,
/// Tamil, Telugu, Kannada or Malayalam blocks.
pub fn has_brahmic(text: &str) -> bool {
    text.chars().any(|c| block_of(c).is_some())
}

/// What one source character becomes.
enum Mapped {
    /// Devanagari text to emit.
    Text(&'static str),
    /// One Devanagari character.
    Char(char),
    /// Gurmukhi addak: double the next consonant.
    Geminate,
    /// Telugu / Kannada length mark: lengthen the vowel sign before it (ి + ౕ = ी).
    Lengthen,
    /// Nothing (signs with no reading).
    Drop,
}

/// Letters whose offset lands on the wrong (or no) Devanagari letter.
fn explicit(c: char) -> Option<Mapped> {
    use Mapped::{Geminate, Lengthen, Text};
    Some(match c {
        // Bengali / Assamese
        'ৎ' => Text("त्"),
        'ৰ' => Text("र"),
        'ৱ' => Text("व"),
        // Gurmukhi
        'ੰ' => Text("ं"),
        'ੱ' => Geminate,
        // vowel bearers: the vowel sign that follows gives the vowel
        'ੲ' | 'ੳ' => Text("अ"),
        'ੵ' => Text("्य"),
        // Oriya
        'ୱ' => Text("व"),
        // short e / short o (Tamil, Telugu, Kannada, Malayalam): written as e / o
        'எ' | 'ఎ' | 'ಎ' | 'എ' => Text("ए"),
        'ஒ' | 'ఒ' | 'ಒ' | 'ഒ' => Text("ओ"),
        'ெ' | 'ె' | 'ೆ' | 'െ' => Text("े"),
        'ொ' | 'ొ' | 'ೊ' | 'ൊ' => Text("ो"),
        // Dravidian r, l, n and other letters with no plain Devanagari letter at the
        // offset
        'ற' | 'ఱ' | 'ಱ' | 'റ' | 'ౚ' => Text("र"),
        'ழ' | 'ఴ' | 'ೞ' | 'ഴ' => Text("ळ"),
        'ன' | 'ഩ' => Text("न"),
        'ഺ' => Text("ट"),
        'ౘ' => Text("च"),
        'ౙ' => Text("ज"),
        // consonants with no vowel: nakaara pollu and the Malayalam chillu letters
        'ౝ' | 'ೝ' | 'ൻ' => Text("न्"),
        'ൺ' => Text("ण्"),
        'ർ' => Text("र्"),
        'ൽ' => Text("ल्"),
        'ൾ' | 'ൖ' => Text("ळ्"),
        'ൿ' => Text("क्"),
        'ൔ' => Text("म्"),
        'ൕ' => Text("य्"),
        // Malayalam dot reph: r before the next consonant
        'ൎ' => Text("र्"),
        // Malayalam vertical-bar and circular viramas
        '\u{0D3B}' | '\u{0D3C}' => Text("्"),
        // Length marks. Precomposed vowel signs decompose into a short-e sign plus one
        // of these; `push_sign` folds them back together (ె + ౖ = ै, ெ + ௗ = ौ).
        'ౖ' | 'ೖ' | 'ୖ' => Text("ै"),
        'ௗ' | 'ୗ' | 'ৗ' | 'ൗ' => Text("ौ"),
        'ౕ' | 'ೕ' => Lengthen,
        _ => return None,
    })
}

/// Blocks whose letters at the Devanagari nukta-letter offsets (U+0958..U+095F) are the
/// same nukta letters (ড়, ਖ਼, ଡ଼). In the other scripts those offsets hold different
/// letters or fraction signs.
const NUKTA_LETTER_BLOCKS: [u32; 3] = [0x0980, 0x0A00, 0x0B00];

/// Map one character from a Brahmic block. `None` means it isn't in one.
fn map_char(c: char) -> Option<Mapped> {
    let block = block_of(c)?;
    if let Some(m) = explicit(c) {
        return Some(m);
    }
    let offset = c as u32 - block;
    let same_letter = match offset {
        0x01..=0x57 | 0x64..=0x6F => true,
        0x58..=0x5F => NUKTA_LETTER_BLOCKS.contains(&block),
        _ => false,
    };
    let dev = char::from_u32(DEVANAGARI + offset).filter(|&d| same_letter && crate::translit::romanizes(d));
    Some(dev.map_or(Mapped::Drop, Mapped::Char))
}

/// Push a mapped character, folding a decomposed vowel sign into the one before it:
/// े + ा = ो (Tamil ெ + ா, Bengali ে + া), े + ै = ै, े + ौ = ौ, े + ू = ो (Kannada).
fn push_sign(out: &mut String, d: char) {
    let folded = match (out.chars().last(), d) {
        (Some('े'), 'ा' | 'ू') => Some('ो'),
        (Some('े'), 'ै' | 'ौ') => Some(d),
        _ => None,
    };
    match folded {
        Some(f) => {
            out.pop();
            out.push(f);
        }
        None => out.push(d),
    }
}

/// Map every Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada and Malayalam
/// character in `text` to Devanagari; everything else passes through.
pub fn brahmic_to_devanagari(text: &str) -> String {
    if !has_brahmic(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut geminate = false;
    let mut buf = [0u8; 4];
    for c in text.chars() {
        let mapped = match map_char(c) {
            None => {
                geminate = false;
                out.push(c);
                continue;
            }
            Some(Mapped::Geminate) => {
                geminate = true;
                continue;
            }
            Some(Mapped::Drop) => continue,
            Some(Mapped::Lengthen) => {
                let long = match out.chars().last() {
                    Some('ि') => 'ी',
                    Some('ु') => 'ू',
                    _ => continue,
                };
                out.pop();
                out.push(long);
                continue;
            }
            Some(Mapped::Text(s)) => s,
            Some(Mapped::Char(d)) => &*d.encode_utf8(&mut buf),
        };
        let mut chars = mapped.chars();
        let Some(first) = chars.next() else { continue };
        // An addak doubles the consonant after it.
        if std::mem::take(&mut geminate) && crate::translit::is_consonant(first) {
            out.push(first);
            out.push(VIRAMA);
        }
        push_sign(&mut out, first);
        out.extend(chars);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(text: &str) -> String {
        crate::translit::devanagari_to_roman(text)
    }

    #[test]
    fn detects_each_block() {
        for s in ["ঘর", "ਘਰ", "ઘર", "ଘର", "வீடு", "ఇల్లు", "ಮನೆ", "വീട്"]
        {
            assert!(has_brahmic(s), "{s}");
        }
        assert!(!has_brahmic("घर ghar گھر"));
    }

    #[test]
    fn offset_letters_map_to_devanagari() {
        assert_eq!(brahmic_to_devanagari("ঘর"), "घर");
        assert_eq!(brahmic_to_devanagari("ਘਰ"), "घर");
        assert_eq!(brahmic_to_devanagari("ઘર"), "घर");
        assert_eq!(brahmic_to_devanagari("ଘର"), "घर");
        assert_eq!(brahmic_to_devanagari("ఘర"), "घर");
        assert_eq!(brahmic_to_devanagari("ಘರ"), "घर");
        assert_eq!(brahmic_to_devanagari("ഘര"), "घर");
        assert_eq!(brahmic_to_devanagari("kal ఘర్ ১২"), "kal घर् १२");
    }

    #[test]
    fn explicit_letters() {
        // Dravidian short e and o read as e and o
        assert_eq!(brahmic_to_devanagari("బ్రెడ్"), "ब्रेड्");
        assert_eq!(brahmic_to_devanagari("కొ"), "को");
        // Gurmukhi addak doubles the next consonant; tippi nasalizes
        assert_eq!(r(&brahmic_to_devanagari("ਪੱਕਾ")), "pakka");
        assert_eq!(r(&brahmic_to_devanagari("ਮੁੰਡਾ")), "munda");
        // Malayalam chillu n
        assert_eq!(brahmic_to_devanagari("അവൻ"), "अवन्");
        // Bengali khanda ta
        assert_eq!(brahmic_to_devanagari("হঠাৎ"), "हठात्");
        // Telugu letters at the nukta-letter offsets are not nukta letters
        assert_eq!(brahmic_to_devanagari("ౘౙౚ"), "चजर");
        assert_eq!(brahmic_to_devanagari("ౝ ೝ"), "न् न्");
        // Malayalam fraction signs at the same offsets are dropped, not read as letters
        assert_eq!(brahmic_to_devanagari("ക\u{0D58}\u{0D5E}"), "क");
        // Kannada ಱ
        assert_eq!(brahmic_to_devanagari("ಱ"), "र");
    }

    #[test]
    fn decomposed_vowel_signs_fold_back() {
        // Tamil ொ = ெ + ா, ௌ = ெ + ௗ
        assert_eq!(brahmic_to_devanagari("க\u{0BC6}\u{0BBE}"), "को");
        assert_eq!(brahmic_to_devanagari("க\u{0BC6}\u{0BD7}"), "कौ");
        // Telugu ై = ె + ౖ
        assert_eq!(brahmic_to_devanagari("క\u{0C46}\u{0C56}"), "कै");
        // Kannada ೀ = ಿ + ೕ, ೋ = ೆ + ೂ + ೕ
        assert_eq!(brahmic_to_devanagari("ಕ\u{0CBF}\u{0CD5}"), "की");
        assert_eq!(brahmic_to_devanagari("ಕ\u{0CC6}\u{0CC2}\u{0CD5}"), "को");
        // Bengali ো = ে + া, Oriya ୈ = େ + ୖ
        assert_eq!(brahmic_to_devanagari("ক\u{09C7}\u{09BE}"), "को");
        assert_eq!(brahmic_to_devanagari("କ\u{0B47}\u{0B56}"), "कै");
        // Malayalam ൌ written as ൗ alone after a consonant
        assert_eq!(brahmic_to_devanagari("ക\u{0D57}"), "कौ");
        // precomposed forms are unchanged
        assert_eq!(brahmic_to_devanagari("కై கொ"), "कै को");
    }
}
