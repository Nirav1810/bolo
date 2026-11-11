//! Deepgram Nova-3 pre-recorded (`POST /v1/listen`).
//!
//! Verified 2026-11-09: raw body + `Content-Type`, `Authorization: Token <key>`,
//! `keyterm` repeated per term (Nova-3 only, ≤500 tokens), `language=multi` covers
//! Hindi code-switching. Transcript at `results.channels[0].alternatives[0].transcript`.

use async_trait::async_trait;
use serde::Deserialize;

use super::{check_status, map_reqwest, warm_connection};
use crate::{ApiKey, Billed, Language, Result, SttEngine, SttError, SttOptions, Transcript};

pub const DEFAULT_BASE_URL: &str = "https://api.deepgram.com/v1";
pub const DEFAULT_MODEL: &str = "nova-3";
/// Pay-as-you-go USD per hour: monolingual $0.0043/min, multilingual $0.0052/min,
/// keyterm add-on $0.0013/min.
pub const PRICE_USD_PER_HOUR_MONO: f64 = 0.258;
pub const PRICE_USD_PER_HOUR_MULTI: f64 = 0.312;
pub const PRICE_USD_PER_HOUR_KEYTERM: f64 = 0.078;
/// Live streaming list prices per hour: monolingual $0.0077/min, multilingual $0.0092/min
/// (Deepgram has run promotions below these; the app labels its costs as estimates).
pub const PRICE_USD_PER_HOUR_STREAM_MONO: f64 = 0.462;
pub const PRICE_USD_PER_HOUR_STREAM_MULTI: f64 = 0.552;

const MAX_KEYTERMS: usize = 50;

#[derive(Debug, Clone)]
pub struct DeepgramEngine {
    client: reqwest::Client,
    key: ApiKey,
    base_url: String,
    model: String,
    id: String,
}

impl DeepgramEngine {
    pub fn new(client: reqwest::Client, key: ApiKey) -> Self {
        Self::with_model(client, key, DEFAULT_BASE_URL, DEFAULT_MODEL)
    }

    pub fn with_model(client: reqwest::Client, key: ApiKey, base_url: &str, model: &str) -> Self {
        Self {
            client,
            key,
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            id: format!("deepgram:{model}"),
        }
    }

    fn query(&self, opts: &SttOptions) -> Vec<(&'static str, String)> {
        listen_params(&self.model, opts)
    }
}

/// `/v1/listen` query parameters shared by the batch and the live (streaming) endpoint.
pub(crate) fn listen_params(model: &str, opts: &SttOptions) -> Vec<(&'static str, String)> {
    let lang = match opts.language {
        Language::En => "en",
        // Hindi output script from Deepgram is undocumented (likely Devanagari);
        // `multi` handles English+Hindi code-switching. Measured in bench.
        Language::Hi | Language::Auto => "multi",
    };
    let mut q = vec![
        ("model", model.to_owned()),
        ("language", lang.to_owned()),
        ("smart_format", "true".to_owned()),
        ("punctuate", "true".to_owned()),
    ];
    for t in opts.dictionary.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).take(MAX_KEYTERMS) {
        q.push(("keyterm", t.to_owned()));
    }
    q
}

/// The list price per hour of audio for a request with `opts`: batch or live, one
/// language or `multi`, plus the keyterm add-on when the dictionary is used.
pub(crate) fn usd_per_hour(opts: &SttOptions, streaming: bool) -> f64 {
    let multi = !matches!(opts.language, Language::En);
    let base = match (streaming, multi) {
        (false, false) => PRICE_USD_PER_HOUR_MONO,
        (false, true) => PRICE_USD_PER_HOUR_MULTI,
        (true, false) => PRICE_USD_PER_HOUR_STREAM_MONO,
        (true, true) => PRICE_USD_PER_HOUR_STREAM_MULTI,
    };
    let keyterms = opts.dictionary.iter().any(|t| !t.trim().is_empty());
    base + if keyterms { PRICE_USD_PER_HOUR_KEYTERM } else { 0.0 }
}

#[derive(Deserialize)]
struct DgResponse {
    results: DgResults,
}
#[derive(Deserialize)]
struct DgResults {
    channels: Vec<DgChannel>,
}
#[derive(Deserialize)]
struct DgChannel {
    alternatives: Vec<DgAlternative>,
    detected_language: Option<String>,
}
#[derive(Deserialize)]
struct DgAlternative {
    transcript: String,
}

#[async_trait]
impl SttEngine for DeepgramEngine {
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
        let wav = bolo_audio::codec::encode_wav_pcm16(audio);
        let resp = self
            .client
            .post(format!("{}/listen", self.base_url))
            .header(reqwest::header::AUTHORIZATION, format!("Token {}", self.key.expose()))
            .header(reqwest::header::CONTENT_TYPE, "audio/wav")
            .query(&self.query(opts))
            .body(wav)
            .send()
            .await
            .map_err(map_reqwest)?;
        let body: DgResponse = check_status(resp).await?.json().await.map_err(map_reqwest)?;
        let channel = body.results.channels.into_iter().next();
        let language = channel.as_ref().and_then(|c| c.detected_language.clone());
        let text = channel.and_then(|c| c.alternatives.into_iter().next()).map(|a| a.transcript).unwrap_or_default();
        Ok(Transcript {
            text: text.trim().to_owned(),
            engine_id: self.id.clone(),
            language,
            billed: Billed::audio(audio.len(), usd_per_hour(opts, false)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_response_shape() {
        let json = r#"{"metadata":{"duration":2.5},"results":{"channels":[{"alternatives":[{"transcript":"hello world","confidence":0.99}]}]}}"#;
        let r: DgResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.results.channels[0].alternatives[0].transcript, "hello world");
    }

    #[test]
    fn keyterms_repeat() {
        let Ok(client) = crate::cloud::http_client() else { return };
        let e = DeepgramEngine::new(client, ApiKey::new("k"));
        let o = SttOptions { dictionary: vec!["Bolo".into(), "Tauri app".into()], ..Default::default() };
        let q = e.query(&o);
        assert_eq!(q.iter().filter(|(k, _)| *k == "keyterm").count(), 2);
        assert!(q.contains(&("language", "multi".to_owned())));
    }

    #[test]
    fn prices_follow_mode_language_and_keyterms() {
        let en = SttOptions { language: Language::En, ..Default::default() };
        let multi = SttOptions::default();
        let terms = SttOptions { dictionary: vec!["Bolo".into()], ..Default::default() };
        assert_eq!(usd_per_hour(&en, false), PRICE_USD_PER_HOUR_MONO);
        assert_eq!(usd_per_hour(&multi, true), PRICE_USD_PER_HOUR_STREAM_MULTI);
        assert_eq!(usd_per_hour(&terms, false), PRICE_USD_PER_HOUR_MULTI + PRICE_USD_PER_HOUR_KEYTERM);
        // A minute of multilingual batch audio is $0.0052.
        let b = Billed::audio(60 * 16_000, usd_per_hour(&multi, false));
        assert_eq!(b.audio_secs, 60.0);
        assert!((b.cost_usd - 0.0052).abs() < 1e-9);
    }

    #[test]
    fn auto_uses_native_multilingual_mode() {
        let Ok(client) = crate::cloud::http_client() else { return };
        let e = DeepgramEngine::new(client, ApiKey::new("k"));
        let lang = |language| {
            e.query(&SttOptions { language, ..Default::default() })
                .into_iter()
                .find(|(k, _)| *k == "language")
                .map(|(_, v)| v)
        };
        assert_eq!(lang(Language::Auto).as_deref(), Some("multi"));
        assert_eq!(lang(Language::Hi).as_deref(), Some("multi"));
        assert_eq!(lang(Language::En).as_deref(), Some("en"));
    }
}
