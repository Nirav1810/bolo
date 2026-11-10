//! Stop is handled while a chunk decode is still running (M2 ticket 01). Each streaming
//! pass decodes on its own thread, so the engine thread takes Stop at once instead of
//! after the decode it was stuck in.

mod common;

use std::time::{Duration, Instant};

use bolo_core::{DictationState, Language, LanguageMode};
use common::{AUDIO_SECS, FakeStt, Harness, errors, finals};

/// The slow decode starts 0.4 s into the 0.5 s waveform; the harness stops at 0.6 s.
const SLOW_AT: usize = 6400;
const EN_PARTIALS: [&str; 3] = ["please", "please send the", "please send the report"];
const EN_FINAL: &str = "Please send the report.";
/// All of the replayed waveform, in samples.
const WAVEFORM: usize = (AUDIO_SECS * bolo_audio::SAMPLE_RATE as f32) as usize;

#[test]
fn stop_is_taken_while_a_chunk_decode_is_running_and_the_text_is_complete() {
    // A 1 s decode is running when Stop arrives (0.4 s → 1.4 s; Stop at 0.6 s).
    let stt = FakeStt::new().script(Language::En, &EN_PARTIALS, EN_FINAL).slow_decode(
        Language::En,
        SLOW_AT,
        Duration::from_secs(1),
    );
    let h = Harness::new(stt, LanguageMode::English);
    h.start_and_speak();
    let stopped = Instant::now();
    h.engine.stop_dictation().expect("stop_dictation");

    // Serialized, the engine thread would see Stop only when the decode ends (~0.8 s).
    let mut events = h.events_until_state(DictationState::Processing);
    let to_processing = stopped.elapsed();
    assert!(to_processing < Duration::from_millis(300), "Stop waited on the decode: {to_processing:?}");

    events.extend(h.events_until_idle());
    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    let finals = finals(&events);
    assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
    assert_eq!(finals[0].plain, EN_FINAL);
    // Complete: the audio that queued up behind the slow decode reached the stream.
    let finished = h.stt.finished();
    assert!(
        matches!(finished[..], [(Language::En, n)] if n >= WAVEFORM),
        "the stream got the whole waveform: {finished:?}"
    );
}

#[test]
fn auto_does_not_wait_for_a_slow_decode_on_the_pass_it_drops() {
    // The Hindi pass hears English words, so Auto decides English at release. Its
    // decode stalls for 1.5 s just before Stop; serialized, the final waits it out.
    let stt = FakeStt::new()
        .script(Language::En, &EN_PARTIALS, EN_FINAL)
        .script(Language::Hi, &EN_PARTIALS, "प्लीज सेंड द रिपोर्ट")
        .slow_decode(Language::Hi, SLOW_AT, Duration::from_millis(1500));
    let h = Harness::new(stt, LanguageMode::Auto);
    h.start_and_speak();
    let stopped = Instant::now();
    h.engine.stop_dictation().expect("stop_dictation");
    let events = h.events_until_idle();
    let to_final = stopped.elapsed();

    assert!(errors(&events).is_empty(), "unexpected errors: {events:?}");
    let finals = finals(&events);
    assert_eq!(finals.len(), 1, "exactly one final: {events:?}");
    assert_eq!((finals[0].plain.as_str(), finals[0].language), (EN_FINAL, LanguageMode::English));
    // The stall ends ~1.3 s after Stop; the final must not wait for it.
    assert!(to_final < Duration::from_millis(700), "the final waited on the dropped pass: {to_final:?}");
    let finished = h.stt.finished();
    assert!(
        matches!(finished[..], [(Language::En, n)] if n >= WAVEFORM),
        "only the English pass finished, with the whole waveform: {finished:?}"
    );
}
