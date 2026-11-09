//! Groq Whisper (OpenAI-compatible `/audio/transcriptions`).
//!
//! Verified 2026-11-09 (docs/DECISIONS.md ADR-005): multipart `file` + `model`,
//! optional `language` (ISO-639-1), `prompt` (≤224 tokens), `response_format=json`
//! → `{"text": "..."}`. Billing has a 10 s minimum per request.

use async_trait::async_trait;
use serde::Deserialize;

use super::{check_status, join_terms, map_reqwest, warm_connection};
use crate::{
    ApiKey, HINGLISH_PROMPT_EXAMPLE, HinglishScript, Language, Result, SttEngine, SttError, SttOptions, Transcript,
};

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const DEFAULT_MODEL: &str = "whisper-large-v3-turbo";
/// USD per hour of audio (turbo). Used by bolo-bench cost estimates.
pub const PRICE_USD_PER_HOUR: f64 = 0.04;
pub const MIN_BILLED_SECONDS: f64 = 10.0;

/// Prompt budget. 224 tokens ≈ 600 chars for English-ish text; stay conservative.
const PROMPT_MAX_CHARS: usize = 500;

#[derive(Debug, Clone)]
pub struct GroqEngine {
    client: reqwest::Client,
    key: ApiKey,
    base_url: String,
    model: String,
    id: String,
}

impl GroqEngine {
    pub fn new(client: reqwest::Client, key: ApiKey) -> Self {
        Self::with_model(client, key, DEFAULT_BASE_URL, DEFAULT_MODEL)
    }

    pub fn with_model(client: reqwest::Client, key: ApiKey, base_url: &str, model: &str) -> Self {
        Self {
            client,
            key,
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            id: format!("groq:{model}"),
        }
    }

    fn prompt(opts: &SttOptions) -> Option<String> {
        let mut p = String::new();
        if opts.language != Language::En && opts.script == HinglishScript::Roman {
            p.push_str(HINGLISH_PROMPT_EXAMPLE);
        }
        let budget = PROMPT_MAX_CHARS.saturating_sub(p.len() + 1);
        let terms = join_terms(&opts.dictionary, budget);
        if !terms.is_empty() {
            if !p.is_empty() {
                p.push(' ');
            }
            p.push_str(&terms);
        }
        (!p.is_empty()).then_some(p)
    }
}

#[derive(Deserialize)]
struct GroqResponse {
    text: String,
}

#[async_trait]
impl SttEngine for GroqEngine {
    fn id(&self) -> &str {
        &self.id
    }

    async fn prewarm(&self) -> Result<()> {
        warm_connection(&self.client, &self.base_url).await
    }

    async fn transcribe(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        if audio.is_empty() {
            return Err(SttError::InvalidInput("empty audio".into()));
        }
        // WAV: lowest server-side latency per Groq docs; size is fine for dictation clips
        // (32 KB/s). Revisit Opus/FLAC on mobile networks after bench (ADR-006).
        let wav = bolo_audio::codec::encode_wav_pcm16(audio);
        let file = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| SttError::InvalidInput(e.to_string()))?;
        let mut form = reqwest::multipart::Form::new()
            .part("file", file)
            .text("model", self.model.clone())
            .text("response_format", "json")
            .text("temperature", "0");
        match opts.language {
            Language::En => form = form.text("language", "en"),
            // Forcing `hi` makes Whisper emit Devanagari; for Roman Hinglish we let it
            // auto-detect and bias the script with the prompt instead.
            Language::Hi if opts.script == HinglishScript::Devanagari => form = form.text("language", "hi"),
            _ => {}
        }
        if let Some(p) = Self::prompt(opts) {
            form = form.text("prompt", p);
        }

        let resp = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(self.key.expose())
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest)?;
        let body: GroqResponse = check_status(resp).await?.json().await.map_err(map_reqwest)?;
        Ok(Transcript { text: body.text.trim().to_owned(), engine_id: self.id.clone(), language: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_hinglish_example_only_when_needed() {
        let mut o = SttOptions { language: Language::En, ..Default::default() };
        assert_eq!(GroqEngine::prompt(&o), None);
        o.dictionary = vec!["Bolo".into()];
        assert_eq!(GroqEngine::prompt(&o).as_deref(), Some("Bolo"));
        o.language = Language::Auto;
        let p = GroqEngine::prompt(&o).unwrap_or_default();
        assert!(p.starts_with(HINGLISH_PROMPT_EXAMPLE) && p.ends_with("Bolo"));
    }
}
