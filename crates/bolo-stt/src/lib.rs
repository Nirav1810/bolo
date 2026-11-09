//! Speech-to-text engines.
//!
//! Every engine implements [`SttEngine`]. Input audio is always 16 kHz mono `f32` in
//! `[-1, 1]` ([`bolo_audio::SAMPLE_RATE`]); engines that need another format (e.g. cloud
//! uploads) encode internally.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[cfg(feature = "cloud")]
pub mod cloud;
pub mod fallback;
#[cfg(feature = "local")]
pub mod local;

/// Language hint passed to the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    /// Hindi / Hinglish (code-mixed Hindi-English).
    Hi,
    #[default]
    Auto,
}

/// Script used when the output contains Hindi words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HinglishScript {
    /// "mera phone number hai" — default; what people type in chat apps.
    #[default]
    Roman,
    /// "मेरा phone number है"
    Devanagari,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SttOptions {
    pub language: Language,
    pub script: HinglishScript,
    /// User dictionary terms, used as hotwords / prompt / keyterms where supported.
    pub dictionary: Vec<String>,
}

/// A final transcript from one engine call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    /// Engine that produced it (after fallback, this is the engine that succeeded).
    pub engine_id: String,
    /// Language reported by the engine, if any (e.g. `en`, `hi-IN`).
    pub language: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("network error: {0}")]
    Network(String),
    #[error("provider returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("authentication failed (check the API key)")]
    Auth,
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("could not decode provider response: {0}")]
    Decode(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("all engines failed; last error: {0}")]
    AllFailed(Box<SttError>),
}

pub type Result<T, E = SttError> = std::result::Result<T, E>;

/// Incremental recognizer for streaming engines. Not `Sync`: owned by one session thread.
pub trait SttStream: Send {
    /// Feed 16 kHz mono samples. Must be cheap; decoding may happen here or lazily.
    fn accept(&mut self, samples: &[f32]);
    /// Current best partial hypothesis, if it changed since the last call.
    fn partial(&mut self) -> Option<String>;
    /// Flush and return the final transcript.
    fn finish(self: Box<Self>) -> Result<Transcript>;
}

#[async_trait]
pub trait SttEngine: Send + Sync {
    fn id(&self) -> &str;

    fn supports_streaming(&self) -> bool {
        false
    }

    /// Get ready for a session: load the model (local) or open a TLS connection (cloud).
    /// Must be idempotent and cheap when already warm.
    async fn prewarm(&self) -> Result<()> {
        Ok(())
    }

    /// Free heavy resources (model weights). The next call reloads lazily.
    fn release(&self) {}

    async fn transcribe(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript>;

    /// Only for engines where [`Self::supports_streaming`] is true.
    fn stream(&self, _opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        None
    }
}

/// An API key that never prints itself. Keys are passed in by the app (Credential
/// Manager on Windows, Keystore-decrypted on Android) and are never written to disk by
/// this crate.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Short Roman-Hinglish sample used to bias Whisper-style prompts towards Latin script.
pub const HINGLISH_PROMPT_EXAMPLE: &str = "Haan, main kal office aaunga. Meeting ka agenda bhej dena, thanks.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_debug_is_redacted() {
        let k = ApiKey::new("gsk_secret_value");
        assert_eq!(format!("{k:?}"), "ApiKey(<redacted>)");
        assert!(!format!("{k:?}").contains("secret"));
    }
}
