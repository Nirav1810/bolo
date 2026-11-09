//! Turn engine specs ("local:nemotron-stream", "deepgram", "deepgram-stream", "groq:whisper-large-v3")
//! into a ready `SttEngine`, with a fallback chain when several are configured.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bolo_models::Manifest;
use bolo_stt::cloud::{
    DeepgramEngine, DeepgramStreamEngine, GroqEngine, SarvamEngine, deepgram, deepgram_stream, groq, sarvam,
};
use bolo_stt::fallback::{DEFAULT_CLOUD_TIMEOUT, FallbackEngine};
use bolo_stt::local::LocalEngine;
use bolo_stt::{ApiKey, SttEngine};

use crate::{BoloError, EngineConfig};

/// Local engines get a longer budget in a fallback chain (they don't hit the network).
const LOCAL_TIMEOUT: Duration = Duration::from_secs(15);

pub struct BuiltStt {
    pub engine: Arc<dyn SttEngine>,
    /// Silero VAD model path, when installed (enables segment-ahead decoding).
    pub vad_model: Option<PathBuf>,
}

fn key(config: &EngineConfig, provider: &str) -> Result<ApiKey, BoloError> {
    config
        .api_keys
        .get(provider)
        .map(ApiKey::new)
        .ok_or_else(|| BoloError::Config(format!("no API key configured for {provider}")))
}

/// Build one engine from a spec.
pub fn build_one(
    spec: &str,
    config: &EngineConfig,
    manifest: &Manifest,
    http: &reqwest::Client,
) -> Result<Arc<dyn SttEngine>, BoloError> {
    let (kind, arg) = spec.split_once(':').map_or((spec, None), |(k, a)| (k, Some(a)));
    let cfg = |e: &dyn std::fmt::Display| BoloError::Config(e.to_string());
    Ok(match kind {
        "local" => {
            let id = arg.ok_or_else(|| BoloError::Config("use local:<model-id>".into()))?;
            let entry = manifest.get(id).map_err(|e| cfg(&e))?.clone();
            if !entry.is_installed(&config.models_dir) {
                return Err(BoloError::ModelMissing(id.to_owned()));
            }
            Arc::new(LocalEngine::new(entry, &config.models_dir, config.threads).map_err(|e| cfg(&e))?)
        }
        "groq" => {
            let k = key(config, "groq")?;
            Arc::new(GroqEngine::with_model(
                http.clone(),
                k,
                groq::DEFAULT_BASE_URL,
                arg.unwrap_or(groq::DEFAULT_MODEL),
            ))
        }
        "deepgram" => {
            let k = key(config, "deepgram")?;
            Arc::new(DeepgramEngine::with_model(
                http.clone(),
                k,
                deepgram::DEFAULT_BASE_URL,
                arg.unwrap_or(deepgram::DEFAULT_MODEL),
            ))
        }
        "deepgram-stream" => {
            let k = key(config, "deepgram")?;
            Arc::new(
                DeepgramStreamEngine::with_urls(
                    http.clone(),
                    k,
                    deepgram::DEFAULT_BASE_URL,
                    deepgram_stream::DEFAULT_WS_URL,
                    arg.unwrap_or(deepgram::DEFAULT_MODEL),
                )
                .map_err(|e| cfg(&e))?,
            )
        }
        "sarvam" => {
            let k = key(config, "sarvam")?;
            Arc::new(SarvamEngine::with_model(
                http.clone(),
                k,
                sarvam::DEFAULT_BASE_URL,
                arg.unwrap_or(sarvam::DEFAULT_MODEL),
            ))
        }
        other => return Err(BoloError::Config(format!("unknown engine `{other}`"))),
    })
}

pub fn build_stt(config: &EngineConfig, manifest: &Manifest, http: &reqwest::Client) -> Result<BuiltStt, BoloError> {
    if config.engines.is_empty() {
        return Err(BoloError::Config("no STT engine configured".into()));
    }
    let mut chain = Vec::new();
    for spec in &config.engines {
        let timeout = if spec.starts_with("local:") { LOCAL_TIMEOUT } else { DEFAULT_CLOUD_TIMEOUT };
        chain.push((build_one(spec, config, manifest, http)?, timeout));
    }
    let engine: Arc<dyn SttEngine> = if chain.len() == 1 {
        chain.remove(0).0
    } else {
        Arc::new(FallbackEngine::new(chain).map_err(|e| BoloError::Config(e.to_string()))?)
    };
    let vad_model = manifest
        .get("silero-vad")
        .ok()
        .filter(|m| m.is_installed(&config.models_dir))
        .and_then(|m| m.file(&config.models_dir, "model"));
    Ok(BuiltStt { engine, vad_model })
}
