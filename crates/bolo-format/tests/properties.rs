//! Property tests for the Layer 1 rules pipeline (ADR-0015): it is idempotent, never
//! returns an empty string for non-empty input, and drops no words outside the passes
//! allowed to (layout commands today; fillers and corrections later).

use bolo_format::rules::{self, FormatContext, Language};
use proptest::prelude::*;

/// Words a transcript is made of, with the awkward cases mixed in: "i" and its
/// contractions, capitals, URLs, times, decimals, Devanagari, edge punctuation.
fn word() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(vec![
            "i",
            "i'm",
            "I'll",
            "hello",
            "Rahul",
            "the",
            "a",
            "send",
            "report",
            "kal",
            "meeting",
            "hai",
            "na",
            "iPhone",
            "PDF",
            "example.com",
            "5:30",
            "3.5",
            "1,000",
            "line",
            "paragraph",
            "of",
            "मीटिंग",
            "है",
            "comma",
            "full",
            "stop",
            "...",
            "e.g.",
            ":)",
        ])
        .prop_map(str::to_owned),
        "[a-zA-Z]{1,8}",
    ]
}

fn separator() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![" ", " ", " ", "  ", ", ", ". ", "? ", "! ", " , ", ",, ", ".. ", "; ", ": ", "\n", " ।"])
}

fn transcript() -> impl Strategy<Value = String> {
    prop::collection::vec((word(), separator()), 1..30).prop_map(|parts| {
        let mut s = String::new();
        for (w, sep) in parts {
            s.push_str(&w);
            s.push_str(sep);
        }
        s
    })
}

fn context() -> impl Strategy<Value = FormatContext> {
    prop::sample::select(vec![FormatContext::plain(), FormatContext::markdown(), FormatContext::code()])
}

fn language() -> impl Strategy<Value = Language> {
    prop::sample::select(vec![Language::English, Language::Hinglish])
}

/// The words of a text, case-folded, with punctuation and spacing ignored.
fn words(text: &str) -> Vec<String> {
    // Devanagari vowel signs are part of words; the danda is punctuation.
    let is_word_char = |c: char| {
        c.is_alphanumeric()
            || matches!(c, '\'' | '\u{2019}')
            || (('\u{0900}'..='\u{097F}').contains(&c) && !matches!(c, '।' | '॥'))
    };
    text.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Whether a transcript contains a layout command, which the pipeline may remove.
fn has_command(text: &str) -> bool {
    let w = words(text);
    w.iter().any(|x| x == "newline")
        || w.windows(2).any(|p| {
            matches!(p[0].as_str(), "new" | "nayi" | "nai" | "nayee" | "naya")
                && matches!(p[1].as_str(), "line" | "paragraph")
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn pipeline_is_idempotent(text in transcript(), lang in language(), ctx in context()) {
        let once = rules::apply(&text, lang, ctx);
        let twice = rules::apply(&once, lang, ctx);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn output_is_never_empty(text in transcript(), lang in language(), ctx in context()) {
        prop_assume!(!text.is_empty());
        prop_assert!(!rules::apply(&text, lang, ctx).is_empty());
    }

    #[test]
    fn no_words_are_dropped(text in transcript(), lang in language(), ctx in context()) {
        prop_assume!(!has_command(&text));
        let out = rules::apply(&text, lang, ctx);
        prop_assert_eq!(words(&text), words(&out), "output: {:?}", out);
    }

    #[test]
    fn commands_drop_only_their_own_words(text in transcript(), lang in language(), ctx in context()) {
        // Spoken with a layout command spliced in, the other words all survive.
        prop_assume!(!has_command(&text));
        let with_command = format!("{text} new paragraph {text}");
        let out = rules::apply(&with_command, lang, ctx);
        let mut expected = words(&text);
        expected.extend(words(&text));
        let got = words(&out);
        // "new paragraph" may be kept when guarded (after a determiner, before "of").
        prop_assert!(got == expected || got.len() == expected.len() + 2, "output: {:?}", out);
    }
}

#[test]
fn whitespace_only_input_is_not_emptied() {
    assert_eq!(rules::apply("\n", Language::English, FormatContext::plain()), "\n");
}
