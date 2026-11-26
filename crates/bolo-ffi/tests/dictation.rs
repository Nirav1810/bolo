//! A whole dictation through the `Bolo` object with audio pushed in as PCM16 frames, the
//! way Kotlin drives it, on a fake speech engine (no model, no network).

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bolo_ffi::{Bolo, BoloConfig, BoloListener, Dictation, ErrorKind, Language, State};
use bolo_stt::{SttEngine, SttOptions, Transcript};

/// Returns fixed text for any audio, and records how much audio it was given.
struct FakeStt {
    heard: Arc<Mutex<usize>>,
}

#[async_trait]
impl SttEngine for FakeStt {
    fn id(&self) -> &str {
        "fake"
    }
    async fn transcribe(&self, audio: &[f32], _opts: &SttOptions) -> bolo_stt::Result<Transcript> {
        *self.heard.lock().unwrap() += audio.len();
        Ok(Transcript { text: "testing one two three".into(), engine_id: "fake".into(), ..Transcript::default() })
    }
}

enum Event {
    Final(Dictation),
    Error(ErrorKind, String),
    State(State),
}

struct Recorder(Mutex<Sender<Event>>);

impl BoloListener for Recorder {
    fn on_level(&self, _rms: f32) {}
    fn on_partial(&self, _text: String) {}
    fn on_final(&self, d: Dictation) {
        let _ = self.0.lock().unwrap().send(Event::Final(d));
    }
    fn on_polished(&self, _d: Dictation) {}
    fn on_state(&self, s: State) {
        let _ = self.0.lock().unwrap().send(Event::State(s));
    }
    fn on_error(&self, kind: ErrorKind, message: String) {
        let _ = self.0.lock().unwrap().send(Event::Error(kind, message));
    }
}

fn config() -> BoloConfig {
    BoloConfig {
        models_dir: std::env::temp_dir().join("bolo-ffi-test-models").to_string_lossy().into_owned(),
        engines: vec!["fake".into()],
        language: Language::English,
        threads: None,
    }
}

fn bolo(heard: &Arc<Mutex<usize>>) -> (Arc<Bolo>, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let stt = Arc::new(FakeStt { heard: heard.clone() });
    let bolo = Bolo::with_stt(config(), Arc::new(Recorder(Mutex::new(tx))), stt).expect("engine starts");
    (bolo, rx)
}

/// 20 ms of a quiet tone as little-endian PCM16 bytes, as `AudioRecord` fills a ByteArray.
fn frame(n: usize) -> Vec<u8> {
    (0..320).flat_map(|i| ((((n * 320 + i) as f32 * 0.05).sin() * 3000.0) as i16).to_le_bytes()).collect()
}

#[test]
fn pushed_frames_become_text() {
    let heard = Arc::new(Mutex::new(0));
    let (bolo, rx) = bolo(&heard);

    bolo.start("com.example.notes".into()).expect("starts");
    assert_eq!(bolo.state(), State::Listening);
    for n in 0..50 {
        bolo.push_audio(frame(n));
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(100));
    bolo.stop().expect("stops");

    let d = loop {
        match rx.recv_timeout(Duration::from_secs(10)).expect("an event within 10 s") {
            Event::Final(d) => break d,
            Event::Error(kind, msg) => panic!("dictation failed: {kind:?} {msg}"),
            Event::State(_) => {}
        }
    };
    assert_eq!(d.plain, "Testing one two three.");
    assert_eq!(d.engine_id, "fake");
    assert_eq!(d.language, Language::English);
    assert!(*heard.lock().unwrap() > 0, "the engine got the pushed audio");
    assert_eq!(bolo.dropped_samples(), 0);
}

#[test]
fn audio_pushed_before_start_is_not_heard() {
    let heard = Arc::new(Mutex::new(0));
    let (bolo, rx) = bolo(&heard);
    // A second of audio while no dictation has opened the mic.
    for n in 0..50 {
        bolo.push_audio(frame(n));
    }
    bolo.start("com.example.notes".into()).expect("starts");
    bolo.stop().expect("stops");
    // Wait for the dictation to end one way or the other.
    while let Ok(event) = rx.recv_timeout(Duration::from_secs(10)) {
        if matches!(event, Event::Final(_) | Event::Error(..) | Event::State(State::Done | State::Idle)) {
            break;
        }
    }
    assert!(*heard.lock().unwrap() < 16_000, "idle audio leaked into the dictation");
}
