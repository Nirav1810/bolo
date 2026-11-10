//! Property tests for the Layer 1 rules pipeline (ADR-0015): it is idempotent, never
//! returns an empty string for non-empty input, and drops no words outside the passes
//! allowed to: fillers and stutters, layout commands, and recognized self-corrections.

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
            "um",
            "uh",
            "er",
            "you",
            "know",
            "like",
            "matlab",
            "woh",
            "haan",
            "toh",
            "the",
            "I",
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

/// Words the filler pass may remove: hesitations, "you know", "like", Hindi fillers.
const FILLER_WORDS: &[&str] = &[
    "um", "umm", "ummm", "uhm", "uh", "uhh", "uhhh", "er", "erm", "you", "know", "like", "matlab", "woh", "wo", "na",
    "haan", "toh",
];

/// Whether a transcript contains a self-correction marker.
fn has_marker(text: &str) -> bool {
    let w = words(text);
    w.iter().any(|x| matches!(x.as_str(), "sorry" | "nahi" | "nahin"))
        || w.windows(2)
            .any(|p| matches!((p[0].as_str(), p[1].as_str()), ("actually", "no") | ("i", "mean") | ("scratch", "that")))
}

/// Whether `out` is `input` with only fillers and stutters (a word equal to the last
/// word before it that isn't a filler) removed.
fn only_fillers_dropped(input: &[String], out: &[String]) -> Result<(), String> {
    let mut o = 0;
    let mut last_word: Option<&String> = None;
    for (i, w) in input.iter().enumerate() {
        if out.get(o) == Some(w) {
            o += 1;
        } else if !FILLER_WORDS.contains(&w.as_str()) && last_word != Some(w) {
            return Err(format!("dropped {w:?} at word {i}"));
        }
        if !FILLER_WORDS.contains(&w.as_str()) {
            last_word = Some(w);
        }
    }
    if o == out.len() { Ok(()) } else { Err(format!("added {:?}", &out[o..])) }
}

/// Plain words for the correction property: no fillers, markers, commands, repair leads
/// or stutter-prone "i", but dates, times and numbers so that corrections fire.
fn plain_word() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(vec![
            "hello", "Rahul", "the", "a", "send", "report", "kal", "meeting", "hai", "Friday", "Tuesday", "5", "pm",
            "4:30", "three", "four", "people", "option", "two", "office", "team", "Priya", "red", "blue", "one",
            "chalte", "hain", "parson", "at", "to",
        ])
        .prop_map(str::to_owned),
        "[a-z]{1,8}".prop_filter("reserved word", |w| {
            !FILLER_WORDS.contains(&w.as_str())
                && !matches!(
                    w.as_str(),
                    "actually"
                        | "no"
                        | "mean"
                        | "sorry"
                        | "scratch"
                        | "that"
                        | "nahi"
                        | "nahin"
                        | "make"
                        | "rather"
                        | "new"
                        | "nayi"
                        | "nai"
                        | "nayee"
                        | "naya"
                        | "newline"
                        | "line"
                        | "paragraph"
                        | "i"
                )
        }),
    ]
}

/// A clause of plain words with no word repeated right after itself.
fn clause() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(plain_word(), 1..8).prop_map(|mut w| {
        w.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        w
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
    fn only_fillers_and_stutters_are_dropped(text in transcript(), lang in language(), ctx in context()) {
        prop_assume!(!has_command(&text) && !has_marker(&text));
        let out = rules::apply(&text, lang, ctx);
        let result = only_fillers_dropped(&words(&text), &words(&out));
        prop_assert!(result.is_ok(), "{}: output {:?}", result.unwrap_err(), out);
    }

    #[test]
    fn corrections_drop_only_the_corrected_sentence(
        a in clause(),
        b in clause(),
        c in clause(),
        marker in prop::sample::select(vec!["actually no", "I mean", "sorry", "scratch that", "nahi nahi", "nahi wait"]),
        lang in language(),
        ctx in context(),
    ) {
        // "<A>. <B>, <marker>, <C>." A correction may only touch B and the marker: A
        // stays whole in front, and every word of C survives.
        let text = format!("{}. {}, {marker}, {}.", a.join(" "), b.join(" "), c.join(" "));
        let out = words(&rules::apply(&text, lang, ctx));
        let (a, b, c) = (words(&a.join(" ")), words(&b.join(" ")), words(&c.join(" ")));
        prop_assert!(out.starts_with(&a), "A was changed: {:?}", out);
        let rest = &out[a.len()..];
        let mut r = rest.iter();
        prop_assert!(c.iter().all(|w| r.any(|x| x == w)), "a word of C was dropped: {:?}", out);
        let mut pool: Vec<&String> = b.iter().chain(&c).collect();
        let marker_words = words(marker);
        pool.extend(&marker_words);
        for w in rest {
            let found = pool.iter().position(|x| *x == w);
            prop_assert!(found.is_some(), "{w:?} came from nowhere: {:?}", out);
            pool.swap_remove(found.unwrap_or_default());
        }
    }

    #[test]
    fn commands_drop_only_their_own_words(text in transcript(), lang in language(), ctx in context()) {
        // Spoken with a layout command spliced in, the other words all survive.
        prop_assume!(!has_command(&text));
        let with_command = format!("{text} new paragraph {text}");
        let out = rules::apply(&with_command, lang, ctx);
        let mut expected = words(&text);
        expected.extend(words(&text));
        let mut kept = words(&text);
        kept.extend(["new".to_owned(), "paragraph".to_owned()]);
        kept.extend(words(&text));
        // Fillers may go as well; "new paragraph" may be kept when guarded (after a
        // determiner, before "of").
        let got = words(&out);
        prop_assert!(
            only_fillers_dropped(&expected, &got).is_ok() || only_fillers_dropped(&kept, &got).is_ok(),
            "output: {:?}",
            out
        );
    }
}

#[test]
fn whitespace_only_input_is_not_emptied() {
    assert_eq!(rules::apply("\n", Language::English, FormatContext::plain()), "\n");
}
