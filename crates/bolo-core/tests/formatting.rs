//! Layer 1 on the engine's finals: every final goes through the rules pipeline for the
//! dictation's target app (ADR-0015), and its time is reported as `rules_ms`. One
//! pipeline run fills both `plain` and `markdown`.

mod common;

use bolo_core::{Language, LanguageMode, OutputFormat, TargetContext};
use common::{FakeStt, Harness, errors, finals, partials};

/// Bare engine output: no capitals, no final period, a spoken layout command.
const RAW: &str = "hi rahul new line i will send it today";

fn engine() -> Harness {
    Harness::new(FakeStt::new().script(Language::En, &["hi", "hi rahul"], RAW), LanguageMode::English)
}

fn final_text(h: &Harness, target: TargetContext) -> String {
    let events = h.dictate_into(target);
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    let finals = finals(&events);
    assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
    let out = &finals[0];
    assert_eq!(out.plain, out.markdown, "no list: both renderings match");
    let t = out.timings;
    assert!(t.rules_ms >= 0.0 && t.rules_ms <= t.total_ms, "rules time within total: {t:?}");
    out.plain.clone()
}

#[test]
fn a_plain_target_gets_the_rules() {
    let h = engine();
    assert_eq!(final_text(&h, TargetContext::default()), "Hi rahul\nI will send it today.");
}

#[test]
fn a_code_target_gets_no_capitals_and_no_period() {
    let h = engine();
    let code = TargetContext { app_id: "code.exe".into(), code_mode: true, ..Default::default() };
    assert_eq!(final_text(&h, code), "hi rahul\ni will send it today");
}

#[test]
fn a_markdown_target_gets_the_rules() {
    let h = engine();
    let md = TargetContext { format: OutputFormat::Markdown, ..Default::default() };
    assert_eq!(final_text(&h, md), "Hi rahul\nI will send it today.");
}

/// A spoken list between two sentences.
const LIST: &str = "hi team for tomorrow first review the deck second fix the numbers third send it to rahul. thanks";

#[test]
fn a_spoken_list_fills_both_renderings() {
    let h = Harness::new(FakeStt::new().script(Language::En, &["hi team"], LIST), LanguageMode::English);
    for target in [TargetContext::default(), TargetContext { format: OutputFormat::Markdown, ..Default::default() }] {
        let events = h.dictate_into(target);
        let finals = finals(&events);
        assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
        let out = &finals[0];
        assert_eq!(
            out.plain,
            "Hi team for tomorrow:
1. Review the deck
2. Fix the numbers
3. Send it to rahul
Thanks."
        );
        assert_eq!(
            out.markdown,
            "Hi team for tomorrow:

1. Review the deck
2. Fix the numbers
3. Send it to rahul

Thanks."
        );
    }
}

#[test]
fn partials_are_not_formatted() {
    let h = engine();
    let events = h.dictate();
    assert_eq!(partials(&events), ["hi", "hi rahul"]);
}
