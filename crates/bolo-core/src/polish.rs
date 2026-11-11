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

use crate::{ApiKeys, BoloError, EngineEvents, Usage};

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

/// How long a polish request may run on in the background after the engine stopped
/// waiting for it, so the tokens it was billed for still reach the usage meter.
const METERED_TIMEOUT: Duration = Duration::from_secs(30);

/// Claude via the Messages API (`bolo_format::llm`). Reports each answered call's
/// tokens, as Anthropic counted them, to the host's usage meter.
pub(crate) struct AnthropicClient {
    http: reqwest::Client,
    key: String,
    meter: Arc<dyn EngineEvents>,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    /// The engine stops waiting at its deadline, but the request runs on in its own task
    /// (up to [`METERED_TIMEOUT`]): Anthropic bills an answer that came too late, so its
    /// usage is still recorded. `_timeout` is therefore not applied to the request.
    async fn polish(&self, req: &CleanupRequest<'_>, _timeout: Duration) -> Result<String, LlmError> {
        let (http, key, meter) = (self.http.clone(), self.key.clone(), self.meter.clone());
        let owned = Owned::of(req);
        let call = tokio::spawn(async move {
            let req = owned.request();
            llm::cleanup_metered(&http, &key, &req, METERED_TIMEOUT, |r| meter.on_usage(usage(r))).await
        });
        match call.await {
            Ok(r) => r.map(|r| r.text),
            Err(e) => Err(LlmError::Network(e.to_string())),
        }
    }
}

/// A [`CleanupRequest`] that can move into a task.
struct Owned {
    transcript: String,
    script: llm::Script,
    tone: String,
    app: String,
    dictionary: Vec<String>,
    context: Option<String>,
}

impl Owned {
    fn of(r: &CleanupRequest<'_>) -> Self {
        Self {
            transcript: r.transcript.to_owned(),
            script: r.script,
            tone: r.tone.to_owned(),
            app: r.app.to_owned(),
            dictionary: r.dictionary.to_vec(),
            context: r.context.map(str::to_owned),
        }
    }

    fn request(&self) -> CleanupRequest<'_> {
        CleanupRequest {
            transcript: &self.transcript,
            script: self.script,
            tone: &self.tone,
            app: &self.app,
            dictionary: &self.dictionary,
            context: self.context.as_deref(),
        }
    }
}

/// The usage meter's record of one answered call.
fn usage(r: &llm::CleanupResult) -> Usage {
    Usage::Polish {
        model: llm::MODEL.into(),
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cache_read_tokens: r.cache_read_tokens,
        cache_write_tokens: r.cache_write_tokens,
        cost_usd: r.cost_usd(),
    }
}

/// The client for `polish`: none when it is off, the injected one (tests), or Claude
/// with the key from `keys`. Polish on without a key is a configuration error.
pub(crate) fn client(
    polish: &Polish,
    keys: &ApiKeys,
    injected: Option<&Arc<dyn LlmClient>>,
    http: &reqwest::Client,
    meter: &Arc<dyn EngineEvents>,
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
    Ok(Some(Arc::new(AnthropicClient { http: http.clone(), key: key.to_owned(), meter: meter.clone() })))
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

    struct NoEvents;
    impl EngineEvents for NoEvents {
        fn on_level(&self, _: f32) {}
        fn on_partial(&self, _: String) {}
        fn on_final(&self, _: crate::FormattedOutput) {}
        fn on_polished(&self, _: crate::FormattedOutput) {}
        fn on_error(&self, _: BoloError) {}
        fn on_state(&self, _: crate::DictationState) {}
    }

    #[test]
    fn on_without_a_key_is_a_config_error() {
        let meter: Arc<dyn EngineEvents> = Arc::new(NoEvents);
        let on = Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] };
        let http = bolo_stt::cloud::http_client().expect("http client");
        let mut keys = ApiKeys::default();
        assert!(matches!(client(&on, &keys, None, &http, &meter), Err(BoloError::Config(_))));
        assert!(matches!(client(&Polish::Off, &keys, None, &http, &meter), Ok(None)));
        keys.set(ANTHROPIC, "sk-test");
        assert!(matches!(client(&on, &keys, None, &http, &meter), Ok(Some(_))));
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
