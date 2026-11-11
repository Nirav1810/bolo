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
/// Silence appended before `input_finished` so a streaming model flushes its last chunk,
/// when the model's [`Chunking`] could not be probed.
const ONLINE_TAIL_PADDING_SECS: f32 = 0.6;

#[derive(Clone)]
enum Loaded {
    Online(Arc<OnlineRecognizer>, Option<Chunking>),
    Offline(Arc<OfflineRecognizer>),
}

/// When a streaming model decodes, in samples: the first chunk is ready once `first`
/// samples are in, then one more every `step`. `first - step` is the model's lookahead.
///
/// Measured once per load by feeding silence, because sherpa-onnx doesn't expose it.
/// Nemotron 320 ms: first ≈ 6840, step 5120.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Chunking {
    first: usize,
    step: usize,
}

impl Chunking {
    /// Feature frames are 10 ms apart, so chunk steps are whole frames.
    const FRAME: usize = (SR / 100) as usize;
    /// Audio past the end of speech that must also be decoded: the transducer emits a
    /// word's last tokens a while after hearing them. With less, the 40 clips lost final
    /// syllables ("shopping li" for "shopping list"); 320 ms was enough when the
    /// language was right, 480 ms also for a pass forced to the wrong one (BENCH.md §10).
    const MARGIN: usize = 48 * Self::FRAME;

    fn probe(rec: &OnlineRecognizer) -> Option<Self> {
        const PIECE: usize = 16; // 1 ms: the precision of `first`
        let stream = rec.create_stream();
        let silence = [0.0f32; PIECE];
        let mut ready_at = Vec::new();
        let mut fed = 0;
        while ready_at.len() < 2 && fed < 5 * SR as usize {
            stream.accept_waveform(SR, &silence);
            fed += PIECE;
            if rec.is_ready(&stream) {
                ready_at.push(fed);
                while rec.is_ready(&stream) {
                    rec.decode(&stream);
                }
            }
        }
        let [first, second] = ready_at[..] else { return None };
        let step = ((second - first) as f64 / Self::FRAME as f64).round() as usize * Self::FRAME;
        (step > 0).then_some(Self { first, step })
    }

    /// Silence to append after `samples` of audio so that the chunk holding the end of
    /// speech plus [`Self::MARGIN`] is decoded, and nothing more. Chunk k is ready at
    /// `first + k * step` and covers the samples before `(k + 1) * step`. When the user
    /// paused before releasing, that chunk was usually decoded live and nothing is left.
    ///
    /// The fixed 0.6 s this replaces cost Nemotron 320 ms two chunk decodes after
    /// release (~100 ms each), even after a pause.
    fn tail_padding(self, samples: usize, speech_end: usize) -> usize {
        if speech_end == 0 {
            return 0; // silence throughout
        }
        let needed = speech_end.min(samples) + Self::MARGIN;
        let last_chunk = needed.div_ceil(self.step) - 1;
        (self.first + last_chunk * self.step).saturating_sub(samples)
    }
}

fn tail_padding(chunking: Option<Chunking>, samples: usize, speech_end: usize) -> usize {
    chunking.map_or((ONLINE_TAIL_PADDING_SECS * SR as f32) as usize, |c| c.tail_padding(samples, speech_end))
}

/// Where a stream's audio last had speech-like energy, so release needn't decode the
/// silence after it. A 10 ms frame counts if its RMS is at least 0.5% of the loudest
/// frame so far (−46 dB): word endings fade through −30 to −55 dB, and −26 dB cut them.
/// In a noisy room every frame counts, which costs latency, not words.
#[derive(Debug, Default)]
struct SpeechEnd {
    frame: Vec<f32>,
    seen: usize,
    peak: f32,
    end: usize,
}

impl SpeechEnd {
    const RELATIVE: f32 = 0.005;

    fn voiced(&self, rms: f32) -> bool {
        rms > 0.0 && rms >= Self::RELATIVE * self.peak
    }

    fn push(&mut self, mut samples: &[f32]) {
        while !samples.is_empty() {
            let take = (Chunking::FRAME - self.frame.len()).min(samples.len());
            self.frame.extend_from_slice(&samples[..take]);
            samples = &samples[take..];
            self.seen += take;
            if self.frame.len() == Chunking::FRAME {
                let rms = bolo_audio::rms(&self.frame);
                self.peak = self.peak.max(rms);
                if self.voiced(rms) {
                    self.end = self.seen;
                }
                self.frame.clear();
            }
        }
    }

    /// The sample just after the last voiced frame. A trailing part-frame counts too.
    fn end(&self) -> usize {
        if !self.frame.is_empty() && self.voiced(bolo_audio::rms(&self.frame)) { self.seen } else { self.end }
    }
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
            Loaded::Online(rec, chunking) => {
                let stream = self.new_online_stream(&rec, opts);
                stream.accept_waveform(SR, audio);
                finish_online(&rec, &stream, tail_padding(chunking, audio.len(), audio.len()))
            }
        };
        Ok(Transcript {
            text: text.trim().to_owned(),
            engine_id: self.id.clone(),
            language: None,
            ..Default::default()
        })
    }
}

fn finish_online(rec: &OnlineRecognizer, stream: &OnlineStream, padding: usize) -> String {
    let t = Instant::now();
    stream.accept_waveform(SR, &vec![0.0f32; padding]);
    stream.input_finished();
    let mut decodes = 0;
    while rec.is_ready(stream) {
        rec.decode(stream);
        decodes += 1;
    }
    tracing::debug!(padding, decodes, ms = t.elapsed().as_secs_f64() * 1e3, "tail decoded");
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
            let rec = OnlineRecognizer::create(&c).ok_or_else(fail)?;
            let chunking = Chunking::probe(&rec);
            tracing::debug!(model = %entry.id, ?chunking, "streaming chunk geometry");
            Ok(Loaded::Online(Arc::new(rec), chunking))
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

    fn per_stream_language(&self) -> bool {
        self.0.entry.streaming && self.0.entry.options.contains_key("stream_language_option")
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
        let Ok(Loaded::Online(rec, chunking)) = self.0.loaded() else { return None };
        let stream = self.0.new_online_stream(&rec, opts);
        Some(Box::new(LocalStream {
            rec,
            stream,
            chunking,
            samples: 0,
            speech: SpeechEnd::default(),
            last: String::new(),
            id: self.0.id.clone(),
        }))
    }
}

struct LocalStream {
    rec: Arc<OnlineRecognizer>,
    stream: OnlineStream,
    chunking: Option<Chunking>,
    /// Samples accepted so far.
    samples: usize,
    speech: SpeechEnd,
    last: String,
    id: String,
}

impl SttStream for LocalStream {
    fn accept(&mut self, samples: &[f32]) {
        self.stream.accept_waveform(SR, samples);
        self.samples += samples.len();
        self.speech.push(samples);
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
        let text = finish_online(&self.rec, &self.stream, tail_padding(self.chunking, self.samples, self.speech.end()));
        Ok(Transcript { text: text.trim().to_owned(), engine_id: self.id, language: None, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEMOTRON_320: Chunking = Chunking { first: 6840, step: 5120 };

    /// The sample where chunk `k` becomes ready.
    fn ready(c: Chunking, k: usize) -> usize {
        c.first + k * c.step
    }

    #[test]
    fn tail_padding_decodes_the_chunk_holding_the_end_of_speech_plus_margin() {
        let c = NEMOTRON_320;
        let m = Chunking::MARGIN; // 1.5 chunks
        // Speaking until release at 0.125 s: speech + margin ends in chunk 1.
        assert_eq!(c.tail_padding(2000, 2000), ready(c, 1) - 2000);
        // Exactly at a chunk boundary, and one sample past it.
        assert_eq!(c.tail_padding(2 * c.step - m, 2 * c.step - m), ready(c, 1) - (2 * c.step - m));
        assert_eq!(c.tail_padding(2 * c.step - m + 1, 2 * c.step - m + 1), ready(c, 2) - (2 * c.step - m + 1));
        // Silence throughout: nothing to decode.
        assert_eq!(c.tail_padding(8000, 0), 0);
        assert_eq!(c.tail_padding(0, 0), 0);
    }

    #[test]
    fn a_pause_before_release_needs_less_or_no_tail_decode() {
        let c = NEMOTRON_320;
        // Speech ended at 1.0 s; with the margin that's 1.48 s, in chunk 4 (1.28–1.6 s),
        // which is ready at 1.7075 s.
        assert_eq!(ready(c, 4), 27_320);
        // Released at 1.8 s: chunk 4 was decoded live, so nothing is left.
        assert_eq!(c.tail_padding(28_800, 16_000), 0);
        // Released at 1.25 s: decode up to chunk 4, not up to the chunk that holds
        // release + margin, as when speaking until release.
        assert_eq!(c.tail_padding(20_000, 16_000), ready(c, 4) - 20_000);
        assert_eq!(c.tail_padding(20_000, 20_000), ready(c, 5) - 20_000);
    }

    #[test]
    fn tail_padding_is_at_most_one_chunk_beyond_the_margin() {
        let c = NEMOTRON_320;
        for samples in 1..40_000 {
            let pad = c.tail_padding(samples, samples);
            // Covers the margin and the lookahead, and never a whole extra chunk.
            assert!(pad + c.step >= Chunking::MARGIN + c.first, "{samples} -> {pad}");
            assert!(pad < Chunking::MARGIN + c.first, "{samples} -> {pad}");
        }
    }

    #[test]
    fn without_a_probe_the_fixed_padding_is_used() {
        assert_eq!(tail_padding(None, 1234, 1234), (ONLINE_TAIL_PADDING_SECS * SR as f32) as usize);
        assert_eq!(tail_padding(Some(NEMOTRON_320), 2000, 2000), ready(NEMOTRON_320, 1) - 2000);
    }

    fn tone(samples: usize, amplitude: f32) -> Vec<f32> {
        (0..samples).map(|i| amplitude * (i as f32 * 0.07).sin()).collect()
    }

    #[test]
    fn speech_end_is_where_the_energy_stops() {
        let mut e = SpeechEnd::default();
        e.push(&tone(16_000, 0.2));
        e.push(&vec![0.0; 4_800]);
        assert_eq!(e.end(), 16_000);
        // Quiet room noise (−60 dB) after speech doesn't count.
        let mut e = SpeechEnd::default();
        e.push(&tone(16_000, 0.2));
        e.push(&tone(4_800, 0.0002));
        assert_eq!(e.end(), 16_000);
        // Fed in odd-sized pieces, the answer is the same.
        let mut e = SpeechEnd::default();
        let audio: Vec<f32> = tone(16_000, 0.2).into_iter().chain(std::iter::repeat_n(0.0, 4_800)).collect();
        audio.chunks(37).for_each(|piece| e.push(piece));
        assert_eq!(e.end(), 16_000);
    }

    #[test]
    fn in_a_noisy_room_everything_counts_as_speech() {
        let mut e = SpeechEnd::default();
        e.push(&tone(16_000, 0.05));
        e.push(&tone(4_800, 0.001)); // noise at −34 dB
        assert_eq!(e.end(), 20_800);
        // A trailing part-frame of speech counts.
        e.push(&tone(50, 0.05));
        assert_eq!(e.end(), 20_850);
        assert_eq!(SpeechEnd::default().end(), 0);
    }
}
