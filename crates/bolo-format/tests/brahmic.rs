//! Script fallback for non-Devanagari Indic output (M3 ticket 12): Hinglish written in
//! Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada or Malayalam script comes
//! out as the same Roman Hinglish as its Devanagari spelling, and Roman-script output
//! never contains a Brahmic or Arabic-script character.

use bolo_format::brahmic::{brahmic_to_devanagari, has_brahmic};
use bolo_format::loanwords::to_roman_hinglish;
use bolo_format::translit::needs_romanizing;
use bolo_format::urdu::has_urdu;
use proptest::prelude::*;

/// "Ghar ke liye doodh le aana" in each script, spelled the way that script's writers
/// would, with its Roman Hinglish.
const GOLDEN: &[(&str, &str, &str)] = &[
    ("devanagari", "घर के लिए दूध ले आना", "ghar ke lie doodh le ana"),
    ("bengali", "ঘর কে লিএ দূধ লে আনা", "ghar ke lie doodh le ana"),
    ("gurmukhi", "ਘਰ ਕੇ ਲਿਏ ਦੂਧ ਲੇ ਆਨਾ", "ghar ke lie doodh le ana"),
    ("gujarati", "ઘર કે લિએ દૂધ લે આના", "ghar ke lie doodh le ana"),
    ("oriya", "ଘର କେ ଲିଏ ଦୂଧ ଲେ ଆନା", "ghar ke lie doodh le ana"),
    // Tamil has no voiced or aspirated stops, so the letters say "kar", "tooth".
    ("tamil", "கர் கே லியே தூத் லே ஆனா", "kar ke liye tooth le ana"),
    ("telugu", "ఘర్ కే లియే దూధ్ లే ఆనా", "ghar ke liye doodh le ana"),
    ("kannada", "ಘರ್ ಕೇ ಲಿಯೇ ದೂಧ್ ಲೇ ಆನಾ", "ghar ke liye doodh le ana"),
    ("malayalam", "ഘർ കേ ലിയേ ദൂധ് ലേ ആനാ", "ghar ke liye doodh le ana"),
];

#[test]
fn every_script_romanizes() {
    for (script, text, roman) in GOLDEN {
        assert!(needs_romanizing(text), "{script}");
        assert_eq!(to_roman_hinglish(text), *roman, "{script}: {text}");
    }
}

#[test]
fn loanwords_are_restored_through_other_scripts() {
    assert_eq!(to_roman_hinglish("మీటింగ్ కల్ హై"), "meeting kal hai");
    assert_eq!(to_roman_hinglish("ਮੀਟਿੰਗ ਹੈ"), "meeting hai");
    assert_eq!(to_roman_hinglish("ઓફિસ"), "office");
    // Gurmukhi addak doubles the next consonant
    assert_eq!(to_roman_hinglish("ਪੱਕਾ"), "pakka");
}

#[test]
fn mixed_scripts_and_digits() {
    assert_eq!(to_roman_hinglish("kal ఘర్ ১২ बजे"), "kal ghar 12 baje");
}

/// hinglish/09 ("Ghar ke liye doodh, bread, ande aur sabzi le aana"), as Deepgram
/// `multi` wrote it in BENCH.md §16. Deepgram misheard most of it, so only the script is
/// fixed: the words come out as what the Telugu letters say.
#[test]
fn deepgram_hinglish_09_telugu() {
    let deepgram = "కరికే లే నూన్ బ్రెడ్ అండి పర్సబ్జీ లేహాల";
    assert_eq!(to_roman_hinglish(deepgram), "karike le noon bred andi parsabji lehaal");
}

/// Any character from the eight Brahmic blocks or the Arabic blocks.
fn foreign_char() -> impl Strategy<Value = char> {
    prop_oneof![0x0980u32..=0x0D7F, 0x0600u32..=0x06FF, 0x0750u32..=0x077F, 0xFB50u32..=0xFDFF,]
        .prop_filter_map("not a char", char::from_u32)
}

/// Text mixing Brahmic words, block noise, Urdu, Devanagari and ASCII.
fn mixed_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        prop::sample::select(
            GOLDEN
                .iter()
                .map(|g| g.1)
                .chain(["ਪੱਕਾ", "ੱ", "ੱਕ", "അവൻ", "ൎ", "হঠাৎ", "ತೈ", "ౖ", "تیم", "है", "kal", " ", ", ", "."])
                .collect::<Vec<_>>()
        )
        .prop_map(str::to_owned),
        prop::collection::vec(foreign_char(), 1..6).prop_map(|v| v.into_iter().collect()),
        "[a-z ]{1,6}",
    ];
    prop::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn roman_output_has_no_brahmic_or_arabic_script(text in mixed_text()) {
        let roman = to_roman_hinglish(&text);
        prop_assert!(!has_brahmic(&roman), "{text:?} -> {roman:?}");
        prop_assert!(!has_urdu(&roman), "{text:?} -> {roman:?}");
    }

    #[test]
    fn devanagari_map_leaves_no_brahmic_script(text in mixed_text()) {
        let dev = brahmic_to_devanagari(&text);
        prop_assert!(!has_brahmic(&dev), "{text:?} -> {dev:?}");
    }
}
