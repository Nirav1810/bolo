//! The engine thread. It owns the mic, the STT engine and the current dictation, and is
//! driven by commands from [`crate::Engine`].
//!
//! Every ~10 ms while the mic is open it drains audio, resamples to 16 kHz, and either
//! feeds the dictation or keeps a short pre-roll ring when idle-warm. Timers close the mic
//! after `mic_warm_secs` and unload the model after `idle_unload_secs`.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bolo_audio::resample::StreamResampler;
use bolo_models::Manifest;
use bolo_stt::vad::{Vad, VadConfig};
use bolo_stt::{SttEngine, SttError, SttOptions, SttStream, Transcript};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::audio::{AudioFactory, AudioSource};
use crate::build::{BuiltStt, build_stt};
use crate::{
    BoloError, DictationState, EngineConfig, EngineEvents, FormattedOutput, HinglishScript, TargetContext, Timings,
};

/// Audio kept from just before the hotkey while the mic is warm (seconds).
const PREROLL_SECS: f32 = 0.3;
/// Silence padding around VAD segments so batch models don't clip word edges.
const SEGMENT_PAD_SECS: f32 = 0.15;
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

pub(crate) enum Cmd {
    Prewarm,
    Start(TargetContext, Sender<Result<(), BoloError>>),
    Stop,
    Cancel,
    Release,
    UpdateConfig(Box<EngineConfig>),
    Shutdown,
}

struct Mic {
    source: Box<dyn AudioSource>,
    resampler: StreamResampler,
    preroll: VecDeque<f32>,
    warm_until: Instant,
}

struct Dictation {
    stream: Option<Box<dyn SttStream>>,
    vad: Option<Vad>,
    segments: Vec<JoinHandle<Result<Transcript, SttError>>>,
    audio: Vec<f32>,
    last_partial: String,
    last_level: Instant,
}

pub(crate) struct Worker {
    config: EngineConfig,
    events: Arc<dyn EngineEvents>,
    state: Arc<Mutex<DictationState>>,
    factory: AudioFactory,
    /// Set by `Engine::with_stt_engine`: used instead of building from `config.engines`.
    injected_stt: Option<Arc<dyn SttEngine>>,
    manifest: Manifest,
    http: reqwest::Client,
    rt: Runtime,
    stt: Option<BuiltStt>,
    model_loaded: bool,
    unload_at: Option<Instant>,
    mic: Option<Mic>,
    dictation: Option<Dictation>,
}

fn pad(seg: &[f32]) -> Vec<f32> {
    let n = (SEGMENT_PAD_SECS * bolo_audio::SAMPLE_RATE as f32) as usize;
    let mut v = Vec::with_capacity(seg.len() + 2 * n);
    v.extend(std::iter::repeat_n(0.0, n));
    v.extend_from_slice(seg);
    v.extend(std::iter::repeat_n(0.0, n));
    v
}

impl Worker {
    pub(crate) fn new(
        config: EngineConfig,
        events: Arc<dyn EngineEvents>,
        state: Arc<Mutex<DictationState>>,
        factory: AudioFactory,
        injected_stt: Option<Arc<dyn SttEngine>>,
    ) -> Result<Self, BoloError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("bolo-rt")
            .enable_all()
            .build()
            .map_err(|e| BoloError::Config(e.to_string()))?;
        let manifest = Manifest::bundled().map_err(|e| BoloError::Config(e.to_string()))?;
        let http = bolo_stt::cloud::http_client().map_err(|e| BoloError::Config(e.to_string()))?;
        Ok(Self {
            config,
            events,
            state,
            factory,
            injected_stt,
            manifest,
            http,
            rt,
            stt: None,
            model_loaded: false,
            unload_at: None,
            mic: None,
            dictation: None,
        })
    }

    fn set_state(&self, s: DictationState) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
        self.events.on_state(s);
    }

    fn opts(&self) -> SttOptions {
        SttOptions {
            language: self.config.language,
            script: self.config.script,
            dictionary: self.config.dictionary.clone(),
        }
    }

    fn ensure_stt(&mut self) -> Result<Arc<dyn SttEngine>, BoloError> {
        if self.stt.is_none() {
            self.stt = Some(match &self.injected_stt {
                Some(engine) => BuiltStt { engine: engine.clone(), vad_model: None },
                None => build_stt(&self.config, &self.manifest, &self.http)?,
            });
        }
        self.stt.as_ref().map(|b| b.engine.clone()).ok_or_else(|| BoloError::Config("engine unavailable".into()))
    }

    /// Load the local model / open the cloud connection in the background.
    fn prewarm(&mut self) {
        match self.ensure_stt() {
            Ok(engine) => {
                self.model_loaded = true;
                let events = self.events.clone();
                self.rt.spawn(async move {
                    if let Err(e) = engine.prewarm().await {
                        events.on_error(BoloError::Stt(e.to_string()));
                    }
                });
            }
            Err(e) => self.events.on_error(e),
        }
    }

    fn open_mic(&mut self) -> Result<(), BoloError> {
        if self.mic.is_none() {
            let source = (self.factory)()?;
            let resampler = StreamResampler::new(source.sample_rate()).map_err(|e| BoloError::Audio(e.to_string()))?;
            self.mic = Some(Mic { source, resampler, preroll: VecDeque::new(), warm_until: Instant::now() });
        }
        Ok(())
    }

    fn start(&mut self, ctx: TargetContext) -> Result<(), BoloError> {
        if self.dictation.is_some() {
            return Err(BoloError::Busy);
        }
        let _ = ctx; // per-app format/tone lands with the formatter (M2/M3)
        let engine = self.ensure_stt()?;
        self.open_mic()?;
        self.model_loaded = true;
        self.unload_at = None;
        // Streaming engines decode as audio arrives; batch engines get VAD segmentation.
        let stream = if engine.supports_streaming() { engine.stream(&self.opts()) } else { None };
        let vad = match (&stream, self.stt.as_ref().and_then(|b| b.vad_model.clone())) {
            (None, Some(model)) => Vad::new(&model, VadConfig::default()).ok(),
            _ => None,
        };
        if stream.is_none() {
            // Cloud: open the TLS connection while the user speaks.
            let e = engine.clone();
            self.rt.spawn(async move { e.prewarm().await });
        }
        let mut dictation = Dictation {
            stream,
            vad,
            segments: Vec::new(),
            audio: Vec::new(),
            last_partial: String::new(),
            last_level: Instant::now(),
        };
        // Warm mic: include what was said just before the hotkey.
        if let Some(mic) = self.mic.as_mut() {
            let pre: Vec<f32> = mic.preroll.drain(..).collect();
            if !pre.is_empty() {
                self.feed(&mut dictation, &pre);
            }
        }
        self.dictation = Some(dictation);
        self.set_state(DictationState::Listening);
        Ok(())
    }

    fn feed(&self, s: &mut Dictation, pcm: &[f32]) {
        s.audio.extend_from_slice(pcm);
        if let Some(stream) = s.stream.as_mut() {
            stream.accept(pcm);
            if let Some(p) = stream.partial()
                && p != s.last_partial
            {
                s.last_partial.clone_from(&p);
                self.events.on_partial(self.romanize(&p));
            }
        } else if let Some(vad) = s.vad.as_mut() {
            vad.accept(pcm);
            for seg in vad.pop_segments() {
                s.segments.push(self.spawn_transcribe(pad(&seg)));
            }
        }
    }

    fn spawn_transcribe(&self, audio: Vec<f32>) -> JoinHandle<Result<Transcript, SttError>> {
        let opts = self.opts();
        let engine = self.stt.as_ref().map(|b| b.engine.clone());
        self.rt.spawn(async move {
            match engine {
                Some(e) => e.transcribe(&audio, &opts).await,
                None => Err(SttError::Model("no engine".into())),
            }
        })
    }

    fn romanize(&self, text: &str) -> String {
        if self.config.script == HinglishScript::Roman && bolo_format::translit::has_devanagari(text) {
            bolo_format::loanwords::to_roman_hinglish(text)
        } else {
            text.to_owned()
        }
    }

    /// Hotkey released: finish recognition and emit `on_final`.
    fn stop(&mut self) {
        let released = Instant::now();
        // Take the last audio that is already buffered.
        self.tick_audio();
        let Some(mut s) = self.dictation.take() else { return };
        self.set_state(DictationState::Processing);
        let mut tail = Vec::new();
        if let Some(mic) = self.mic.as_mut() {
            let _ = mic.resampler.flush(&mut tail);
            mic.warm_until = Instant::now() + Duration::from_secs(u64::from(self.config.mic_warm_secs));
            mic.preroll.clear();
        }
        if !tail.is_empty() {
            self.feed(&mut s, &tail);
        }

        let result: Result<(String, String), BoloError> = if let Some(stream) = s.stream.take() {
            stream.finish().map(|t| (t.text, t.engine_id)).map_err(|e| BoloError::Stt(e.to_string()))
        } else {
            if let Some(vad) = s.vad.as_mut() {
                for seg in vad.flush() {
                    s.segments.push(self.spawn_transcribe(pad(&seg)));
                }
            }
            // No VAD (model missing) or VAD heard nothing but there was sound: send it all.
            if s.segments.is_empty() && (s.vad.is_none() || bolo_audio::rms(&s.audio) > 0.003) {
                s.segments.push(self.spawn_transcribe(s.audio.clone()));
            }
            let handles = std::mem::take(&mut s.segments);
            self.rt.block_on(async move {
                let mut parts = Vec::new();
                let mut engine_id = String::new();
                for h in handles {
                    let t = h
                        .await
                        .map_err(|e| BoloError::Stt(e.to_string()))?
                        .map_err(|e| BoloError::Stt(e.to_string()))?;
                    engine_id = t.engine_id;
                    if !t.text.trim().is_empty() {
                        parts.push(t.text.trim().to_owned());
                    }
                }
                Ok((parts.join(" "), engine_id))
            })
        };
        let stt_ms = released.elapsed().as_secs_f64() * 1e3;

        match result {
            Ok((raw, engine_id)) => {
                let t_rules = Instant::now();
                let text = self.romanize(raw.trim());
                let rules_ms = t_rules.elapsed().as_secs_f64() * 1e3;
                let total_ms = released.elapsed().as_secs_f64() * 1e3;
                self.events.on_final(FormattedOutput {
                    plain: text.clone(),
                    markdown: text,
                    html: None,
                    timings: Timings { stt_ms, rules_ms, llm_ms: 0.0, format_ms: 0.0, total_ms },
                    engine_id,
                });
                self.set_state(DictationState::Done);
            }
            Err(e) => self.events.on_error(e),
        }
        self.set_state(DictationState::Idle);
        self.unload_at = Some(Instant::now() + Duration::from_secs(u64::from(self.config.idle_unload_secs)));
    }

    fn cancel(&mut self) {
        if let Some(s) = self.dictation.take() {
            for h in s.segments {
                h.abort();
            }
            self.set_state(DictationState::Idle);
        }
    }

    fn release(&mut self) {
        if let Some(b) = &self.stt {
            b.engine.release();
        }
        self.model_loaded = false;
        self.unload_at = None;
    }

    /// Drain the mic into the dictation, or into the pre-roll ring when idle-warm.
    fn tick_audio(&mut self) {
        let Some(mic) = self.mic.as_mut() else { return };
        let mut dev = Vec::new();
        mic.source.drain_mono(&mut dev);
        let mut pcm = Vec::new();
        if let Err(e) = mic.resampler.push(&dev, &mut pcm) {
            tracing::warn!(error = %e, "resampler error");
            return;
        }
        let level = mic.source.level();
        if let Some(mut s) = self.dictation.take() {
            if !pcm.is_empty() {
                self.feed(&mut s, &pcm);
            }
            if s.last_level.elapsed() >= LEVEL_INTERVAL {
                s.last_level = Instant::now();
                self.events.on_level(level);
            }
            self.dictation = Some(s);
        } else if let Some(mic) = self.mic.as_mut() {
            let cap = (PREROLL_SECS * bolo_audio::SAMPLE_RATE as f32) as usize;
            mic.preroll.extend(pcm);
            let excess = mic.preroll.len().saturating_sub(cap);
            mic.preroll.drain(..excess);
        }
    }

    fn tick_timers(&mut self) {
        let now = Instant::now();
        if self.dictation.is_none() && self.mic.as_ref().is_some_and(|m| now >= m.warm_until) {
            self.mic = None; // closes the device; Windows mic indicator goes off
            tracing::debug!("mic closed after warm window");
        }
        if self.dictation.is_none() && self.model_loaded && self.unload_at.is_some_and(|t| now >= t) {
            self.release();
            tracing::debug!("model released after idle timeout");
        }
    }

    pub(crate) fn run(mut self, rx: Receiver<Cmd>) {
        loop {
            let wait = if self.dictation.is_some() || self.mic.is_some() { 10 } else { 200 };
            match rx.recv_timeout(Duration::from_millis(wait)) {
                Ok(Cmd::Prewarm) => self.prewarm(),
                Ok(Cmd::Start(ctx, reply)) => {
                    let r = self.start(ctx);
                    if let Err(e) = &r {
                        self.events.on_error(e.clone());
                    }
                    let _ = reply.send(r);
                }
                Ok(Cmd::Stop) => self.stop(),
                Ok(Cmd::Cancel) => self.cancel(),
                Ok(Cmd::Release) => self.release(),
                Ok(Cmd::UpdateConfig(c)) => {
                    // Engines, keys or paths may have changed: drop and rebuild lazily.
                    self.release();
                    self.stt = None;
                    self.config = *c;
                }
                Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick_audio();
            self.tick_timers();
        }
        self.cancel();
        self.release();
    }
}
