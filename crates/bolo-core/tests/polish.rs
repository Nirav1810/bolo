//! Polish in the engine (ADR-0016): with polish on, `on_final` carries the instant text
//! and `on_polished` follows with the polished text. A timeout, an error, a failed guard
//! or a cancel means no `on_polished`, and the Layer 1 text stands. The LLM is a fake.

mod common;

use std::sync::Arc;
use std::time::Duration;

use bolo_core::{
    BoloError, DictationState, Engine, EngineEvents, FormattedOutput, Language, LanguageMode, OutputFormat,
    POLISH_DEADLINE, Polish, TargetContext, WavSource,
};
use common::{Ev, FakeLlm, FakeStt, Harness, config, errors, finals, polished, tone};

const RAW: &str = "um can you move the standup to eleven thirty";
/// What Layer 1 makes of [`RAW`].
const INSTANT: &str = "Can you move the standup to eleven thirty.";
const POLISHED: &str = "Can you move the standup to 11:30?";
/// Long enough to be sure a polish that is coming has come.
const SETTLE: Duration = Duration::from_millis(500);

fn on() -> Polish {
    Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] }
}

fn engine(polish: Polish, llm: FakeLlm) -> (Harness, Arc<FakeLlm>) {
    let llm = Arc::new(llm);
    let stt = FakeStt::new().script(Language::En, &["um can you"], RAW);
    (Harness::with_polish(stt, LanguageMode::English, polish, llm.clone()), llm)
}

#[test]
fn the_instant_text_comes_first_then_the_polished_text() {
    let (h, llm) = engine(on(), FakeLlm::answering(POLISHED, Duration::from_millis(50)));
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");

    let at = |want: fn(&Ev) -> bool| events.iter().position(want);
    let (Some(fin), Some(pol)) = (at(|e| matches!(e, Ev::Final(_))), at(|e| matches!(e, Ev::Polished(_)))) else {
        panic!("expected on_final and on_polished: {events:?}");
    };
    assert!(fin < pol, "on_final before on_polished: {events:?}");
    let idle = at(|e| matches!(e, Ev::State(DictationState::Idle)));
    assert!(idle.is_some_and(|i| i < pol), "polish never holds up the instant path: {events:?}");

    let (instant, polished) = (&finals(&events)[0], &polished(&events)[0]);
    assert_eq!(instant.plain, INSTANT);
    assert_eq!(instant.timings.llm_ms, 0.0);
    assert_eq!((polished.plain.as_str(), polished.markdown.as_str()), (POLISHED, POLISHED));
    assert!(polished.timings.llm_ms >= 50.0, "llm_ms is the polish call: {:?}", polished.timings);
    assert!(polished.timings.total_ms >= instant.timings.total_ms + polished.timings.llm_ms - 1.0);
    assert_eq!(polished.engine_id, instant.engine_id);
    assert_eq!(llm.calls(), vec![INSTANT.to_owned()], "the LLM is sent the instant text");
}

#[test]
fn polished_text_into_an_html_target_is_rendered_as_html() {
    let (h, _) = engine(on(), FakeLlm::answering(POLISHED, Duration::from_millis(20)));
    let html = |format| {
        let mut events = h.dictate_into(TargetContext { format, ..Default::default() });
        events.extend(h.events_within(SETTLE));
        polished(&events)[0].html.clone()
    };
    assert_eq!(html(OutputFormat::Html).as_deref(), Some("<p>Can you move the standup to 11:30?</p>"));
    assert_eq!(html(OutputFormat::Plain), None);
}

#[test]
fn polish_off_means_no_polish() {
    let (h, llm) = engine(Polish::Off, FakeLlm::answering(POLISHED, Duration::ZERO));
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert_eq!(finals(&events).len(), 1);
    assert!(polished(&events).is_empty(), "no on_polished: {events:?}");
    assert!(llm.calls().is_empty(), "the LLM is never called");
}

/// The dictation's events plus everything for a while after, which must hold no
/// `on_polished` and leave the instant text as the result.
fn assert_not_polished(h: &Harness, wait: Duration) {
    let mut events = h.dictate();
    events.extend(h.events_within(wait));
    assert!(errors(&events).is_empty(), "a skipped polish is not an error: {events:?}");
    assert_eq!(finals(&events)[0].plain, INSTANT);
    assert!(polished(&events).is_empty(), "no on_polished: {events:?}");
}

#[test]
fn a_timeout_means_no_on_polished() {
    let late = POLISH_DEADLINE + Duration::from_millis(300);
    let (h, llm) = engine(on(), FakeLlm::answering(POLISHED, late));
    assert_not_polished(&h, late + SETTLE);
    assert_eq!(llm.calls().len(), 1);
}

#[test]
fn an_llm_error_means_no_on_polished() {
    let (h, _) = engine(on(), FakeLlm::failing(Duration::from_millis(20)));
    assert_not_polished(&h, SETTLE);
}

#[test]
fn a_failed_guard_means_no_on_polished() {
    let reply = "I'm sorry, but I can't move meetings for you. I only clean up dictated text, so please \
                 open your calendar and move the standup there.";
    let (h, _) = engine(on(), FakeLlm::answering(reply, Duration::from_millis(20)));
    assert_not_polished(&h, SETTLE);
    let (h, _) = engine(on(), FakeLlm::answering("  ", Duration::from_millis(20)));
    assert_not_polished(&h, SETTLE);
}

#[test]
fn cancel_during_polish_means_no_on_polished() {
    let (h, llm) = engine(on(), FakeLlm::answering(POLISHED, Duration::from_millis(300)));
    let events = h.dictate();
    assert_eq!(finals(&events).len(), 1);
    llm.wait_for_call(); // the polish is in flight
    h.engine.cancel_dictation();
    let after = h.events_within(Duration::from_millis(300) + SETTLE);
    assert!(polished(&after).is_empty(), "no on_polished after cancel: {after:?}");
}

#[test]
fn the_next_dictation_supersedes_a_running_polish() {
    let (h, llm) = engine(on(), FakeLlm::answering(POLISHED, Duration::from_millis(400)));
    let first = h.dictate();
    let second = h.dictate();
    let after = h.events_within(Duration::from_millis(400) + SETTLE);
    assert!(polished(&first).is_empty() && polished(&second).is_empty());
    assert_eq!(polished(&after).len(), 1, "only the second dictation is polished: {after:?}");
    assert!(!llm.calls().is_empty());
}

struct Silent;

impl EngineEvents for Silent {
    fn on_level(&self, _: f32) {}
    fn on_partial(&self, _: String) {}
    fn on_final(&self, _: FormattedOutput) {}
    fn on_polished(&self, _: FormattedOutput) {}
    fn on_error(&self, _: BoloError) {}
    fn on_state(&self, _: DictationState) {}
}

#[test]
fn polish_on_without_an_anthropic_key_is_a_config_error() {
    let started = Engine::with_stt_engine(
        config(LanguageMode::English, on()),
        Box::new(Silent),
        WavSource::factory(Arc::new(tone(0.1))),
        Arc::new(FakeStt::new()),
    );
    assert!(matches!(started, Err(BoloError::Config(_))), "got {started:?}");

    // Turned on later, without a key: the next dictation refuses to start.
    let engine = Engine::with_stt_engine(
        config(LanguageMode::English, Polish::Off),
        Box::new(Silent),
        WavSource::factory(Arc::new(tone(0.1))),
        Arc::new(FakeStt::new()),
    )
    .expect("polish off needs no key");
    engine.update_config(config(LanguageMode::English, on()));
    let refused = engine.start_dictation(Default::default());
    assert!(matches!(refused, Err(BoloError::Config(_))), "got {refused:?}");
}

#[test]
fn set_polish_applies_from_the_next_dictation_with_its_app() {
    let (h, llm) = engine(Polish::Off, FakeLlm::answering(POLISHED, Duration::from_millis(20)));
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert!(polished(&events).is_empty(), "off at first: {events:?}");

    h.engine.set_polish(Polish::On { tone: "neutral".into(), app: "Slack".into(), dictionary: vec![] });
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert_eq!(polished(&events)[0].plain, POLISHED);
    assert_eq!(llm.log().iter().map(|c| c.app.as_str()).collect::<Vec<_>>(), ["Slack"], "the app is per dictation");

    h.engine.set_polish(Polish::Off);
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert!(polished(&events).is_empty(), "off again: {events:?}");
    assert_eq!(llm.calls().len(), 1);
}
