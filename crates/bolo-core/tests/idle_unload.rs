//! The idle unload: the model is released `idle_unload_secs` after a dictation, and
//! never when that is 0 (the desktop app keeps it loaded so push-to-talk starts at once).

mod common;

use std::time::Duration;

use bolo_core::{EngineConfig, Language, LanguageMode, Polish, WavSource};
use common::{AUDIO_SECS, FakeStt, Harness, config, tone};

fn harness(idle_unload_secs: u32) -> Harness {
    let stt = FakeStt::new().script(Language::En, &[], "hello");
    let config = EngineConfig { idle_unload_secs, ..config(LanguageMode::English, Polish::Off) };
    Harness::with_factory(stt, config, WavSource::factory(std::sync::Arc::new(tone(AUDIO_SECS))))
}

#[test]
fn the_model_is_released_after_the_idle_timeout() {
    let h = harness(1);
    h.dictate();
    assert_eq!(h.stt.released(), 0, "not straight after the dictation");
    std::thread::sleep(Duration::from_millis(1600));
    assert_eq!(h.stt.released(), 1);
}

#[test]
fn zero_keeps_the_model_loaded() {
    let h = harness(0);
    h.dictate();
    std::thread::sleep(Duration::from_millis(1600));
    assert_eq!(h.stt.released(), 0, "an idle unload of 0 never releases");
    // And the next dictation still works.
    h.dictate();
    assert_eq!(h.stt.released(), 0);
}
