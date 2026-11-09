//! Dictation state machine and pipeline orchestration, shared by the Windows app and the
//! Android IME (via `bolo-ffi`).
//!
//! ```text
//! start_dictation ─► mic (warm or opened) ─► 16 kHz ─┬─► streaming STT (partials) ─┐
//!                                                     └─► VAD ─► per-segment batch STT ┤
//! stop_dictation ─► flush last chunk / segment ─► romanize (Layer 1) ─► on_final ◄───┘
//! ```
//!
//! All work happens on one engine thread, so every method here returns immediately.
//! Results arrive through [`EngineEvents`].

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};

pub mod audio;
mod build;
mod worker;

pub use audio::{AudioFactory, AudioSource, MicSource, WavSource};
pub use bolo_stt::{HinglishScript, Language};
pub use build::{BuiltStt, build_one, build_stt};

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetContext {
    pub app_id: String,
    pub window_title: Option<String>,
    pub format: OutputFormat,
    /// Code editors / terminals: no sentence capitalization or trailing period.
    pub code_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationState {
    Idle,
    Listening,
    Processing,
    Done,
}

/// Per-stage latency in milliseconds, measured from `stop_dictation`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Timings {
    /// Stop → STT final text.
    pub stt_ms: f64,
    pub rules_ms: f64,
    /// 0 when the LLM layer was skipped.
    pub llm_ms: f64,
    pub format_ms: f64,
    /// Stop → output ready for insertion.
    pub total_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormattedOutput {
    pub plain: String,
    pub markdown: String,
    pub html: Option<String>,
    pub timings: Timings,
    pub engine_id: String,
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

/// Callbacks into the host app, called from the engine thread. Implementations must be
/// cheap and non-blocking: post to the UI thread.
pub trait EngineEvents: Send + Sync {
    /// Input level for the waveform UI, ~30 Hz while listening.
    fn on_level(&self, rms: f32);
    fn on_partial(&self, text: String);
    fn on_final(&self, out: FormattedOutput);
    fn on_error(&self, err: BoloError);
    fn on_state(&self, state: DictationState);
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
    pub language: Language,
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
    #[serde(skip)]
    pub api_keys: ApiKeys,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            engines: vec!["local:nemotron-stream".into()],
            language: Language::Auto,
            script: HinglishScript::Roman,
            dictionary: Vec::new(),
            idle_unload_secs: 30,
            mic_warm_secs: 120,
            models_dir: PathBuf::new(),
            threads: None,
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
        let state = Arc::new(Mutex::new(DictationState::Idle));
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let st = state.clone();
        // The worker (and the mic it opens) lives entirely on the engine thread: audio
        // device handles are not guaranteed to be `Send`.
        let thread = std::thread::Builder::new()
            .name("bolo-engine".into())
            .spawn(move || match Worker::new(config, Arc::from(events), st, factory) {
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

    /// Finish; the result arrives via `on_final` (or `on_error`).
    pub fn stop_dictation(&self) -> Result<(), BoloError> {
        self.send(Cmd::Stop)
    }

    pub fn cancel_dictation(&self) {
        let _ = self.send(Cmd::Cancel);
    }

    /// Unload the model now (Android `onTrimMemory`).
    pub fn release_resources(&self) {
        let _ = self.send(Cmd::Release);
    }

    pub fn update_config(&self, config: EngineConfig) {
        let _ = self.send(Cmd::UpdateConfig(Box::new(config)));
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
}
