//! Dictation lifecycle through the engine's public API, on a scripted fake STT engine and
//! a replayed waveform (no models, no network).

mod common;

use std::time::Duration;

use bolo_core::{DictationState, Language, LanguageMode};
use common::{ENGINE_ID, FakeStt, Harness, errors, finals, partials, states};

fn english() -> FakeStt {
    FakeStt::new().script(
        Language::En,
        &["please", "please send the", "please send the report"],
        "Please send the report.",
    )
}

#[test]
fn a_dictation_yields_the_scripted_final_with_timings() {
    let h = Harness::new(english(), LanguageMode::English);
    let events = h.dictate();

    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    let finals = finals(&events);
    assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
    let out = &finals[0];
    assert_eq!(out.plain, "Please send the report.");
    assert_eq!(out.engine_id, ENGINE_ID);
    let t = out.timings;
    assert!(t.total_ms > 0.0, "total latency recorded: {t:?}");
    assert!(t.stt_ms >= 0.0 && t.stt_ms <= t.total_ms, "stt latency within total: {t:?}");

    // The fake really streamed: partials arrived live, in script order.
    assert_eq!(partials(&events), ["please", "please send the", "please send the report"]);
    assert_eq!(h.stt.opened(), [Language::En]);
}

#[test]
fn dictation_states_go_idle_listening_processing_done_idle() {
    let h = Harness::new(english(), LanguageMode::English);
    assert_eq!(h.engine.state(), DictationState::Idle);

    let events = h.dictate();

    assert_eq!(
        states(&events),
        [DictationState::Listening, DictationState::Processing, DictationState::Done, DictationState::Idle]
    );
    assert_eq!(h.engine.state(), DictationState::Idle);
}

#[test]
fn cancel_produces_no_final_and_returns_to_idle() {
    let h = Harness::new(english(), LanguageMode::English);
    h.start_and_speak();
    h.engine.cancel_dictation();

    let mut events = h.events_until_idle();
    events.extend(h.events_within(Duration::from_millis(300)));

    assert!(finals(&events).is_empty(), "cancel must not produce a final: {events:?}");
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    assert_eq!(states(&events), [DictationState::Listening, DictationState::Idle]);
    assert_eq!(h.engine.state(), DictationState::Idle);

    // The engine is reusable after a cancel.
    let finals = finals(&h.dictate());
    assert_eq!(finals.len(), 1);
    assert_eq!(finals[0].plain, "Please send the report.");
}
