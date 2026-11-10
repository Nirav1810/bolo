//! Per-segment polish (ADR-0016): with polish on, a dictation of 20 s or longer is
//! polished one segment at a time while the user is still speaking, each segment closed
//! by a pause of 700 ms or more. Release then only waits for the rest. The STT and the
//! LLM are fakes, and the waveform replays 10× faster than real time.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use bolo_core::{DictationState, Language, LanguageMode, Polish, SEGMENT_AFTER, TargetContext};
use common::{Ev, FakeLlm, FakeStt, Harness, Reply, errors, finals, polished, silence, states, tone};

const SPEED: f64 = 10.0;
/// Long enough to be sure a polish that is coming has come.
const SETTLE: Duration = Duration::from_millis(500);

const FIRST: &str = "first part of the note";
const SECOND: &str = "second bit here";
const LAST: &str = "last words";

fn on() -> Polish {
    Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] }
}

/// Speech and pauses, in seconds: `(speech, pause after it)`.
fn waveform(phrases: &[(f32, f32)]) -> Vec<f32> {
    phrases.iter().flat_map(|&(speech, pause)| [tone(speech), silence(pause)]).flatten().collect()
}

/// A fake stream whose text grows phrase by phrase: `texts[i]` is shown from the start
/// of phrase `i` until the next phrase starts (through the pause after it).
fn stt(phrases: &[(f32, f32)], texts: &[&str]) -> FakeStt {
    let mut partials = Vec::new();
    let mut shown = Vec::new();
    for (&(speech, pause), text) in phrases.iter().zip(texts) {
        shown.push(*text);
        let ticks = ((speech + pause) * 10.0).round() as usize; // one partial per 0.1 s
        partials.extend(std::iter::repeat_n(shown.join(" "), ticks));
    }
    let partials: Vec<&str> = partials.iter().map(String::as_str).collect();
    FakeStt::new().script(Language::En, &partials, &texts.join(" "))
}

/// The three-phrase dictation: 21 s, then two short phrases, each after a 1 s pause.
const LONG: [(f32, f32); 3] = [(21.0, 1.0), (2.0, 1.0), (1.5, 0.0)];

/// The fake LLM's polish: Layer 1's periods become exclamation marks.
fn exclaim(transcript: &str) -> Reply {
    Reply { delay: Duration::from_millis(50), answer: Ok(transcript.replace('.', "!")) }
}

struct Run {
    events: Vec<Ev>,
    /// When `stop_dictation` was called.
    released: Instant,
}

/// Play `phrases` into a dictation, release, and collect events until polish settles.
fn dictate(h: &Harness, phrases: &[(f32, f32)]) -> Run {
    h.engine.start_dictation(TargetContext::default()).expect("start_dictation");
    let secs: f32 = phrases.iter().map(|(s, p)| s + p).sum();
    std::thread::sleep(Duration::from_secs_f64(f64::from(secs) / SPEED + 0.2));
    let released = Instant::now();
    h.engine.stop_dictation().expect("stop_dictation");
    let mut events = h.events_until_idle();
    events.extend(h.events_within(SETTLE));
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    Run { events, released }
}

fn long_engine(llm: FakeLlm) -> (Harness, Arc<FakeLlm>) {
    let llm = Arc::new(llm);
    let h = Harness::with_audio(
        stt(&LONG, &[FIRST, SECOND, LAST]),
        LanguageMode::English,
        on(),
        llm.clone(),
        waveform(&LONG),
        SPEED,
    );
    (h, llm)
}

#[test]
fn a_long_dictation_is_polished_per_segment_before_release() {
    assert!(LONG[0].0 > SEGMENT_AFTER.as_secs_f32(), "the first pause comes after the 20 s mark");
    let (h, llm) = long_engine(FakeLlm::replying(exclaim));
    let run = dictate(&h, &LONG);

    let log = llm.log();
    let sent: Vec<&str> = log.iter().map(|c| c.transcript.as_str()).collect();
    assert_eq!(sent, ["First part of the note.", "Second bit here.", "Last words."], "segments in order");
    assert!(log[0].at < run.released && log[1].at < run.released, "segment polish starts before release: {log:?}");
    assert!(log[2].at >= run.released, "the rest is polished at release");
    let contexts: Vec<Option<&str>> = log.iter().map(|c| c.context.as_deref()).collect();
    assert_eq!(
        contexts,
        [None, Some("First part of the note!"), Some("Second bit here!")],
        "each segment sees the previous polished one"
    );

    assert_eq!(finals(&run.events)[0].plain, "First part of the note second bit here last words.");
    let polished = polished(&run.events);
    assert_eq!(polished.len(), 1, "one on_polished for the whole dictation: {:?}", run.events);
    assert_eq!(polished[0].plain, "First part of the note! Second bit here! Last words!");
    assert_eq!(polished[0].markdown, polished[0].plain);
}

#[test]
fn a_failed_segment_keeps_its_layer1_text() {
    let (h, llm) = long_engine(FakeLlm::replying(|t| {
        if t.starts_with("Second") {
            Reply { delay: Duration::from_millis(20), answer: Err("rate limited".into()) }
        } else {
            exclaim(t)
        }
    }));
    let run = dictate(&h, &LONG);
    assert_eq!(llm.calls().len(), 3);
    let polished = polished(&run.events);
    assert_eq!(polished.len(), 1);
    assert_eq!(polished[0].plain, "First part of the note! Second bit here. Last words!");
}

#[test]
fn when_every_part_fails_there_is_no_on_polished() {
    let (h, llm) = long_engine(FakeLlm::failing(Duration::from_millis(20)));
    let run = dictate(&h, &LONG);
    assert_eq!(llm.calls().len(), 3);
    assert!(polished(&run.events).is_empty(), "the instant text stands: {:?}", run.events);
}

#[test]
fn a_dictation_under_20_s_is_polished_once_as_a_whole() {
    let short = [(5.0, 1.0), (2.0, 1.0), (1.0, 0.0)];
    let llm = Arc::new(FakeLlm::replying(exclaim));
    let h = Harness::with_audio(
        stt(&short, &[FIRST, SECOND, LAST]),
        LanguageMode::English,
        on(),
        llm.clone(),
        waveform(&short),
        SPEED,
    );
    let run = dictate(&h, &short);
    let log = llm.log();
    assert_eq!(log.len(), 1, "pauses before 20 s close no segment: {log:?}");
    assert_eq!(log[0].transcript, "First part of the note second bit here last words.");
    assert!(log[0].at >= run.released && log[0].context.is_none());
    assert_eq!(polished(&run.events)[0].plain, "First part of the note second bit here last words!");
}

#[test]
fn cancel_mid_dictation_drops_the_segments() {
    let (h, llm) = long_engine(FakeLlm::replying(exclaim));
    h.engine.start_dictation(TargetContext::default()).expect("start_dictation");
    llm.wait_for_call(); // the first segment is being polished
    h.engine.cancel_dictation();
    let events = h.events_within(Duration::from_secs(3));
    assert!(polished(&events).is_empty(), "no on_polished after cancel: {events:?}");
    assert!(finals(&events).is_empty());
    assert_eq!(states(&events).last(), Some(&DictationState::Idle));
}
