//! Dictation state machine and pipeline orchestration, shared by the Windows app and the
//! Android IME (via `bolo-ffi`).
//!
//! ```text
//! start_dictation ─► mic (warm or opened) ─► 16 kHz ─┬─► streaming STT (partials) ─┐
//!                                                     └─► VAD ─► per-segment batch STT ┤
//! stop_dictation ─► flush last chunk / segment ─► Layer 1 (romanize, rules) ─► on_final ◄┘
//!                                                       polish on ─► LLM (1.5 s) ─► on_polished
//! ```
//!
//! With [`Polish::On`] the instant text still goes out first; the polished text follows
//! as `on_polished`, or not at all when polish times out, fails or fails its guard
//! (ADR-0016). Dictations of [`SEGMENT_AFTER`] or longer are polished a segment at a
//! time while the user speaks, so release only waits for the last one.
//!
//! In the Auto [`LanguageMode`] on an engine with a per-stream language option
//! (Nemotron), the streaming STT runs twice, forced to English and to Hindi, and one
//! result is chosen at release (ADR-0014).
//!
//! All work happens on one engine thread, so every method here returns immediately.
//! Streaming decodes run on a thread per pass, so the engine thread never waits on one
//! and takes `stop_dictation` as soon as it's called. Results arrive through
//! [`EngineEvents`].

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};

pub mod audio;
mod build;
mod polish;
mod segment;
mod worker;

pub use audio::{AudioFactory, AudioSource, MicSource, WavSource};
pub use bolo_stt::{HinglishScript, Language};
pub use build::{BuiltStt, build_one, build_stt};
pub use polish::{ANTHROPIC, LlmClient, POLISH_DEADLINE, Polish};
pub use segment::{SEGMENT_AFTER, SEGMENT_PAUSE};

use worker::{Cmd, Worker};

/// Output flavour the target app wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Plain,
    Markdown,
    Html,
}

/// Where the text is going. Desktop: foreground exe (+ window title for browsers).
/// Android: `EditorInfo.packageName` + input type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetContext {
    pub app_id: String,
    pub window_title: Option<String>,
    pub format: OutputFormat,
    /// Code editors / terminals: no sentence capitalization or trailing period.
    pub code_mode: bool,
}

/// Which language a dictation is in (GLOSSARY: Language mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageMode {
    /// Decide once per dictation. Engines with a per-stream language option decode
    /// English and Hindi side by side and Bolo picks one at release; an ambiguous
    /// dictation reuses the previous dictation's language (English the first time).
    /// Other engines use their own multilingual mode (Deepgram `language=multi`).
    #[default]
    Auto,
    /// English only: one pass, never read as Hindi.
    #[serde(alias = "en")]
    English,
    /// Hindi-English mixed speech, written in the configured [`HinglishScript`].
    #[serde(alias = "hi")]
    Hinglish,
}

impl LanguageMode {
    /// The hint for a single STT request in this mode.
    pub fn stt_language(self) -> Language {
        match self {
            Self::Auto => Language::Auto,
            Self::English => Language::En,
            Self::Hinglish => Language::Hi,
        }
    }

    /// The mode a single-language STT hint corresponds to.
    pub fn from_stt_language(language: Language) -> Self {
        match language {
            Language::Auto => Self::Auto,
            Language::En => Self::English,
            Language::Hi => Self::Hinglish,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationState {
    Idle,
    Listening,
    Processing,
    Done,
}

/// Per-stage latency in milliseconds, measured from the `stop_dictation` call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Timings {
    /// Stop → STT final text.
    pub stt_ms: f64,
    /// Layer 1: romanization and the rules pipeline (`bolo_format::rules`).
    pub rules_ms: f64,
    /// The polish call; 0 in `on_final` and whenever polish was skipped.
    pub llm_ms: f64,
    pub format_ms: f64,
    /// Stop → output ready for insertion.
    pub total_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormattedOutput {
    pub plain: String,
    pub markdown: String,
    /// `<p>`, `<br>` and `<ol><li>` with every text node escaped; `Some` only for a
    /// target whose format is [`OutputFormat::Html`].
    pub html: Option<String>,
    pub timings: Timings,
    pub engine_id: String,
    /// The language the text is in: English or Hinglish when Bolo knows (a forced mode,
    /// or Auto's choice), `Auto` when the engine's own multilingual mode decided.
    pub language: LanguageMode,
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum BoloError {
    #[error("microphone unavailable: {0}")]
    Audio(String),
    #[error("speech recognition failed: {0}")]
    Stt(String),
    #[error("model not installed: {0}")]
    ModelMissing(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("a dictation is already running")]
    Busy,
    #[error("the engine has shut down")]
    Stopped,
}

/// Callbacks into the host app, called from the engine's threads. Implementations must
/// be cheap and non-blocking: post to the UI thread.
pub trait EngineEvents: Send + Sync {
    /// Input level for the waveform UI, ~30 Hz while listening.
    fn on_level(&self, rms: f32);
    fn on_partial(&self, text: String);
    /// The instant text (Layer 1), as soon as recognition finishes.
    fn on_final(&self, out: FormattedOutput);
    /// With [`Polish::On`]: the polished text for the last `on_final`, which it replaces.
    /// Both `plain` and `markdown` hold the polished text, `html` (for an HTML target) is
    /// that text through the HTML renderer, and `timings.llm_ms` is the wait for polish
    /// after `on_final`. In a long dictation polished per segment, a segment whose polish
    /// failed keeps its Layer 1 text in the joined result. Never sent when polish timed
    /// out, failed or failed its guard, nor after `cancel_dictation` or the next
    /// `start_dictation`.
    fn on_polished(&self, out: FormattedOutput);
    fn on_error(&self, err: BoloError);
    fn on_state(&self, state: DictationState);
    /// A paid cloud request finished: for the host's usage meter. Once per request that
    /// returned a result; a request dropped at a deadline or cancelled isn't reported.
    fn on_usage(&self, _usage: Usage) {}
}

/// One paid cloud request, as [`EngineEvents::on_usage`] reports it. Costs are
/// estimates at the provider's pay-as-you-go list price.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Usage {
    /// A cloud speech engine transcribed `audio_secs` of audio.
    Speech { engine_id: String, audio_secs: f64, cost_usd: f64 },
    /// Claude polished a dictation, or one segment of a long one.
    Polish {
        model: String,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        cost_usd: f64,
    },
}

impl Usage {
    /// The speech usage of `t`, or `None` for a local engine.
    pub fn of_transcript(t: &bolo_stt::Transcript) -> Option<Self> {
        (!t.billed.is_zero()).then(|| Usage::Speech {
            engine_id: t.engine_id.clone(),
            audio_secs: t.billed.audio_secs,
            cost_usd: t.billed.cost_usd,
        })
    }
}

/// Provider → API key. Never printed, never serialized into settings.
#[derive(Clone, Default)]
pub struct ApiKeys(HashMap<String, String>);

impl ApiKeys {
    pub fn set(&mut self, provider: &str, key: impl Into<String>) {
        let key = key.into();
        if key.trim().is_empty() {
            self.0.remove(provider);
        } else {
            self.0.insert(provider.to_owned(), key.trim().to_owned());
        }
    }
    pub fn get(&self, provider: &str) -> Option<&str> {
        self.0.get(provider).map(String::as_str)
    }
}

impl fmt::Debug for ApiKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.0.keys().map(|k| format!("{k}=<redacted>"))).finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Fallback chain of engine specs: `local:<model-id>`, `deepgram`, `groq[:model]`,
    /// `sarvam[:model]`. The first one is primary.
    pub engines: Vec<String>,
    pub language: LanguageMode,
    pub script: HinglishScript,
    pub dictionary: Vec<String>,
    /// Unload the local model after this many idle seconds (30 desktop, 20 Android).
    pub idle_unload_secs: u32,
    /// Keep the mic open this long after a dictation (ADR-009 hybrid policy). 0 = close
    /// immediately.
    pub mic_warm_secs: u32,
    pub models_dir: PathBuf,
    /// Inference threads for local models (default min(4, cores)).
    pub threads: Option<i32>,
    /// LLM polish after the instant text. `On` needs the `anthropic` key in `api_keys`.
    #[serde(default)]
    pub polish: Polish,
    #[serde(skip)]
    pub api_keys: ApiKeys,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            engines: vec!["local:nemotron-stream".into()],
            language: LanguageMode::Auto,
            script: HinglishScript::Roman,
            dictionary: Vec::new(),
            idle_unload_secs: 30,
            mic_warm_secs: 120,
            models_dir: PathBuf::new(),
            threads: None,
            polish: Polish::Off,
            api_keys: ApiKeys::default(),
        }
    }
}

pub struct Engine {
    tx: Sender<Cmd>,
    state: Arc<Mutex<DictationState>>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine").field("state", &self.state()).finish_non_exhaustive()
    }
}

impl Engine {
    /// Engine on the default microphone.
    pub fn new(config: EngineConfig, events: Box<dyn EngineEvents>) -> Result<Self, BoloError> {
        Self::with_audio_source(config, events, MicSource::factory())
    }

    /// Engine on a custom audio source (tests, replay benchmarks).
    pub fn with_audio_source(
        config: EngineConfig,
        events: Box<dyn EngineEvents>,
        factory: AudioFactory,
    ) -> Result<Self, BoloError> {
        Self::spawn(config, events, factory, None, None)
    }

    /// Engine on a custom audio source and an injected STT engine, which replaces the one
    /// `config.engines` would build (and survives `update_config`). For tests that drive a
    /// whole dictation with no models and no network.
    pub fn with_stt_engine(
        config: EngineConfig,
        events: Box<dyn EngineEvents>,
        factory: AudioFactory,
        stt: Arc<dyn bolo_stt::SttEngine>,
    ) -> Result<Self, BoloError> {
        Self::spawn(config, events, factory, Some(stt), None)
    }

    /// [`Self::with_stt_engine`] plus an injected LLM client for polish, used instead of
    /// Claude (no `anthropic` key needed). For tests of polish with no network.
    pub fn with_llm_client(
        config: EngineConfig,
        events: Box<dyn EngineEvents>,
        factory: AudioFactory,
        stt: Arc<dyn bolo_stt::SttEngine>,
        llm: Arc<dyn LlmClient>,
    ) -> Result<Self, BoloError> {
        Self::spawn(config, events, factory, Some(stt), Some(llm))
    }

    fn spawn(
        config: EngineConfig,
        events: Box<dyn EngineEvents>,
        factory: AudioFactory,
        injected_stt: Option<Arc<dyn bolo_stt::SttEngine>>,
        injected_llm: Option<Arc<dyn LlmClient>>,
    ) -> Result<Self, BoloError> {
        let state = Arc::new(Mutex::new(DictationState::Idle));
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let st = state.clone();
        // The worker (and the mic it opens) lives entirely on the engine thread: audio
        // device handles are not guaranteed to be `Send`.
        let thread = std::thread::Builder::new()
            .name("bolo-engine".into())
            .spawn(move || match Worker::new(config, Arc::from(events), st, factory, injected_stt, injected_llm) {
                Ok(worker) => {
                    let _ = ready_tx.send(Ok(()));
                    worker.run(rx);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| BoloError::Config(e.to_string()))?;
        ready_rx.recv().map_err(|_| BoloError::Stopped)??;
        Ok(Self { tx, state, thread: Some(thread) })
    }

    fn send(&self, cmd: Cmd) -> Result<(), BoloError> {
        self.tx.send(cmd).map_err(|_| BoloError::Stopped)
    }

    /// Load the local model / open the cloud connection (hotkey down, keyboard shown).
    pub fn prewarm(&self) {
        let _ = self.send(Cmd::Prewarm);
    }

    /// Start listening. Returns once the mic is open, so audio from now on is captured.
    pub fn start_dictation(&self, ctx: TargetContext) -> Result<(), BoloError> {
        let (reply, rx) = mpsc::channel();
        self.send(Cmd::Start(ctx, reply))?;
        rx.recv().map_err(|_| BoloError::Stopped)?
    }

    /// Finish; the result arrives via `on_final` (or `on_error`), then `on_polished`
    /// when polish is on and succeeds.
    pub fn stop_dictation(&self) -> Result<(), BoloError> {
        self.send(Cmd::Stop(std::time::Instant::now()))
    }

    /// Drop the dictation in progress, or the polish still running for the last one.
    pub fn cancel_dictation(&self) {
        let _ = self.send(Cmd::Cancel);
    }

    /// Unload the model now (Android `onTrimMemory`).
    pub fn release_resources(&self) {
        let _ = self.send(Cmd::Release);
    }

    /// Replace the whole config. The STT engine is dropped and rebuilt (the local model
    /// reloads) on the next dictation, so use [`Self::set_polish`] when only polish changes.
    pub fn update_config(&self, config: EngineConfig) {
        let _ = self.send(Cmd::UpdateConfig(Box::new(config)));
    }

    /// Change polish alone, from the next dictation on; the model stays loaded. The
    /// desktop app calls this per dictation as the target app's polish decision changes.
    /// A polish already running for the last dictation is unaffected.
    pub fn set_polish(&self, polish: Polish) {
        let _ = self.send(Cmd::SetPolish(polish));
    }

    pub fn state(&self) -> DictationState {
        self.state.lock().map(|g| *g).unwrap_or(DictationState::Idle)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_keys_are_redacted_and_not_serialized() {
        let mut c = EngineConfig::default();
        c.api_keys.set("deepgram", "dg_secret_123");
        assert!(!format!("{c:?}").contains("dg_secret_123"));
        let json = serde_json::to_string(&c).unwrap_or_default();
        assert!(!json.contains("dg_secret_123") && !json.contains("api_keys"));
        c.api_keys.set("deepgram", "  ");
        assert!(c.api_keys.get("deepgram").is_none());
    }

    #[test]
    fn language_mode_settings_round_trip() {
        let parse = |s: &str| serde_json::from_str::<LanguageMode>(s).ok();
        assert_eq!(parse(r#""auto""#), Some(LanguageMode::Auto));
        assert_eq!(parse(r#""english""#), Some(LanguageMode::English));
        assert_eq!(parse(r#""hinglish""#), Some(LanguageMode::Hinglish));
        assert_eq!(parse(r#""en""#), Some(LanguageMode::English));
        assert_eq!(parse(r#""hi""#), Some(LanguageMode::Hinglish));
        assert_eq!(serde_json::to_string(&LanguageMode::Hinglish).ok().as_deref(), Some(r#""hinglish""#));
        for mode in [LanguageMode::Auto, LanguageMode::English, LanguageMode::Hinglish] {
            assert_eq!(LanguageMode::from_stt_language(mode.stt_language()), mode);
        }
    }
}
