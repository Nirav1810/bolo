//! Session state machine and pipeline orchestration, shared by the Windows app and the
//! Android IME (via `bolo-ffi`).
//!
//! Status (M1): public API surface only. The orchestration (capture → VAD → STT →
//! rules → LLM → format) lands in M1 phase 2 once `bolo-audio` capture exists.

use std::fmt;

use serde::{Deserialize, Serialize};

pub use bolo_stt::{HinglishScript, Language};

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
pub enum SessionState {
    Idle,
    Listening,
    Processing,
    Done,
}

/// Per-stage latency in milliseconds.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Timings {
    /// Capture end (hotkey up) → STT final text.
    pub stt_ms: f64,
    pub rules_ms: f64,
    /// 0 when the LLM layer was skipped.
    pub llm_ms: f64,
    pub format_ms: f64,
    /// Capture end → output ready for insertion.
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
    #[error("a session is already running")]
    Busy,
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
}

/// Callbacks into the host app. Called from engine threads; implementations must be
/// cheap and non-blocking (post to the UI thread).
pub trait EngineEvents: Send + Sync {
    /// Input level for the waveform UI, throttled to ~30 Hz.
    fn on_level(&self, rms: f32);
    fn on_partial(&self, text: String);
    fn on_final(&self, out: FormattedOutput);
    fn on_error(&self, err: BoloError);
    fn on_state(&self, state: SessionState);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Fallback chain of engine specs, e.g. `["groq", "local:nemotron-stream"]`.
    pub engines: Vec<String>,
    pub language: Language,
    pub script: HinglishScript,
    pub dictionary: Vec<String>,
    /// Unload the local model after this many idle seconds (30 desktop, 20 Android).
    pub idle_unload_secs: u32,
    pub models_dir: String,
}

pub struct Engine {
    config: EngineConfig,
    events: Box<dyn EngineEvents>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine").field("config", &self.config).finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(config: EngineConfig, events: Box<dyn EngineEvents>) -> Result<Self, BoloError> {
        Ok(Self { config, events })
    }

    /// Load the local model / open the cloud TLS connection.
    pub fn prewarm(&self) {}

    pub fn start_session(&self, _ctx: TargetContext) -> Result<(), BoloError> {
        self.events.on_state(SessionState::Listening);
        Err(BoloError::NotImplemented("start_session (M1 phase 2)"))
    }

    /// Finalize; emits `on_final`.
    pub fn stop_session(&self) -> Result<(), BoloError> {
        Err(BoloError::NotImplemented("stop_session (M1 phase 2)"))
    }

    pub fn cancel_session(&self) {
        self.events.on_state(SessionState::Idle);
    }

    /// Unload the model now (Android `onTrimMemory`).
    pub fn release_resources(&self) {}

    pub fn update_config(&mut self, config: EngineConfig) {
        self.config = config;
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }
}
