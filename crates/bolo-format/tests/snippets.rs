//! Snippet expansion (`bolo_format::snippets`): the golden cases in
//! `tests/snippets/cases.toml`, and the property that text with no trigger in it comes
//! back unchanged.

use bolo_format::rules::{self, Language};
use bolo_format::snippets::{Snippet, Snippets, expand, expand_html};
use proptest::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    snippet: Vec<Snippet>,
    case: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    note: String,
    input: String,
    expected: String,
    #[serde(default)]
    format: Format,
    #[serde(default)]
    language: Lang,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum Format {
    #[default]
    Text,
    Html,
    Layer1,
    Layer1Markdown,
    Layer1Code,
    Layer1Html,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Lang {
    #[default]
    English,
    Hinglish,
}

impl Case {
    fn run(&self, snippets: &[Snippet]) -> String {
        let language = match self.language {
            Lang::English => Language::English,
            Lang::Hinglish => Language::Hinglish,
        };
        let layer1 = |code| rules::format(&self.input, language, code);
        match self.format {
            Format::Text => expand(&self.input, snippets),
            Format::Html => expand_html(&self.input, snippets),
            Format::Layer1 => expand(&layer1(false).plain, snippets),
            Format::Layer1Markdown => expand(&layer1(false).markdown, snippets),
            Format::Layer1Code => expand(&layer1(true).plain, snippets),
            Format::Layer1Html => expand_html(&layer1(false).html, snippets),
        }
    }
}

#[test]
fn golden_cases() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snippets/cases.toml");
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let file: File = toml::from_str(&raw).unwrap_or_else(|e| panic!("{path}: {e}"));
    let failures: Vec<String> = file
        .case
        .iter()
        .filter_map(|case| {
            let got = case.run(&file.snippet);
            (got != case.expected).then(|| {
                format!(
                    "{}\n    input:    {:?}\n    expected: {:?}\n    got:      {got:?}",
                    case.note, case.input, case.expected
                )
            })
        })
        .collect();
    assert!(!file.case.is_empty(), "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n\n{}",
        failures.len(),
        file.case.len(),
        failures.join("\n\n")
    );
}

#[test]
fn no_snippets_leave_text_alone() {
    for text in ["", "  ", "My address.", "<p>x</p>"] {
        assert_eq!(expand(text, &[]), text);
        assert_eq!(expand_html(text, &[]), text);
    }
    assert!(Snippets::new(&[Snippet { trigger: " ,. ".into(), text: "x".into() }]).is_empty(), "no words, no trigger");
}

/// Text made only of the letters a–m (and punctuation, spaces, line breaks, Devanagari
/// and emoji), and triggers made only of the letters n–z, so no trigger can be in the
/// text. No digits in the text: a trigger like "two" matches "2".
fn text() -> impl Strategy<Value = String> {
    "[a-mA-M .,!?;:'\"()\\-\n]{0,40}(मीटिंग|है|🙂|é|—)?[a-mA-M .,!?\n]{0,40}"
}

fn snippets() -> impl Strategy<Value = Vec<Snippet>> {
    prop::collection::vec(
        ("[n-zN-Z]{1,6}( [n-z]{1,6}){0,2}", ".{0,20}").prop_map(|(trigger, text)| Snippet { trigger, text }),
        0..5,
    )
}

proptest! {
    #[test]
    fn text_without_a_trigger_comes_back_unchanged(text in text(), snippets in snippets()) {
        prop_assert_eq!(expand(&text, &snippets), text.clone());
        let formatted = rules::format(&text, Language::English, false);
        prop_assert_eq!(expand(&formatted.plain, &snippets), formatted.plain);
        prop_assert_eq!(expand_html(&formatted.html, &snippets), formatted.html.clone());
        let html = rules::to_html(&text);
        prop_assert_eq!(expand_html(&html, &snippets), html);
    }

    #[test]
    fn a_lone_trigger_after_layer1_is_its_expansion(
        trigger in "[a-z]{3,8}( [a-z]{3,8}){0,2}",
        text in "[A-Za-z0-9 ,]{1,20}",
    ) {
        let snippets = [Snippet { trigger: trigger.clone(), text: text.clone() }];
        let plain = rules::format(&trigger, Language::English, false).plain;
        prop_assume!(plain.chars().any(char::is_alphanumeric), "the rules dropped every word");
        prop_assert_eq!(expand(&plain, &snippets), text);
    }
}
