//! Other Indic scripts → Devanagari, so Hindi that an engine wrote in Bengali, Gurmukhi,
//! Gujarati, Oriya, Tamil, Telugu, Kannada or Malayalam script goes through the same
//! Devanagari → Roman romanizer and loanword lexicon as everything else.
//!
//! Deepgram `multi` sometimes picks a third script for a Hinglish dictation (hinglish/09
//! came back entirely in Telugu). Unicode lays these eight blocks out like Devanagari:
//! the same letter sits at the same offset from the start of its 128-codepoint block. So
//! a letter maps onto Devanagari by that offset, when the Devanagari codepoint at the
//! offset is one the romanizer reads. Letters that sit at an offset with no or a
//! different Devanagari meaning (Dravidian short e and o, Tamil ழ, Malayalam chillu
//! letters, Bengali ৎ, Gurmukhi tippi and addak) come from a small explicit table.
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
    /// Devanagari text to emit (one or more characters).
    Text(&'static str),
    /// A single Devanagari character.
    Char(char),
    /// Gurmukhi addak: double the next consonant.
    Geminate,
    /// Nothing (length marks, signs with no reading).
    Drop,
}

/// Letters whose offset lands on the wrong (or no) Devanagari letter.
fn explicit(c: char) -> Option<Mapped> {
    use Mapped::{Drop, Geminate, Text};
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
        // Dravidian r, l, n variants with no plain Devanagari letter at the offset
        'ற' | 'ఱ' | 'റ' => Text("र"),
        'ழ' | 'ఴ' | 'ೞ' | 'ഴ' => Text("ळ"),
        'ன' | 'ഩ' => Text("न"),
        'ഺ' => Text("ट"),
        // Malayalam chillu letters: a consonant with no vowel
        'ൺ' => Text("ण्"),
        'ൻ' => Text("न्"),
        'ർ' => Text("र्"),
        'ൽ' => Text("ल्"),
        'ൾ' => Text("ळ्"),
        'ൿ' => Text("क्"),
        'ൔ' => Text("म्"),
        'ൕ' => Text("य्"),
        'ൖ' => Text("ळ्"),
        // Malayalam dot reph: r before the next consonant
        'ൎ' => Text("र्"),
        // Malayalam au length mark written alone after a consonant
        'ൗ' => Text("ौ"),
        // ai / au length marks that follow a short-e sign: the precomposed forms are
        // what engines emit, so the stray mark is dropped
        'ౕ' | 'ౖ' | 'ೕ' | 'ೖ' | 'ୖ' | 'ୗ' | 'ௗ' => Drop,
        _ => return None,
    })
}

/// Map one character from a Brahmic block. `None` means it isn't in one.
fn map_char(c: char) -> Option<Mapped> {
    let block = block_of(c)?;
    if let Some(m) = explicit(c) {
        return Some(m);
    }
    let dev = char::from_u32(c as u32 - block + DEVANAGARI).filter(|&d| crate::translit::romanizes(d));
    Some(dev.map_or(Mapped::Drop, Mapped::Char))
}

/// Map every Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada and Malayalam
/// character in `text` to Devanagari; everything else passes through.
pub fn brahmic_to_devanagari(text: &str) -> String {
    if !has_brahmic(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut geminate = false;
    for c in text.chars() {
        let Some(m) = map_char(c) else {
            geminate = false;
            out.push(c);
            continue;
        };
        match m {
            Mapped::Geminate => geminate = true,
            Mapped::Drop => {}
            Mapped::Text(s) => {
                // An addak before a multi-char mapping: double its first letter.
                if std::mem::take(&mut geminate) && s.starts_with(|f: char| crate::translit::is_consonant(f)) {
                    let first = s.chars().next().unwrap_or_default();
                    out.push(first);
                    out.push(VIRAMA);
                }
                out.push_str(s);
            }
            Mapped::Char(d) => {
                if std::mem::take(&mut geminate) && crate::translit::is_consonant(d) {
                    out.push(d);
                    out.push(VIRAMA);
                }
                out.push(d);
            }
        }
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
    }
}
