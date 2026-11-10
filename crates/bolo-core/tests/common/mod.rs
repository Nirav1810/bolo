//! Shared harness for engine tests: a scripted fake STT engine, a replayed waveform and an
//! event recorder. Everything runs in-process with no models and no network, so it is safe
//! for `cargo test --workspace` in CI.

// Each test binary uses a different subset of this module.
#![allow(dead_code)]

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bolo_core::{
    BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput, Language, LanguageMode,
    TargetContext, WavSource,
};
use bolo_stt::{SttEngine, SttOptions, SttStream, Transcript};

pub const ENGINE_ID: &str = "fake";
/// The fake stream reveals its next scripted partial after every this-many samples (0.1 s).
const SAMPLES_PER_PARTIAL: usize = 1600;
/// How long to wait for the engine thread before failing a test.
const EVENT_TIMEOUT: Duration = Duration::from_secs(10);
/// Length of the replayed waveform.
pub const AUDIO_SECS: f32 = 0.5;

/// What one stream language says: partials in order, then the final text.
#[derive(Debug, Clone, Default)]
pub struct Script {
    pub partials: Vec<String>,
    pub final_text: String,
    /// A chunk decode that blocks `accept` for this long once the stream has taken in
    /// this many samples, like a slow real decode.
    pub slow_decode: Option<(usize, Duration)>,
}

/// A streaming STT engine that returns preset text per stream language. Like Nemotron,
/// it takes a per-stream language, so the Auto language mode runs two passes on it.
#[derive(Default)]
pub struct FakeStt {
    scripts: Mutex<Vec<(Language, Script)>>,
    opened: Mutex<Vec<Language>>,
    /// (language, samples accepted) of every stream that finished.
    finished: Arc<Mutex<Vec<(Language, usize)>>>,
}

impl FakeStt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script what a stream opened with `language` returns.
    pub fn script(self, language: Language, partials: &[&str], final_text: &str) -> Self {
        self.set_script(language, partials, final_text);
        self
    }

    /// Re-script between dictations (streams opened from now on use it).
    pub fn set_script(&self, language: Language, partials: &[&str], final_text: &str) {
        let script = Script {
            partials: partials.iter().map(|p| (*p).to_owned()).collect(),
            final_text: final_text.into(),
            slow_decode: None,
        };
        if let Ok(mut g) = self.scripts.lock() {
            g.retain(|(l, _)| *l != language);
            g.push((language, script));
        }
    }

    /// Make the `language` stream's decode block `accept` for `stall` once it has taken in
    /// `at_samples` samples.
    pub fn slow_decode(self, language: Language, at_samples: usize, stall: Duration) -> Self {
        if let Ok(mut g) = self.scripts.lock()
            && let Some((_, script)) = g.iter_mut().find(|(l, _)| *l == language)
        {
            script.slow_decode = Some((at_samples, stall));
        }
        self
    }

    /// (language, samples accepted) of every stream that finished, in order.
    pub fn finished(&self) -> Vec<(Language, usize)> {
        self.finished.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Languages of every stream opened so far, in order.
    pub fn opened(&self) -> Vec<Language> {
        self.opened.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn script_for(&self, language: Language) -> Script {
        let g = self.scripts.lock();
        g.ok().and_then(|g| g.iter().find(|(l, _)| *l == language).map(|(_, s)| s.clone())).unwrap_or_default()
    }
}

#[async_trait]
impl SttEngine for FakeStt {
    fn id(&self) -> &str {
        ENGINE_ID
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn per_stream_language(&self) -> bool {
        true
    }

    async fn transcribe(&self, _audio: &[f32], opts: &SttOptions) -> bolo_stt::Result<Transcript> {
        Ok(transcript(opts.language, self.script_for(opts.language).final_text))
    }

    fn stream(&self, opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        if let Ok(mut g) = self.opened.lock() {
            g.push(opts.language);
        }
        Some(Box::new(FakeStream {
            language: opts.language,
            script: self.script_for(opts.language),
            samples: 0,
            shown: 0,
            finished: self.finished.clone(),
        }))
    }
}

struct FakeStream {
    language: Language,
    script: Script,
    samples: usize,
    shown: usize,
    finished: Arc<Mutex<Vec<(Language, usize)>>>,
}

impl SttStream for FakeStream {
    fn accept(&mut self, samples: &[f32]) {
        let before = self.samples;
        self.samples += samples.len();
        if let Some((at, stall)) = self.script.slow_decode
            && before < at
            && self.samples >= at
        {
            std::thread::sleep(stall);
        }
    }

    fn partial(&mut self) -> Option<String> {
        let due = (self.samples / SAMPLES_PER_PARTIAL).min(self.script.partials.len());
        if self.shown >= due {
            return None;
        }
        self.shown = due;
        Some(self.script.partials[due - 1].clone())
    }

    fn finish(self: Box<Self>) -> bolo_stt::Result<Transcript> {
        if let Ok(mut g) = self.finished.lock() {
            g.push((self.language, self.samples));
        }
        Ok(transcript(self.language, self.script.final_text))
    }
}

fn transcript(language: Language, text: String) -> Transcript {
    let code = match language {
        Language::En => "en",
        Language::Hi => "hi",
        Language::Auto => "auto",
    };
    Transcript { text, engine_id: ENGINE_ID.into(), language: Some(code.into()) }
}

/// Everything the engine reported, in order.
#[derive(Debug, Clone)]
pub enum Ev {
    Level,
    Partial(String),
    Final(FormattedOutput),
    Error(String),
    State(DictationState),
}

struct Recorder(Mutex<Sender<Ev>>);

impl Recorder {
    fn send(&self, ev: Ev) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(ev);
        }
    }
}

impl EngineEvents for Recorder {
    fn on_level(&self, _rms: f32) {
        self.send(Ev::Level);
    }
    fn on_partial(&self, text: String) {
        self.send(Ev::Partial(text));
    }
    fn on_final(&self, out: FormattedOutput) {
        self.send(Ev::Final(out));
    }
    fn on_error(&self, err: BoloError) {
        self.send(Ev::Error(err.to_string()));
    }
    fn on_state(&self, state: DictationState) {
        self.send(Ev::State(state));
    }
}

/// A 220 Hz tone: something non-silent for the "mic" to replay.
pub fn tone(secs: f32) -> Vec<f32> {
    let rate = bolo_audio::SAMPLE_RATE as f32;
    (0..(secs * rate) as usize).map(|i| 0.1 * (std::f32::consts::TAU * 220.0 * i as f32 / rate).sin()).collect()
}

/// An engine on a fake STT and a replayed waveform, plus its event stream.
pub struct Harness {
    pub engine: Engine,
    pub stt: Arc<FakeStt>,
    rx: Receiver<Ev>,
}

impl Harness {
    pub fn new(stt: FakeStt, language: LanguageMode) -> Self {
        let config = EngineConfig {
            engines: Vec::new(), // unused: the STT engine is injected
            language,
            mic_warm_secs: 0, // replay the waveform from the start every dictation
            ..Default::default()
        };
        let stt = Arc::new(stt);
        let (tx, rx) = mpsc::channel();
        let engine = Engine::with_stt_engine(
            config,
            Box::new(Recorder(Mutex::new(tx))),
            WavSource::factory(Arc::new(tone(AUDIO_SECS))),
            stt.clone(),
        )
        .expect("engine starts with an injected STT engine");
        Self { engine, stt, rx }
    }

    /// Start a dictation and let the whole waveform play into it.
    pub fn start_and_speak(&self) {
        self.engine.start_dictation(TargetContext::default()).expect("start_dictation");
        std::thread::sleep(Duration::from_secs_f32(AUDIO_SECS + 0.1));
    }

    /// Events up to and including the next return to `Idle`.
    pub fn events_until_idle(&self) -> Vec<Ev> {
        self.events_until_state(DictationState::Idle)
    }

    /// Events up to and including the next change to `state`.
    pub fn events_until_state(&self, state: DictationState) -> Vec<Ev> {
        let deadline = Instant::now() + EVENT_TIMEOUT;
        let mut events = Vec::new();
        loop {
            match self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(ev) => {
                    let reached = matches!(ev, Ev::State(s) if s == state);
                    events.push(ev);
                    if reached {
                        return events;
                    }
                }
                Err(RecvTimeoutError::Timeout) => panic!("engine never reached {state:?}; got {events:?}"),
                Err(RecvTimeoutError::Disconnected) => panic!("engine thread exited; got {events:?}"),
            }
        }
    }

    /// Events that arrive within `wait` (for asserting that nothing more happens).
    pub fn events_within(&self, wait: Duration) -> Vec<Ev> {
        let deadline = Instant::now() + wait;
        let mut events = Vec::new();
        while let Ok(ev) = self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            events.push(ev);
        }
        events
    }

    /// One complete dictation: start, speak, stop. Returns every event it produced.
    pub fn dictate(&self) -> Vec<Ev> {
        self.start_and_speak();
        self.engine.stop_dictation().expect("stop_dictation");
        self.events_until_idle()
    }
}

pub fn states(events: &[Ev]) -> Vec<DictationState> {
    events.iter().filter_map(|e| if let Ev::State(s) = e { Some(*s) } else { None }).collect()
}

pub fn finals(events: &[Ev]) -> Vec<FormattedOutput> {
    events.iter().filter_map(|e| if let Ev::Final(f) = e { Some(f.clone()) } else { None }).collect()
}

pub fn partials(events: &[Ev]) -> Vec<String> {
    events.iter().filter_map(|e| if let Ev::Partial(p) = e { Some(p.clone()) } else { None }).collect()
}

pub fn errors(events: &[Ev]) -> Vec<String> {
    events.iter().filter_map(|e| if let Ev::Error(m) = e { Some(m.clone()) } else { None }).collect()
}
