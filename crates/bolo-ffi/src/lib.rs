//! UniFFI bindings for the Android app (ADR-0023).
//!
//! Kotlin sees one object, [`Bolo`], and one callback interface, [`BoloListener`]. Audio is
//! recorded by Kotlin (`AudioRecord`, `VOICE_RECOGNITION`) and pushed in with
//! [`Bolo::push_audio`]; the engine drains it through [`push::PushSource`].
//!
//! Every listener call comes from a Rust thread: Kotlin posts it to the main thread.

use std::path::PathBuf;
use std::sync::{Arc, Once};

use bolo_core::{
    BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput, LanguageMode, TargetContext,
};

pub mod push;

use push::PushRing;

uniffi::setup_scaffolding!();

/// Which language a dictation is in (GLOSSARY: Language mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Language {
    Auto,
    English,
    Hinglish,
}

impl From<Language> for LanguageMode {
    fn from(l: Language) -> Self {
        match l {
            Language::Auto => LanguageMode::Auto,
            Language::English => LanguageMode::English,
            Language::Hinglish => LanguageMode::Hinglish,
        }
    }
}

impl From<LanguageMode> for Language {
    fn from(l: LanguageMode) -> Self {
        match l {
            LanguageMode::Auto => Language::Auto,
            LanguageMode::English => Language::English,
            LanguageMode::Hinglish => Language::Hinglish,
        }
    }
}

/// What the engine needs to start. Ticket 04 replaces this with the settings file.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BoloConfig {
    /// Where models live (`noBackupFilesDir/models`).
    pub models_dir: String,
    /// The engine fallback chain, e.g. `["local:moonshine-tiny"]`.
    pub engines: Vec<String>,
    pub language: Language,
    /// Inference threads; `None` for min(4, cores).
    pub threads: Option<i32>,
}

impl BoloConfig {
    fn engine_config(&self) -> EngineConfig {
        EngineConfig {
            engines: self.engines.clone(),
            language: self.language.into(),
            models_dir: PathBuf::from(&self.models_dir),
            threads: self.threads,
            // Android budgets (ADR-0022): unload soon, keep the mic only briefly.
            idle_unload_secs: 20,
            mic_warm_secs: 30,
            ..EngineConfig::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum State {
    Idle,
    Listening,
    Processing,
    Done,
}

impl From<DictationState> for State {
    fn from(s: DictationState) -> Self {
        match s {
            DictationState::Idle => State::Idle,
            DictationState::Listening => State::Listening,
            DictationState::Processing => State::Processing,
            DictationState::Done => State::Done,
        }
    }
}

/// One dictation's text, as `on_final` or `on_polished` delivers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Dictation {
    pub plain: String,
    pub markdown: String,
    pub engine_id: String,
    pub language: Language,
    /// Stop → recognized text.
    pub stt_ms: f64,
    /// Stop → text ready to insert.
    pub total_ms: f64,
}

impl From<FormattedOutput> for Dictation {
    fn from(o: FormattedOutput) -> Self {
        Self {
            plain: o.plain,
            markdown: o.markdown,
            engine_id: o.engine_id,
            language: o.language.into(),
            stt_ms: o.timings.stt_ms,
            total_ms: o.timings.total_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ErrorKind {
    Audio,
    Stt,
    ModelMissing,
    Config,
    Busy,
    Stopped,
}

/// A failed call. (The text is `detail`: Kotlin exceptions already have a `message`.)
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("{detail}")]
    Engine { kind: ErrorKind, detail: String },
}

impl From<BoloError> for FfiError {
    fn from(e: BoloError) -> Self {
        FfiError::Engine { kind: kind_of(&e), detail: e.to_string() }
    }
}

fn kind_of(e: &BoloError) -> ErrorKind {
    match e {
        BoloError::Audio(_) => ErrorKind::Audio,
        BoloError::Stt(_) => ErrorKind::Stt,
        BoloError::ModelMissing(_) => ErrorKind::ModelMissing,
        BoloError::Config(_) => ErrorKind::Config,
        BoloError::Busy => ErrorKind::Busy,
        BoloError::Stopped => ErrorKind::Stopped,
    }
}

/// Engine events, implemented in Kotlin.
#[uniffi::export(with_foreign)]
pub trait BoloListener: Send + Sync {
    /// Input level (RMS, 0..1) for the waveform, ~30 Hz while listening.
    fn on_level(&self, rms: f32);
    /// The words so far; replaces the previous partial.
    fn on_partial(&self, text: String);
    /// The instant text, as soon as recognition finishes.
    fn on_final(&self, dictation: Dictation);
    /// With polish on: the polished text that replaces the last `on_final`.
    fn on_polished(&self, dictation: Dictation);
    fn on_state(&self, state: State);
    fn on_error(&self, kind: ErrorKind, message: String);
}

struct Events(Arc<dyn BoloListener>);

impl EngineEvents for Events {
    fn on_level(&self, rms: f32) {
        self.0.on_level(rms);
    }
    fn on_partial(&self, text: String) {
        self.0.on_partial(text);
    }
    fn on_final(&self, out: FormattedOutput) {
        self.0.on_final(out.into());
    }
    fn on_polished(&self, out: FormattedOutput) {
        self.0.on_polished(out.into());
    }
    fn on_error(&self, err: BoloError) {
        self.0.on_error(kind_of(&err), err.to_string());
    }
    fn on_state(&self, state: DictationState) {
        self.0.on_state(state.into());
    }
}

/// The dictation engine with its audio fed from Kotlin.
#[derive(uniffi::Object)]
pub struct Bolo {
    engine: Engine,
    ring: Arc<PushRing>,
}

#[uniffi::export]
impl Bolo {
    #[uniffi::constructor]
    pub fn new(config: BoloConfig, listener: Arc<dyn BoloListener>) -> Result<Arc<Self>, FfiError> {
        init_logging();
        let ring = PushRing::new();
        let engine = Engine::with_audio_source(config.engine_config(), Box::new(Events(listener)), ring.factory())?;
        tracing::info!(engines = ?config.engines, language = ?config.language, "engine started");
        Ok(Arc::new(Self { engine, ring }))
    }

    /// One frame of mono 16-bit little-endian PCM at 16 kHz, as `AudioRecord.read` fills
    /// a `ByteArray` (~20 ms, 640 bytes). Dropped while the engine's mic is closed.
    pub fn push_audio(&self, pcm16le: Vec<u8>) {
        self.ring.push_pcm16le(&pcm16le);
    }

    /// Load the model ahead of the first dictation (the keyboard was shown).
    pub fn prewarm(&self) {
        self.engine.prewarm();
    }

    /// Start a dictation into the app with this package name. Returns once the mic is
    /// open, so audio pushed from now on is heard.
    pub fn start(&self, app_id: String) -> Result<(), FfiError> {
        let ctx = TargetContext { app_id, ..TargetContext::default() };
        Ok(self.engine.start_dictation(ctx)?)
    }

    /// Finish; the text arrives through `on_final`.
    pub fn stop(&self) -> Result<(), FfiError> {
        Ok(self.engine.stop_dictation()?)
    }

    /// Throw the running dictation (or its polish) away.
    pub fn cancel(&self) {
        self.engine.cancel_dictation();
    }

    /// Unload the model now (`onTrimMemory`).
    pub fn release(&self) {
        self.engine.release_resources();
    }

    pub fn state(&self) -> State {
        self.engine.state().into()
    }

    /// Samples dropped because the engine fell behind, for diagnostics.
    pub fn dropped_samples(&self) -> u64 {
        self.ring.dropped()
    }
}

impl Bolo {
    /// A `Bolo` on an injected speech engine, for tests with no model.
    #[doc(hidden)]
    pub fn with_stt(
        config: BoloConfig,
        listener: Arc<dyn BoloListener>,
        stt: Arc<dyn bolo_stt::SttEngine>,
    ) -> Result<Arc<Self>, FfiError> {
        let ring = PushRing::new();
        let engine = Engine::with_stt_engine(config.engine_config(), Box::new(Events(listener)), ring.factory(), stt)?;
        Ok(Arc::new(Self { engine, ring }))
    }
}

/// Route `tracing` to logcat (tag `bolo`) once per process.
fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(target_os = "android")]
        {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;
            if let Ok(layer) = tracing_android::layer("bolo") {
                let filter = tracing_subscriber::EnvFilter::new("info,bolo=debug");
                let _ = tracing_subscriber::registry().with(filter).with(layer).try_init();
            }
        }
    });
}
