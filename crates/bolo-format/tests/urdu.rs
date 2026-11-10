//! Property test for the Urdu-script map (M2 ticket 07): Roman-script output never
//! contains an Arabic-script character, whatever mix of scripts goes in.

use bolo_format::loanwords::to_roman_hinglish;
use bolo_format::urdu::{has_urdu, urdu_to_devanagari};
use proptest::prelude::*;

/// Any character from the Arabic blocks, presentation forms included.
fn arabic_char() -> impl Strategy<Value = char> {
    prop_oneof![0x0600u32..=0x06FF, 0x0750u32..=0x077F, 0x0870u32..=0x08FF, 0xFB50u32..=0xFDFF, 0xFE70u32..=0xFEFE,]
        .prop_filter_map("not a char", char::from_u32)
}

/// Text mixing Urdu words, Arabic-block noise, Devanagari and ASCII.
fn mixed_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        prop::sample::select(vec![
            "تیم",
            "ہے",
            "میٹنگ",
            "کیا",
            "ٹھیک",
            "ئ",
            "ۓ",
            "ھ",
            "ں",
            "ّ",
            "ٰ",
            "۔",
            "،",
            "۱۲",
            "ﷺ",
            "मीटिंग",
            "है",
            "kal",
            "meeting",
            " ",
            ", ",
            ".",
        ])
        .prop_map(str::to_owned),
        prop::collection::vec(arabic_char(), 1..6).prop_map(|v| v.into_iter().collect()),
        "[a-z ]{1,6}",
    ];
    prop::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn roman_output_has_no_arabic_script(text in mixed_text()) {
        let roman = to_roman_hinglish(&text);
        prop_assert!(!has_urdu(&roman), "{text:?} -> {roman:?}");
    }

    #[test]
    fn devanagari_map_leaves_no_arabic_script(text in mixed_text()) {
        let dev = urdu_to_devanagari(&text);
        prop_assert!(!has_urdu(&dev), "{text:?} -> {dev:?}");
    }
}
