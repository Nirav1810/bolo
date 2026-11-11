//! Sarvam AI Saaras (`POST /speech-to-text`).
//!
//! Verified 2026-11-09: model `saaras:v4` (default), header `api-subscription-key`,
//! multipart `file`, `model`, `mode` (`transcribe|translate|verbatim|translit|codemix`),
//! `language_code` (`unknown` = auto, `hi-IN`, `en-IN`), `keyterms` (JSON array, ≤50 ×
//! 64 chars, v4 only). REST max 30 s per request → we split longer audio.
//! `mode=translit` gives all-Roman Hinglish; `codemix` keeps Hindi words in Devanagari.
//! Docs disagree on whether `mode` works on v4 — validated by bench (ADR-005).

use async_trait::async_trait;
use serde::Deserialize;

use super::{check_status, map_reqwest, warm_connection};
use crate::{ApiKey, Billed, HinglishScript, Language, Result, SttEngine, SttError, SttOptions, Transcript};

pub const DEFAULT_BASE_URL: &str = "https://api.sarvam.ai";
pub const DEFAULT_MODEL: &str = "saaras:v4";
/// Pay-as-you-go ₹0.49/min ≈ ₹29.4/hr ≈ US$0.35/hr (at ~₹84/USD; excl. GST).
pub const PRICE_USD_PER_HOUR: f64 = 0.35;

/// Keep each request under the 30 s REST limit.
const MAX_CHUNK_SAMPLES: usize = 29 * bolo_audio::SAMPLE_RATE as usize;
const MAX_KEYTERMS: usize = 50;
const MAX_KEYTERM_CHARS: usize = 64;

#[derive(Debug, Clone)]
pub struct SarvamEngine {
    client: reqwest::Client,
    key: ApiKey,
    base_url: String,
    model: String,
    id: String,
}

impl SarvamEngine {
    pub fn new(client: reqwest::Client, key: ApiKey) -> Self {
        Self::with_model(client, key, DEFAULT_BASE_URL, DEFAULT_MODEL)
    }

    pub fn with_model(client: reqwest::Client, key: ApiKey, base_url: &str, model: &str) -> Self {
        Self {
            client,
            key,
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            id: format!("sarvam:{model}"),
        }
    }

    fn mode_and_lang(opts: &SttOptions) -> (&'static str, &'static str) {
        match (opts.language, opts.script) {
            (Language::En, _) => ("transcribe", "en-IN"),
            (Language::Hi, HinglishScript::Roman) => ("translit", "hi-IN"),
            (Language::Hi, HinglishScript::Devanagari) => ("codemix", "hi-IN"),
            (Language::Auto, HinglishScript::Roman) => ("translit", "unknown"),
            (Language::Auto, HinglishScript::Devanagari) => ("codemix", "unknown"),
        }
    }

    fn keyterms_json(opts: &SttOptions) -> Option<String> {
        let terms: Vec<&str> = opts
            .dictionary
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty() && t.chars().count() <= MAX_KEYTERM_CHARS)
            .take(MAX_KEYTERMS)
            .collect();
        if terms.is_empty() {
            return None;
        }
        serde_json::to_string(&terms).ok()
    }

    async fn transcribe_chunk(&self, audio: &[f32], opts: &SttOptions) -> Result<SarvamResponse> {
        let (mode, lang) = Self::mode_and_lang(opts);
        let wav = bolo_audio::codec::encode_wav_pcm16(audio);
        let file = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| SttError::InvalidInput(e.to_string()))?;
        let mut form = reqwest::multipart::Form::new()
            .part("file", file)
            .text("model", self.model.clone())
            .text("mode", mode)
            .text("language_code", lang);
        if supports_keyterms(&self.model)
            && let Some(k) = Self::keyterms_json(opts)
        {
            form = form.text("keyterms", k);
        }
        let resp = self
            .client
            .post(format!("{}/speech-to-text", self.base_url))
            .header("api-subscription-key", self.key.expose())
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest)?;
        check_status(resp).await?.json().await.map_err(map_reqwest)
    }
}

/// `keyterms` is a Saaras v4 feature.
fn supports_keyterms(model: &str) -> bool {
    model.starts_with("saaras:v4")
}

#[derive(Deserialize)]
struct SarvamResponse {
    transcript: String,
    language_code: Option<String>,
}

#[async_trait]
impl SttEngine for SarvamEngine {
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
        // Normally the engine splits a dictation on VAD pauses well before 30 s; this is a backstop.
        let mut parts = Vec::new();
        let mut language = None;
        for chunk in audio.chunks(MAX_CHUNK_SAMPLES) {
            let r = self.transcribe_chunk(chunk, opts).await?;
            language = language.or(r.language_code);
            let t = r.transcript.trim().to_owned();
            if !t.is_empty() {
                parts.push(t);
            }
        }
        Ok(Transcript {
            text: parts.join(" "),
            engine_id: self.id.clone(),
            language,
            billed: Billed::audio(audio.len(), PRICE_USD_PER_HOUR),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_hinglish_uses_translit() {
        let o = SttOptions { language: Language::Auto, script: HinglishScript::Roman, ..Default::default() };
        assert_eq!(SarvamEngine::mode_and_lang(&o), ("translit", "unknown"));
        let o = SttOptions { language: Language::En, ..Default::default() };
        assert_eq!(SarvamEngine::mode_and_lang(&o).0, "transcribe");
    }

    #[test]
    fn keyterms_filtered() {
        let o = SttOptions { dictionary: vec!["Bolo".into(), "x".repeat(65), " ".into()], ..Default::default() };
        assert_eq!(SarvamEngine::keyterms_json(&o).as_deref(), Some(r#"["Bolo"]"#));
    }
}
