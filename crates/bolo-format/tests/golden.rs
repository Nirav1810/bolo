//! Golden cases for the Layer 1 rules pipeline (ADR-0015).
//!
//! Every `tests/golden/*.toml` file holds `[[case]]` tables:
//!
//! ```toml
//! [[case]]
//! note = "what the case shows"   # reported when the case fails
//! input = "raw transcript"
//! expected = "formatted text"
//! language = "hinglish"          # optional: english (default) or hinglish
//! format = "code"                # optional: plain (default), markdown or code
//! ```
//!
//! One test runs them all and lists every failing case by its note. Each output is also
//! run through the pipeline again, which must leave it unchanged. A user-reported
//! regression becomes one new case.

use std::path::{Path, PathBuf};

use bolo_format::rules::{self, FormatContext, Language};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    case: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    note: String,
    input: String,
    expected: String,
    #[serde(default)]
    language: Lang,
    #[serde(default)]
    format: Format,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Lang {
    #[default]
    English,
    Hinglish,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Plain,
    Markdown,
    Code,
}

impl Case {
    fn run(&self, text: &str) -> String {
        let language = match self.language {
            Lang::English => Language::English,
            Lang::Hinglish => Language::Hinglish,
        };
        let ctx = match self.format {
            Format::Plain => FormatContext::plain(),
            Format::Markdown => FormatContext::markdown(),
            Format::Code => FormatContext::code(),
        };
        rules::apply(text, language, ctx)
    }
}

fn golden_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    files
}

#[test]
fn golden_cases() {
    let mut failures = Vec::new();
    let mut count = 0;
    for path in golden_files() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let file: File = toml::from_str(&raw).unwrap_or_else(|e| panic!("{name}: {e}"));
        for case in file.case {
            count += 1;
            let got = case.run(&case.input);
            if got != case.expected {
                failures.push(format!(
                    "{name}: {}\n    input:    {:?}\n    expected: {:?}\n    got:      {got:?}",
                    case.note, case.input, case.expected
                ));
                continue;
            }
            let again = case.run(&got);
            if again != got {
                failures
                    .push(format!("{name}: {} (not idempotent)\n    once:  {got:?}\n    twice: {again:?}", case.note));
            }
        }
    }
    assert!(count > 0, "no golden cases found");
    assert!(failures.is_empty(), "{} of {count} golden cases failed:\n\n{}", failures.len(), failures.join("\n\n"));
}
