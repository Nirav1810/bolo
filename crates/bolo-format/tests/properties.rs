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

/// Transcripts dense with number words, units and time and date words, for the
/// properties that hold whatever the numbers pass does.
fn number_transcript() -> impl Strategy<Value = String> {
    let word = prop::sample::select(vec![
        "one",
        "two",
        "five",
        "nine",
        "ten",
        "twelve",
        "fifteen",
        "twenty",
        "fifty",
        "twenty-five",
        "hundred",
        "thousand",
        "lakh",
        "crore",
        "million",
        "and",
        "point",
        "oh",
        "dollars",
        "bucks",
        "rupees",
        "cents",
        "percent",
        "per",
        "cent",
        "at",
        "pm",
        "o'clock",
        "in",
        "the",
        "evening",
        "march",
        "june",
        "third",
        "first",
        "of",
        "4",
        "30",
        "1,250",
        "₹500",
        "$5",
        "5:30",
        "people",
        "send",
        "kal",
    ]);
    prop::collection::vec((word, separator()), 1..30).prop_map(|parts| {
        let mut s = String::new();
        for (w, sep) in parts {
            s.push_str(w);
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
    text.split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .map(|w| number_word_digits(&w).map_or(w, str::to_owned))
        // Digits compare one by one, so "three pm" → "3 pm" and "4 5 7 2" → "4572"
        // keep their words.
        .flat_map(
            |w| {
                if w.chars().all(|c| c.is_ascii_digit()) { w.chars().map(String::from).collect() } else { vec![w] }
            },
        )
        .collect()
}

/// The digits of a single English number word, which the numbers pass may write as digits.
fn number_word_digits(w: &str) -> Option<&'static str> {
    const WORDS: &[(&str, &str)] = &[
        ("zero", "0"),
        ("one", "1"),
        ("two", "2"),
        ("three", "3"),
        ("four", "4"),
        ("five", "5"),
        ("six", "6"),
        ("seven", "7"),
        ("eight", "8"),
        ("nine", "9"),
        ("ten", "10"),
        ("eleven", "11"),
        ("twelve", "12"),
        ("thirteen", "13"),
        ("fourteen", "14"),
        ("fifteen", "15"),
        ("sixteen", "16"),
        ("seventeen", "17"),
        ("eighteen", "18"),
        ("nineteen", "19"),
    ];
    WORDS.iter().find(|(word, _)| *word == w).map(|(_, d)| *d)
}

/// `n` in English words, Western ("one million two hundred thousand five") or Indian
/// ("twelve lakh five").
fn spoken(n: u64, indian: bool) -> String {
    const SMALL: &[&str] = &[
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: &[&str] = &["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    fn below_1000(n: u64, out: &mut Vec<String>) {
        if n >= 100 {
            out.push(SMALL[(n / 100) as usize].to_owned());
            out.push("hundred".to_owned());
        }
        let r = n % 100;
        if r >= 20 {
            out.push(TENS[(r / 10) as usize].to_owned());
            if !r.is_multiple_of(10) {
                out.push(SMALL[(r % 10) as usize].to_owned());
            }
        } else if r > 0 {
            out.push(SMALL[r as usize].to_owned());
        }
    }
    let scales: &[(u64, &str)] = if indian {
        &[(10_000_000, "crore"), (100_000, "lakh"), (1_000, "thousand")]
    } else {
        &[(1_000_000_000, "billion"), (1_000_000, "million"), (1_000, "thousand")]
    };
    let mut out = Vec::new();
    let mut rest = n;
    for &(scale, word) in scales {
        if rest >= scale {
            below_1000(rest / scale, &mut out);
            out.push(word.to_owned());
            rest %= scale;
        }
    }
    below_1000(rest, &mut out);
    out.join(" ")
}

/// Digits with Western or Indian thousands separators.
fn grouped(n: u64, indian: bool) -> String {
    let s = n.to_string();
    if s.len() <= 3 {
        return s;
    }
    let (head, last3) = s.split_at(s.len() - 3);
    let step = if indian { 2 } else { 3 };
    let mut parts: Vec<String> = Vec::new();
    let mut rest = head.to_owned();
    while rest.len() > step {
        let t = rest.split_off(rest.len() - step);
        parts.insert(0, t);
    }
    parts.insert(0, rest);
    format!("{},{last3}", parts.join(","))
}

/// Whether a value is spoken as a round lakh, crore, million or billion, which keeps its
/// word ("2 crore").
fn is_round(n: u64, indian: bool) -> bool {
    if indian {
        n.is_multiple_of(100_000) && (n < 10_000_000 || n.is_multiple_of(10_000_000))
    } else {
        n >= 1_000_000 && n.is_multiple_of(1_000_000) && (n < 1_000_000_000 || n.is_multiple_of(1_000_000_000))
    }
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
        // "haan to" is the start filler "haan toh" in another spelling.
        let haan_to = w == "to" && i > 0 && input[i - 1] == "haan";
        if out.get(o) == Some(w) {
            o += 1;
        } else if !FILLER_WORDS.contains(&w.as_str()) && last_word != Some(w) && !haan_to {
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
        // "... new" + "new paragraph" or "paragraph" + "paragraph ..." is a stutter, and
        // the stutter pass rightly keeps one copy. That's not what this property is about.
        let w = words(&text);
        prop_assume!(w.last().is_none_or(|l| l != "new") && w.first().is_none_or(|f| f != "paragraph"));
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn numbers_are_idempotent(text in number_transcript(), lang in language(), ctx in context()) {
        let once = rules::apply(&text, lang, ctx);
        let twice = rules::apply(&once, lang, ctx);
        prop_assert_eq!(&once, &twice);
        prop_assert!(!once.is_empty());
    }

    #[test]
    fn spoken_cardinals_become_grouped_digits(n in 10u64..2_000_000_000) {
        prop_assume!(!is_round(n, false));
        let out = rules::apply(&format!("we counted {} items", spoken(n, false)), Language::English, FormatContext::plain());
        let digits = if n >= 10_000 { grouped(n, false) } else { n.to_string() };
        prop_assert_eq!(out, format!("We counted {digits} items."));
    }

    #[test]
    fn spoken_rupees_use_indian_grouping(n in 1u64..1_000_000_000, lang in language()) {
        prop_assume!(!is_round(n, true));
        let out = rules::apply(&format!("pay {} rupees today", spoken(n, true)), lang, FormatContext::plain());
        prop_assert_eq!(out, format!("Pay ₹{} today.", grouped(n, true)));
    }

    #[test]
    fn spoken_dollars_use_western_grouping(n in 1u64..1_000_000_000) {
        prop_assume!(!is_round(n, false));
        let out = rules::apply(&format!("pay {} dollars today", spoken(n, false)), Language::English, FormatContext::plain());
        prop_assert_eq!(out, format!("Pay ${} today.", grouped(n, false)));
    }
}

#[test]
fn whitespace_only_input_is_not_emptied() {
    assert_eq!(rules::apply("\n", Language::English, FormatContext::plain()), "\n");
}

/// Transcripts dense with list markers, joiners and determiners, for the properties that
/// hold whatever the lists pass does.
fn list_transcript() -> impl Strategy<Value = String> {
    let word = prop_oneof![
        prop::sample::select(vec![
            "first", "second", "third", "fourth", "firstly", "one", "two", "three", "four", "ten", "number", "the",
            "my", "of", "and", "then", "milk", "eggs", "bread", "send", "it", "1", "2", "3", "new", "line", "i",
            "need", "things",
        ])
        .prop_map(str::to_owned),
        "[a-z]{1,6}",
    ];
    prop::collection::vec((word, separator()), 1..40).prop_map(|parts| {
        let mut s = String::new();
        for (w, sep) in parts {
            s.push_str(&w);
            s.push_str(sep);
        }
        s
    })
}

/// The words of a list's output, without the item numbers at the start of each line.
fn words_without_item_numbers(text: &str) -> Vec<String> {
    let body: Vec<&str> = text
        .lines()
        .map(|line| {
            let digits = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(line.len());
            if digits > 0 && line[digits..].starts_with(". ") { &line[digits + 2..] } else { line }
        })
        .collect();
    words(&body.join("\n"))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn lists_are_idempotent(text in list_transcript(), lang in language(), ctx in context()) {
        let once = rules::apply(&text, lang, ctx);
        let twice = rules::apply(&once, lang, ctx);
        prop_assert_eq!(&once, &twice);
        prop_assert!(!once.is_empty());
    }

    #[test]
    fn both_renderings_have_the_same_words(text in list_transcript(), lang in language(), code in any::<bool>()) {
        let out = rules::format(&text, lang, code);
        prop_assert_eq!(words(&out.plain), words(&out.markdown));
        let ctx = |markup| FormatContext { markup, code_mode: code };
        prop_assert_eq!(&out.plain, &rules::apply(&text, lang, ctx(rules::Markup::Plain)));
        prop_assert_eq!(&out.markdown, &rules::apply(&text, lang, ctx(rules::Markup::Markdown)));
    }

    #[test]
    fn lists_drop_only_markers_and_joiners(
        lead in prop::option::of(clause()),
        items in prop::collection::vec(clause(), 3..6),
        style in prop::sample::select(vec!["ordinal", "number"]),
        joiner in prop::sample::select(vec!["", "and ", "then "]),
        lang in language(),
        ctx in context(),
    ) {
        // "<lead>, first A, second B, <joiner>third C." keeps every word of the lead and
        // the items, in order, one item per line.
        const ORD: &[&str] = &["first", "second", "third", "fourth", "fifth"];
        const CARD: &[&str] = &["one", "two", "three", "four", "five"];
        // "first of" is not a marker.
        prop_assume!(items.iter().all(|it| !it[0].eq_ignore_ascii_case("of")));
        // A joining word at the end of an item is dropped with the joiner it looks like.
        prop_assume!(items.iter().all(|it| !matches!(it[it.len() - 1].to_lowercase().as_str(), "and" | "then" | "aur")));
        // "number three four five" is a spoken number, so such an item starts with a word.
        let items: Vec<Vec<String>> = items
            .into_iter()
            .map(|mut it| {
                let starts_with_number = number_word_digits(&it[0].to_lowercase()).is_some()
                    || it[0].starts_with(|c: char| c.is_ascii_digit());
                if style == "number" && starts_with_number {
                    it.insert(0, "send".to_owned());
                }
                it
            })
            .collect();
        let last = items.len() - 1;
        let spoken: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(k, it)| {
                let marker = if style == "ordinal" { ORD[k].to_owned() } else { format!("number {}", CARD[k]) };
                let join = if k == last { joiner } else { "" };
                format!("{join}{marker} {}", it.join(" "))
            })
            .collect();
        // A comma ends the lead-in, so that a determiner at its end ("at first") can't
        // guard the first marker.
        let lead_text = lead.as_ref().map(|l| format!("{}, ", l.join(" "))).unwrap_or_default();
        let text = format!("{lead_text}{}.", spoken.join(", "));
        let out = rules::apply(&text, lang, ctx);
        let mut expected = lead.as_ref().map(|l| words(&l.join(" "))).unwrap_or_default();
        for it in &items {
            expected.extend(words(&it.join(" ")));
        }
        prop_assert_eq!(words_without_item_numbers(&out), expected, "output: {:?}", out);
        let item_lines = out.lines().filter(|l| l.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok())).count();
        prop_assert_eq!(item_lines, items.len(), "output: {:?}", out);
    }
}

/// Transcripts with list markers, layout commands and characters HTML treats specially.
fn html_transcript() -> impl Strategy<Value = String> {
    let word = prop_oneof![
        prop::sample::select(vec![
            "first",
            "second",
            "third",
            "one",
            "two",
            "three",
            "number",
            "new",
            "line",
            "paragraph",
            "milk",
            "eggs",
            "<b>",
            "</p>",
            "&",
            "&amp;",
            "a<b",
            "x>y",
            "\"quoted\"",
            "it's",
            "<script>alert(1)</script>",
            "1",
            "2",
            "um",
            "the",
            "send",
        ])
        .prop_map(str::to_owned),
        "[a-z<>&\"']{1,6}",
    ];
    prop::collection::vec((word, separator()), 1..40).prop_map(|parts| {
        let mut s = String::new();
        for (w, sep) in parts {
            s.push_str(&w);
            s.push_str(sep);
        }
        s
    })
}

/// The text of Bolo's HTML, with each list item written "n." on a new line as plain
/// text writes it. Fails on any tag Bolo does not write and on any unescaped `<`, `>`
/// or `&`.
fn html_text(html: &str) -> Result<String, String> {
    const ENTITIES: &[(&str, char)] = &[("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&quot;", '"'), ("&#39;", '\'')];
    let mut out = String::new();
    let mut next_item: Option<u32> = None;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        match c {
            '<' => {
                let end = rest.find('>').ok_or("unclosed tag")?;
                let tag = &rest[..=end];
                match tag {
                    "<p>" | "</p>" | "<br>" => out.push('\n'),
                    "</li>" | "</ol>" => {}
                    "<ol>" => next_item = Some(1),
                    "<li>" => {
                        let n = next_item.ok_or("<li> outside <ol>")?;
                        out.push_str(&format!("\n{n}."));
                        next_item = Some(n + 1);
                    }
                    t if t.starts_with("<ol start=\"") && t.ends_with("\">") => {
                        next_item = Some(t[11..t.len() - 2].parse().map_err(|_| format!("bad start in {t}"))?);
                    }
                    t => return Err(format!("unexpected tag {t}")),
                }
                rest = &rest[end + 1..];
            }
            '&' => {
                let (entity, ch) =
                    ENTITIES.iter().find(|(e, _)| rest.starts_with(e)).ok_or_else(|| format!("bare & at {rest:?}"))?;
                out.push(*ch);
                rest = &rest[entity.len()..];
            }
            '>' => return Err(format!("bare > at {rest:?}")),
            c => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    Ok(out)
}

fn without_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn html_holds_the_plain_text_escaped(text in html_transcript(), lang in language(), code in any::<bool>()) {
        let out = rules::format(&text, lang, code);
        let content = html_text(&out.html);
        prop_assert!(content.is_ok(), "{}: {:?}", content.unwrap_err(), out.html);
        prop_assert_eq!(without_whitespace(&content.unwrap_or_default()), without_whitespace(&out.plain), "html: {:?}", out.html);
        let ctx = FormatContext { markup: rules::Markup::Html, code_mode: code };
        prop_assert_eq!(&out.html, &rules::apply(&text, lang, ctx));
    }

    #[test]
    fn polish_output_is_never_trusted_as_html(text in "[a-z0-9<>&\"'/=. ,\n]{0,80}") {
        // Lines starting "n. " become list items, so the text compares with the numbers out.
        let html = rules::to_html(&text);
        let content = html_text(&html);
        prop_assert!(content.is_ok(), "{}: {:?}", content.unwrap_err(), html);
        let digitless = |s: &str| without_whitespace(s).chars().filter(|c| !c.is_ascii_digit() && *c != '.').collect::<String>();
        prop_assert_eq!(digitless(&content.unwrap_or_default()), digitless(&text), "html: {:?}", html);
    }

    #[test]
    fn to_html_reads_back_the_pipeline_html(text in list_transcript(), lang in language()) {
        // Polish output is parsed back by the same renderer: the plain rendering of a
        // dictation reads back as its HTML, unless a line of it starts like a list item
        // without being one.
        let out = rules::format(&text, lang, false);
        let looks_like_item = |l: &str| l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        prop_assume!(out.plain.lines().filter(|l| looks_like_item(l)).count() == out.html.matches("<li>").count());
        prop_assert_eq!(rules::to_html(&out.plain), out.html.clone(), "plain: {:?}", out.plain);
    }
}
