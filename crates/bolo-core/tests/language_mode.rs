//! The Auto language mode (ADR-0014) through the engine's public API: two scripted passes
//! on a fake STT engine, one result chosen per dictation.

mod common;

use bolo_core::{Language, LanguageMode};
use bolo_format::loanwords::to_roman_hinglish;
use bolo_format::rules::{self, FormatContext};
use common::{FakeStt, Harness, errors, finals, partials};

/// A partial as the engine shows it: Devanagari romanized, English untouched.
fn roman(devanagari: &str) -> String {
    to_roman_hinglish(devanagari)
}

/// A final as the engine inserts it into a plain target: romanized, then the Layer 1
/// rules.
fn inserted(text: &str, language: rules::Language) -> String {
    rules::apply(&roman(text), language, FormatContext::plain())
}

fn hinglish(text: &str) -> String {
    inserted(text, rules::Language::Hinglish)
}

const EN_FINAL: &str = "Please send the report.";
/// What Nemotron forced to Hindi makes of English speech: English words in Devanagari.
const EN_AS_HI: &str = "प्लीज सेंड द रिपोर्ट";
const HI_FINAL: &str = "कल मीटिंग है ना एजेंडा भेज देना";
/// What Nemotron forced to English makes of Hinglish speech.
const HI_AS_EN: &str = "Call meeting high na agenda bhej dena";

fn script_english(stt: &FakeStt) {
    stt.set_script(Language::En, &["please", "please send the", "please send the report"], EN_FINAL);
    stt.set_script(Language::Hi, &["प्लीज", "प्लीज सेंड द", EN_AS_HI], EN_AS_HI);
}

fn script_hinglish(stt: &FakeStt) {
    // The Hindi pass only shows function words from its third partial on.
    stt.set_script(Language::En, &["Call", "Call meeting", HI_AS_EN], HI_AS_EN);
    stt.set_script(Language::Hi, &["कल", "कल मीटिंग", HI_FINAL], HI_FINAL);
}

/// "ok": too short to tell.
fn script_ambiguous(stt: &FakeStt) {
    stt.set_script(Language::En, &["OK"], "OK.");
    stt.set_script(Language::Hi, &["ओके"], "ओके");
}

fn harness(mode: LanguageMode, script: fn(&FakeStt)) -> Harness {
    let stt = FakeStt::new();
    script(&stt);
    Harness::new(stt, mode)
}

/// The one final of a dictation: (text, language).
fn dictate(h: &Harness) -> (String, LanguageMode) {
    let events = h.dictate();
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    let finals = finals(&events);
    assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
    (finals[0].plain.clone(), finals[0].language)
}

#[test]
fn auto_chooses_english_for_english_text() {
    let h = harness(LanguageMode::Auto, script_english);
    assert_eq!(dictate(&h), (EN_FINAL.to_owned(), LanguageMode::English));
    assert_eq!(h.stt.opened(), [Language::En, Language::Hi], "Auto runs both passes");
}

#[test]
fn auto_chooses_hinglish_for_hindi_function_words() {
    let h = harness(LanguageMode::Auto, script_hinglish);
    assert_eq!(dictate(&h), (hinglish(HI_FINAL), LanguageMode::Hinglish));
}

#[test]
fn a_first_ambiguous_dictation_is_english() {
    let h = harness(LanguageMode::Auto, script_ambiguous);
    assert_eq!(dictate(&h), ("OK.".to_owned(), LanguageMode::English));
}

#[test]
fn an_ambiguous_dictation_reuses_the_previous_language() {
    let h = harness(LanguageMode::Auto, script_hinglish);
    assert_eq!(dictate(&h).1, LanguageMode::Hinglish);

    script_ambiguous(&h.stt);
    assert_eq!(dictate(&h), (hinglish("ओके"), LanguageMode::Hinglish));
    assert_eq!(dictate(&h).1, LanguageMode::Hinglish, "still Hinglish: ambiguous keeps the language");

    script_english(&h.stt);
    assert_eq!(dictate(&h).1, LanguageMode::English);
    script_ambiguous(&h.stt);
    assert_eq!(dictate(&h), ("OK.".to_owned(), LanguageMode::English));
}

#[test]
fn auto_decides_on_the_final_text_when_the_preview_was_ambiguous() {
    let h = harness(LanguageMode::Auto, |stt| {
        // At release the Hindi pass has only shown one word; its final text is Hindi.
        stt.set_script(Language::En, &["Call"], HI_AS_EN);
        stt.set_script(Language::Hi, &["कल"], HI_FINAL);
    });
    let events = h.dictate();
    assert_eq!(partials(&events), ["Call"], "the preview stayed on English");
    let finals = finals(&events);
    assert_eq!((finals[0].plain.as_str(), finals[0].language), (hinglish(HI_FINAL).as_str(), LanguageMode::Hinglish));
}

#[test]
fn forced_english_never_runs_a_second_pass() {
    // Even on Hinglish speech: the mode is pinned, so nothing is detected.
    let h = harness(LanguageMode::English, script_hinglish);
    assert_eq!(dictate(&h), (inserted(HI_AS_EN, rules::Language::English), LanguageMode::English));
    assert_eq!(h.stt.opened(), [Language::En]);
}

#[test]
fn forced_hinglish_never_runs_a_second_pass() {
    let h = harness(LanguageMode::Hinglish, script_english);
    assert_eq!(dictate(&h), (hinglish(EN_AS_HI), LanguageMode::Hinglish));
    assert_eq!(h.stt.opened(), [Language::Hi]);
}

#[test]
fn partials_follow_the_favoured_pass() {
    let h = harness(LanguageMode::Auto, script_hinglish);
    let events = h.dictate();
    // Ambiguous at first (no history: English is favoured), then the Hindi pass's
    // function words win. The passes decode on their own threads, so the English
    // pass's third partial may show before the Hindi pass's third one takes over.
    let got = partials(&events);
    let hi = roman(HI_FINAL);
    let hindi_first = ["Call", "Call meeting", hi.as_str()];
    let english_first = ["Call", "Call meeting", HI_AS_EN, hi.as_str()];
    assert!(got == hindi_first || got == english_first, "partials follow English, then the Hindi pass: {got:?}");
    assert_eq!(finals(&events)[0].language, LanguageMode::Hinglish);
}
