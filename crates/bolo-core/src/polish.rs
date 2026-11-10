//! Layer 2 polish in the engine (ADR-0016): after `on_final` has delivered the instant
//! text, the engine asks the LLM to clean it up and emits `on_polished` if the answer
//! comes back in time and passes the guard. On a timeout, an error or a failed guard it
//! emits nothing more, and the Layer 1 text stands.
//!
//! The LLM sits behind [`LlmClient`], so engine tests run on a fake with no network.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bolo_format::llm::{self, CleanupRequest, LlmError};
use serde::{Deserialize, Serialize};

use crate::{ApiKeys, BoloError};

/// Polish's hard deadline. A later answer is dropped and the Layer 1 text stands.
pub const POLISH_DEADLINE: Duration = Duration::from_millis(1500);

/// The `ApiKeys` provider polish needs.
pub const ANTHROPIC: &str = "anthropic";

/// Whether a dictation is polished after its instant text (GLOSSARY: Polish).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Polish {
    /// The instant text is the result. The default.
    #[default]
    Off,
    /// Polish with Claude after `on_final`; needs the `anthropic` key in `ApiKeys`.
    On {
        /// e.g. "neutral", "casual", "formal".
        tone: String,
        /// The target app's name, e.g. "WhatsApp", "Gmail".
        app: String,
        /// Preferred spellings (names, jargon).
        #[serde(default)]
        dictionary: Vec<String>,
    },
}

/// The LLM behind polish. The engine enforces the deadline and the guard around it.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// The cleaned-up text for `req`. `timeout` is the time left before the deadline.
    async fn polish(&self, req: &CleanupRequest<'_>, timeout: Duration) -> Result<String, LlmError>;
}

/// Claude via the Messages API (`bolo_format::llm`).
pub(crate) struct AnthropicClient {
    http: reqwest::Client,
    key: String,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn polish(&self, req: &CleanupRequest<'_>, timeout: Duration) -> Result<String, LlmError> {
        llm::cleanup(&self.http, &self.key, req, timeout).await.map(|r| r.text)
    }
}

/// The client for `polish`: none when it is off, the injected one (tests), or Claude
/// with the key from `keys`. Polish on without a key is a configuration error.
pub(crate) fn client(
    polish: &Polish,
    keys: &ApiKeys,
    injected: Option<&Arc<dyn LlmClient>>,
    http: &reqwest::Client,
) -> Result<Option<Arc<dyn LlmClient>>, BoloError> {
    if *polish == Polish::Off {
        return Ok(None);
    }
    if let Some(c) = injected {
        return Ok(Some(c.clone()));
    }
    let key = keys
        .get(ANTHROPIC)
        .ok_or_else(|| BoloError::Config("polish is on but no API key is configured for anthropic".into()))?;
    Ok(Some(Arc::new(AnthropicClient { http: http.clone(), key: key.to_owned() })))
}

/// The guard on an answer: `Some(text)` when it can replace `input`. An empty answer
/// or one that looks like a reply to the user rather than cleaned text fails it.
pub(crate) fn guard(input: &str, answer: &str) -> Option<String> {
    let text = answer.trim();
    (!text.is_empty() && !llm::looks_like_reply(input, text)).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_is_off_by_default_and_round_trips() {
        assert_eq!(Polish::default(), Polish::Off);
        let on: Polish = serde_json::from_str(r#"{"on":{"tone":"casual","app":"WhatsApp"}}"#).unwrap_or_default();
        assert_eq!(on, Polish::On { tone: "casual".into(), app: "WhatsApp".into(), dictionary: vec![] });
        assert_eq!(serde_json::from_str::<Polish>(r#""off""#).ok(), Some(Polish::Off));
    }

    #[test]
    fn on_without_a_key_is_a_config_error() {
        let on = Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] };
        let http = bolo_stt::cloud::http_client().expect("http client");
        let mut keys = ApiKeys::default();
        assert!(matches!(client(&on, &keys, None, &http), Err(BoloError::Config(_))));
        assert!(matches!(client(&Polish::Off, &keys, None, &http), Ok(None)));
        keys.set(ANTHROPIC, "sk-test");
        assert!(matches!(client(&on, &keys, None, &http), Ok(Some(_))));
    }

    #[test]
    fn the_guard_drops_empty_answers_and_replies() {
        assert_eq!(guard("um lets meet at ten", " Let's meet at 10. "), Some("Let's meet at 10.".into()));
        assert_eq!(guard("hi", "   "), None);
        assert_eq!(
            guard(
                "kal milte hain",
                "I can't clean this up because it is in a language I was not told to expect, sorry."
            ),
            None
        );
    }
}
