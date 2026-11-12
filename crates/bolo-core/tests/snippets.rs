//! Snippets in the engine (M4 ticket 02): triggers are expanded last, in `on_final` and
//! `on_polished`, in every rendering. Polish is sent the text before expansion and told
//! which triggers to keep.

mod common;

use std::sync::Arc;
use std::time::Duration;

use bolo_core::{Language, LanguageMode, OutputFormat, Polish, Snippet, TargetContext};
use common::{FakeLlm, FakeStt, Harness, config, errors, finals, polished};

const RAW: &str = "please send it to my address";
/// What Layer 1 makes of [`RAW`], before expansion.
const INSTANT: &str = "Please send it to my address.";
const ADDRESS: &str = "221B Baker St\nLondon <NW1>";
const EXPANDED: &str = "Please send it to 221B Baker St\nLondon <NW1>.";
const SETTLE: Duration = Duration::from_millis(500);

fn snippets() -> Vec<Snippet> {
    vec![Snippet { trigger: "my address".into(), text: ADDRESS.into() }]
}

fn target(format: OutputFormat) -> TargetContext {
    TargetContext { format, ..Default::default() }
}

fn stt() -> FakeStt {
    FakeStt::new().script(Language::En, &["please send"], RAW)
}

#[test]
fn the_final_is_expanded_in_every_rendering() {
    let h = Harness::new(stt(), LanguageMode::English);
    h.engine.set_snippets(snippets());
    let events = h.dictate_into(target(OutputFormat::Html));
    assert!(errors(&events).is_empty(), "{events:?}");
    let out = &finals(&events)[0];
    assert_eq!((out.plain.as_str(), out.markdown.as_str()), (EXPANDED, EXPANDED));
    assert_eq!(out.html.as_deref(), Some("<p>Please send it to 221B Baker St<br>London &lt;NW1&gt;.</p>"));

    h.engine.set_snippets(Vec::new());
    let events = h.dictate();
    assert_eq!(finals(&events)[0].plain, INSTANT, "no snippets, no expansion");
}

#[test]
fn snippets_come_in_through_the_engine_config() {
    let h = Harness::new(stt(), LanguageMode::English);
    h.engine
        .update_config(bolo_core::EngineConfig { snippets: snippets(), ..config(LanguageMode::English, Polish::Off) });
    let events = h.dictate();
    assert_eq!(finals(&events)[0].plain, EXPANDED);
}

#[test]
fn polish_keeps_the_trigger_and_its_answer_is_expanded() {
    let llm = Arc::new(FakeLlm::answering("Please send it to My Address!", Duration::from_millis(20)));
    let on = Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] };
    let h = Harness::with_polish(stt(), LanguageMode::English, on, llm.clone());
    h.engine.set_snippets(snippets());
    let mut events = h.dictate_into(target(OutputFormat::Html));
    events.extend(h.events_within(SETTLE));
    assert!(errors(&events).is_empty(), "{events:?}");

    assert_eq!(finals(&events)[0].plain, EXPANDED, "the instant text is expanded");
    let calls = llm.log();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].transcript, INSTANT, "polish gets the text before expansion");
    assert_eq!(calls[0].triggers, ["my address"], "and is told which triggers to keep");

    let out = &polished(&events)[0];
    let want = "Please send it to 221B Baker St\nLondon <NW1>!";
    assert_eq!((out.plain.as_str(), out.markdown.as_str()), (want, want));
    assert_eq!(out.html.as_deref(), Some("<p>Please send it to 221B Baker St<br>London &lt;NW1&gt;!</p>"));
}

#[test]
fn polish_is_not_told_about_triggers_it_was_not_sent() {
    let llm = Arc::new(FakeLlm::answering("Please send it to my address.", Duration::from_millis(10)));
    let on = Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] };
    let h = Harness::with_polish(stt(), LanguageMode::English, on, llm.clone());
    h.engine.set_snippets(vec![Snippet { trigger: "sig".into(), text: "Thanks".into() }]);
    let mut events = h.dictate();
    events.extend(h.events_within(SETTLE));
    assert!(llm.log()[0].triggers.is_empty());
    assert_eq!(polished(&events)[0].plain, INSTANT);
}
