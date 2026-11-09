//! Local engines on sherpa-onnx (the only local runtime).
//!
//! Lifecycle: weights load lazily on `prewarm()`/first use and are dropped on
//! `release()`. The app calls `release()` after its idle timeout (30 s desktop, 20 s
//! Android) and on `onTrimMemory`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bolo_models::{Family, ModelEntry};
use sherpa_onnx::{
    OfflineMoonshineModelConfig, OfflineQwen3ASRModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig, OfflineWhisperModelConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
    OnlineTransducerModelConfig,
};

use crate::{Language, Result, SttEngine, SttError, SttOptions, SttStream, Transcript};

const SR: i32 = bolo_audio::SAMPLE_RATE as i32;
/// Silence appended before `input_finished` so streaming models flush the last chunk.
const ONLINE_TAIL_PADDING_SECS: f32 = 0.6;

#[derive(Clone)]
enum Loaded {
    Online(Arc<OnlineRecognizer>),
    Offline(Arc<OfflineRecognizer>),
}

struct Inner {
    id: String,
    entry: ModelEntry,
    models_root: PathBuf,
    num_threads: i32,
    loaded: Mutex<Option<Loaded>>,
}

/// Default inference threads: `min(4, cores)`. Race to idle, then sleep.
pub fn default_threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get().min(4) as i32).unwrap_or(2)
}

/// A local sherpa-onnx engine. Cheap to clone (shared state).
#[derive(Clone)]
pub struct LocalEngine(Arc<Inner>);

impl LocalEngine {
    pub fn new(entry: ModelEntry, models_root: impl Into<PathBuf>, num_threads: Option<i32>) -> Result<Self> {
        Inner::new(entry, models_root.into(), num_threads).map(|i| Self(Arc::new(i)))
    }

    pub fn entry(&self) -> &ModelEntry {
        &self.0.entry
    }

    pub fn is_loaded(&self) -> bool {
        self.0.lock().is_some()
    }

    /// Load the model synchronously (call from a blocking thread). Returns the load
    /// time, or zero if it was already loaded.
    pub fn load_blocking(&self) -> Result<Duration> {
        self.0.load_blocking()
    }

    /// Blocking transcription of a whole buffer (bench, tests).
    pub fn transcribe_blocking(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        self.0.transcribe_blocking(audio, opts)
    }
}

impl Inner {
    fn new(entry: ModelEntry, models_root: PathBuf, num_threads: Option<i32>) -> Result<Self> {
        if entry.family == Family::SileroVad {
            return Err(SttError::InvalidInput(format!("{} is a VAD model, not ASR", entry.id)));
        }
        Ok(Self {
            id: format!("local:{}", entry.id),
            entry,
            models_root,
            num_threads: num_threads.unwrap_or_else(default_threads),
            loaded: Mutex::new(None),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Option<Loaded>> {
        // A poisoned lock only means another thread panicked mid-load; the Option is
        // still coherent, so recover instead of propagating the panic.
        self.loaded.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn load_blocking(&self) -> Result<Duration> {
        let mut guard = self.lock();
        if guard.is_some() {
            return Ok(Duration::ZERO);
        }
        let t0 = Instant::now();
        *guard = Some(build(&self.entry, &self.models_root, self.num_threads)?);
        let dt = t0.elapsed();
        tracing::info!(model = %self.entry.id, ms = dt.as_millis() as u64, "local model loaded");
        Ok(dt)
    }

    fn loaded(&self) -> Result<Loaded> {
        self.load_blocking()?;
        self.lock().clone().ok_or_else(|| SttError::Model("model unloaded concurrently".into()))
    }

    fn language_value(&self, opts: &SttOptions) -> &'static str {
        match opts.language {
            Language::En => "en",
            Language::Hi => "hi",
            Language::Auto => "auto",
        }
    }

    fn new_online_stream(&self, rec: &OnlineRecognizer, opts: &SttOptions) -> OnlineStream {
        let stream = rec.create_stream();
        if let Some(key) = self.entry.options.get("stream_language_option") {
            stream.set_option(key, self.language_value(opts));
        }
        stream
    }

    fn transcribe_blocking(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        if audio.is_empty() {
            return Err(SttError::InvalidInput("empty audio".into()));
        }
        let text = match self.loaded()? {
            Loaded::Offline(rec) => {
                let stream = rec.create_stream();
                stream.accept_waveform(SR, audio);
                rec.decode(&stream);
                stream.get_result().map(|r| r.text).unwrap_or_default()
            }
            Loaded::Online(rec) => {
                let stream = self.new_online_stream(&rec, opts);
                stream.accept_waveform(SR, audio);
                finish_online(&rec, &stream)
            }
        };
        Ok(Transcript { text: text.trim().to_owned(), engine_id: self.id.clone(), language: None })
    }
}

fn finish_online(rec: &OnlineRecognizer, stream: &OnlineStream) -> String {
    let pad = vec![0.0f32; (ONLINE_TAIL_PADDING_SECS * SR as f32) as usize];
    stream.accept_waveform(SR, &pad);
    stream.input_finished();
    while rec.is_ready(stream) {
        rec.decode(stream);
    }
    rec.get_result(stream).map(|r| r.text).unwrap_or_default()
}

fn file(entry: &ModelEntry, root: &Path, role: &str) -> Result<String> {
    let p = entry
        .file(root, role)
        .ok_or_else(|| SttError::Model(format!("{}: manifest has no `{role}` file", entry.id)))?;
    if !p.exists() {
        return Err(SttError::Model(format!("{}: missing {} (download the model first)", entry.id, p.display())));
    }
    Ok(p.to_string_lossy().into_owned())
}

fn build(entry: &ModelEntry, root: &Path, threads: i32) -> Result<Loaded> {
    let f = |role: &str| file(entry, root, role).map(Some);
    let fail = || SttError::Model(format!("sherpa-onnx rejected the config for {}", entry.id));
    match entry.family {
        Family::OnlineTransducer => {
            let mut c = OnlineRecognizerConfig::default();
            c.model_config.transducer =
                OnlineTransducerModelConfig { encoder: f("encoder")?, decoder: f("decoder")?, joiner: f("joiner")? };
            c.model_config.tokens = f("tokens")?;
            c.model_config.num_threads = threads;
            c.decoding_method = Some("greedy_search".into());
            c.enable_endpoint = false; // VAD + hotkey decide segment boundaries
            OnlineRecognizer::create(&c).map(|r| Loaded::Online(Arc::new(r))).ok_or_else(fail)
        }
        family => {
            let mut c = OfflineRecognizerConfig { decoding_method: Some("greedy_search".into()), ..Default::default() };
            let m = &mut c.model_config;
            m.num_threads = threads;
            match family {
                Family::OfflineNemoTransducer => {
                    m.transducer = OfflineTransducerModelConfig {
                        encoder: f("encoder")?,
                        decoder: f("decoder")?,
                        joiner: f("joiner")?,
                    };
                    m.tokens = f("tokens")?;
                    m.model_type = entry.options.get("model_type").cloned();
                }
                Family::OfflineQwen3Asr => {
                    m.qwen3_asr = OfflineQwen3ASRModelConfig {
                        conv_frontend: f("conv_frontend")?,
                        encoder: f("encoder")?,
                        decoder: f("decoder")?,
                        tokenizer: f("tokenizer")?,
                        ..Default::default()
                    };
                }
                Family::OfflineMoonshineV2 => {
                    m.moonshine = OfflineMoonshineModelConfig {
                        encoder: f("encoder")?,
                        merged_decoder: f("merged_decoder")?,
                        ..Default::default()
                    };
                    m.tokens = f("tokens")?;
                }
                Family::OfflineWhisper => {
                    m.whisper = OfflineWhisperModelConfig {
                        encoder: f("encoder")?,
                        decoder: f("decoder")?,
                        language: entry.options.get("whisper_language").cloned(),
                        task: entry.options.get("whisper_task").cloned(),
                        tail_paddings: -1,
                        ..Default::default()
                    };
                    m.tokens = f("tokens")?;
                }
                Family::OnlineTransducer | Family::SileroVad => {
                    return Err(SttError::InvalidInput(format!("{} is not an offline ASR model", entry.id)));
                }
            }
            OfflineRecognizer::create(&c).map(|r| Loaded::Offline(Arc::new(r))).ok_or_else(fail)
        }
    }
}

#[async_trait]
impl SttEngine for LocalEngine {
    fn id(&self) -> &str {
        &self.0.id
    }

    fn supports_streaming(&self) -> bool {
        self.0.entry.streaming
    }

    async fn prewarm(&self) -> Result<()> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.0.load_blocking().map(|_| ()))
            .await
            .map_err(|e| SttError::Model(e.to_string()))?
    }

    fn release(&self) {
        if self.0.lock().take().is_some() {
            tracing::info!(model = %self.0.entry.id, "local model released");
        }
    }

    async fn transcribe(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        let (this, audio, opts) = (self.clone(), audio.to_vec(), opts.clone());
        tokio::task::spawn_blocking(move || this.0.transcribe_blocking(&audio, &opts))
            .await
            .map_err(|e| SttError::Model(e.to_string()))?
    }

    fn stream(&self, opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        let Ok(Loaded::Online(rec)) = self.0.loaded() else { return None };
        let stream = self.0.new_online_stream(&rec, opts);
        Some(Box::new(LocalStream { rec, stream, last: String::new(), id: self.0.id.clone() }))
    }
}

struct LocalStream {
    rec: Arc<OnlineRecognizer>,
    stream: OnlineStream,
    last: String,
    id: String,
}

impl SttStream for LocalStream {
    fn accept(&mut self, samples: &[f32]) {
        self.stream.accept_waveform(SR, samples);
        while self.rec.is_ready(&self.stream) {
            self.rec.decode(&self.stream);
        }
    }

    fn partial(&mut self) -> Option<String> {
        let text = self.rec.get_result(&self.stream).map(|r| r.text).unwrap_or_default();
        if text == self.last {
            return None;
        }
        self.last.clone_from(&text);
        Some(text)
    }

    fn finish(self: Box<Self>) -> Result<Transcript> {
        let text = finish_online(&self.rec, &self.stream);
        Ok(Transcript { text: text.trim().to_owned(), engine_id: self.id, language: None })
    }
}
