//! The engine thread. It owns the mic, the STT engine and the current dictation, and is
//! driven by commands from [`crate::Engine`].
//!
//! Every ~10 ms while the mic is open it drains audio, resamples to 16 kHz, and either
//! feeds the dictation or keeps a short pre-roll ring when idle-warm. Timers close the mic
//! after `mic_warm_secs` and unload the model after `idle_unload_secs`.
//!
//! Each streaming pass decodes on its own thread, so the engine thread never waits on a
//! chunk decode: it hands audio over, picks up partials, and handles Stop as soon as it
//! arrives. In the Auto language mode on an engine with a per-stream language option, a
//! dictation has two passes, forced to English and to Hindi, fed the same audio.
//! Partials come from whichever pass the language decision currently favours; at
//! release the chosen pass is finished, or both when the decision has to wait for them.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bolo_audio::resample::StreamResampler;
use bolo_format::language::{Detected, detect};
use bolo_format::rules;
use bolo_models::Manifest;
use bolo_stt::vad::{Vad, VadConfig};
use bolo_stt::{Language, SttEngine, SttError, SttOptions, SttStream, Transcript};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::audio::{AudioFactory, AudioSource};
use crate::build::{BuiltStt, build_stt};
use crate::{
    BoloError, DictationState, EngineConfig, EngineEvents, FormattedOutput, HinglishScript, LanguageMode,
    TargetContext, Timings,
};

/// Audio kept from just before the hotkey while the mic is warm (seconds).
const PREROLL_SECS: f32 = 0.3;
/// Silence padding around VAD segments so batch models don't clip word edges.
const SEGMENT_PAD_SECS: f32 = 0.15;
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

pub(crate) enum Cmd {
    Prewarm,
    Start(TargetContext, Sender<Result<(), BoloError>>),
    /// Release, with when `stop_dictation` was called.
    Stop(Instant),
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

/// One streaming decode of a dictation, forced to `language` (or left to the engine's
/// own detection when it is `Auto`). The stream lives on a decoder thread; dropping the
/// pass abandons it (the thread ends after the decode it is in).
struct Pass {
    language: Language,
    audio: Sender<PassMsg>,
    /// Partials from the decoder thread, newest last.
    partials: Receiver<String>,
    /// The finished transcript, after [`PassMsg::Finish`].
    done: Receiver<Result<Transcript, SttError>>,
    /// Latest partial hypothesis, raw.
    partial: String,
}

enum PassMsg {
    Audio(Vec<f32>),
    Finish,
}

impl Pass {
    fn spawn(language: Language, stream: Box<dyn SttStream>) -> Option<Self> {
        let (audio, rx) = mpsc::channel();
        let (partial_tx, partials) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("bolo-pass-{language:?}").to_lowercase())
            .spawn(move || decode_pass(stream, &rx, &partial_tx, &done_tx))
            .map_err(|e| tracing::warn!(error = %e, "could not start a decoder thread"))
            .ok()?;
        Some(Self { language, audio, partials, done, partial: String::new() })
    }

    /// Queue audio for the decoder thread. Never waits for a decode.
    fn send(&self, pcm: &[f32]) {
        let _ = self.audio.send(PassMsg::Audio(pcm.to_vec()));
    }

    /// Pick up the newest partial, if one arrived.
    fn poll_partial(&mut self) {
        if let Some(p) = self.partials.try_iter().last() {
            self.partial = p;
        }
    }

    /// Ask the decoder thread to decode what's queued plus the tail.
    fn request_finish(&self) {
        let _ = self.audio.send(PassMsg::Finish);
    }

    /// Wait for the final transcript, after [`Self::request_finish`].
    fn wait_finished(self) -> Result<Transcript, SttError> {
        self.done.recv().unwrap_or_else(|_| Err(SttError::Model("decoder thread exited".into())))
    }
}

/// The decoder thread: feed audio as it comes, taking everything that queued up behind
/// a slow decode in one go, and report partials. Ends at `Finish` or when the pass is
/// dropped.
fn decode_pass(
    mut stream: Box<dyn SttStream>,
    rx: &Receiver<PassMsg>,
    partials: &Sender<String>,
    done: &Sender<Result<Transcript, SttError>>,
) {
    let mut pcm = Vec::new();
    while let Ok(first) = rx.recv() {
        let mut finish = false;
        for msg in std::iter::once(first).chain(rx.try_iter()) {
            match msg {
                PassMsg::Audio(a) => pcm.extend_from_slice(&a),
                PassMsg::Finish => finish = true,
            }
        }
        if !pcm.is_empty() {
            stream.accept(&pcm);
            pcm.clear();
        }
        if finish {
            let _ = done.send(stream.finish());
            return;
        }
        if let Some(p) = stream.partial() {
            let _ = partials.send(p);
        }
    }
}

struct Dictation {
    /// Where the text is going: picks the Layer 1 format context.
    target: TargetContext,
    /// Streaming decodes: none (batch engine), one, or English + Hindi in Auto mode.
    passes: Vec<Pass>,
    vad: Option<Vad>,
    segments: Vec<JoinHandle<Result<Transcript, SttError>>>,
    audio: Vec<f32>,
    /// The partial last sent to the host: which pass, and its raw text.
    shown: (Language, String),
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
    /// The language of the previous dictation (`En` or `Hi`), for ambiguous Auto ones.
    /// Lives as long as the engine.
    previous_language: Option<Language>,
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
            previous_language: None,
        })
    }

    fn set_state(&self, s: DictationState) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
        self.events.on_state(s);
    }

    fn opts(&self, language: Language) -> SttOptions {
        SttOptions { language, script: self.config.script, dictionary: self.config.dictionary.clone() }
    }

    /// Stream languages for a dictation on a streaming engine.
    fn pass_languages(&self, engine: &dyn SttEngine) -> Vec<Language> {
        match self.config.language {
            LanguageMode::Auto if engine.per_stream_language() => vec![Language::En, Language::Hi],
            mode => vec![mode.stt_language()],
        }
    }

    /// The Auto language decision for a dictation whose Hindi pass says `hindi_text`.
    fn decide(&self, hindi_text: &str) -> Language {
        match detect(hindi_text) {
            Detected::English => Language::En,
            Detected::Hinglish => Language::Hi,
            Detected::Ambiguous => self.previous_language.unwrap_or(Language::En),
        }
    }

    /// Which of `passes` the user should see: the only one, or Auto's current choice.
    fn favoured<'a>(&self, passes: &'a [Pass]) -> Option<&'a Pass> {
        match passes {
            [only] => Some(only),
            _ => {
                let hindi = passes.iter().find(|p| p.language == Language::Hi).map_or("", |p| p.partial.as_str());
                let choice = self.decide(hindi);
                passes.iter().find(|p| p.language == choice).or(passes.first())
            }
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
        let engine = self.ensure_stt()?;
        self.open_mic()?;
        self.model_loaded = true;
        self.unload_at = None;
        // Streaming engines decode as audio arrives; batch engines get VAD segmentation.
        let passes: Vec<Pass> = if engine.supports_streaming() {
            self.pass_languages(engine.as_ref())
                .into_iter()
                .filter_map(|language| Pass::spawn(language, engine.stream(&self.opts(language))?))
                .collect()
        } else {
            Vec::new()
        };
        let vad = match self.stt.as_ref().and_then(|b| b.vad_model.clone()) {
            Some(model) if passes.is_empty() => Vad::new(&model, VadConfig::default()).ok(),
            _ => None,
        };
        if passes.is_empty() {
            // Cloud: open the TLS connection while the user speaks.
            let e = engine.clone();
            self.rt.spawn(async move { e.prewarm().await });
        }
        let mut dictation = Dictation {
            target: ctx,
            passes,
            vad,
            segments: Vec::new(),
            audio: Vec::new(),
            shown: (Language::Auto, String::new()),
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
        if !s.passes.is_empty() {
            for pass in &mut s.passes {
                pass.send(pcm);
                pass.poll_partial();
            }
            if let Some(pass) = self.favoured(&s.passes)
                && pass.partial != s.shown.1
            {
                s.shown = (pass.language, pass.partial.clone());
                self.events.on_partial(self.romanize(&pass.partial));
            }
        } else if let Some(vad) = s.vad.as_mut() {
            vad.accept(pcm);
            for seg in vad.pop_segments() {
                s.segments.push(self.spawn_transcribe(pad(&seg)));
            }
        }
    }

    fn spawn_transcribe(&self, audio: Vec<f32>) -> JoinHandle<Result<Transcript, SttError>> {
        let opts = self.opts(self.config.language.stt_language());
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

    /// Layer 1 on a final: romanize (with the loanword lexicon), then the rules pipeline
    /// (ADR-0015).
    fn format(&self, raw: &str, language: LanguageMode, target: &TargetContext) -> rules::Formatted {
        let language = match language {
            LanguageMode::English => rules::Language::English,
            LanguageMode::Hinglish => rules::Language::Hinglish,
            LanguageMode::Auto => rules::Language::of_engine_output(raw),
        };
        // One pipeline run gives both renderings; HTML waits for M3 (ADR-0015).
        rules::format(&self.romanize(raw), language, target.code_mode)
    }

    /// Hotkey released: finish recognition and emit `on_final`. Timings run from
    /// `called`, when the host called `stop_dictation`.
    fn stop(&mut self, called: Instant) {
        let received = Instant::now();
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
        let handed_over = Instant::now();

        let result: Result<(String, String, LanguageMode), BoloError> = if !s.passes.is_empty() {
            self.finish_passes(std::mem::take(&mut s.passes))
                .map(|(t, language)| (t.text, t.engine_id, LanguageMode::from_stt_language(language)))
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
            self.rt
                .block_on(async move {
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
                .map(|(text, engine_id)| (text, engine_id, self.config.language))
        };
        let stt_ms = called.elapsed().as_secs_f64() * 1e3;

        match result {
            Ok((raw, engine_id, language)) => {
                self.previous_language = match language {
                    LanguageMode::Auto => self.previous_language,
                    known => Some(known.stt_language()),
                };
                let t_rules = Instant::now();
                let text = self.format(raw.trim(), language, &s.target);
                let rules_ms = t_rules.elapsed().as_secs_f64() * 1e3;
                let total_ms = called.elapsed().as_secs_f64() * 1e3;
                let ms = |a: Instant, b: Instant| b.duration_since(a).as_secs_f64() * 1e3;
                // Where release → text goes (M2 ticket 01): Stop waiting for the engine
                // thread, the last audio handed over, the tail decode, the rules.
                tracing::debug!(
                    queued_ms = ms(called, received),
                    handover_ms = ms(received, handed_over),
                    finish_ms = ms(handed_over, t_rules),
                    rules_ms,
                    total_ms,
                    "stop breakdown"
                );
                self.events.on_final(FormattedOutput {
                    plain: text.plain,
                    markdown: text.markdown,
                    html: None,
                    timings: Timings { stt_ms, rules_ms, llm_ms: 0.0, format_ms: 0.0, total_ms },
                    engine_id,
                    language,
                });
                self.set_state(DictationState::Done);
            }
            Err(e) => self.events.on_error(e),
        }
        self.set_state(DictationState::Idle);
        self.unload_at = Some(Instant::now() + Duration::from_secs(u64::from(self.config.idle_unload_secs)));
    }

    /// Finish the passes and pick the result.
    ///
    /// When the Hindi pass's text at release already decides the language, only the
    /// chosen pass is finished and the other is dropped, even mid-decode. Auto then
    /// costs no more after release than a forced mode: finishing two passes at once
    /// measured ~260 ms p50 against ~190 ms for one. Otherwise both are finished in
    /// parallel and Auto decides on their final text, falling back to the other pass if
    /// the chosen one fails.
    fn finish_passes(&self, mut passes: Vec<Pass>) -> Result<(Transcript, Language), BoloError> {
        for pass in &mut passes {
            pass.poll_partial();
        }
        if passes.len() > 1 {
            let hindi = passes.iter().find(|p| p.language == Language::Hi).map_or("", |p| p.partial.as_str());
            let early = match detect(hindi) {
                Detected::English => Some(Language::En),
                Detected::Hinglish => Some(Language::Hi),
                Detected::Ambiguous => None,
            };
            if let Some(choice) = early {
                passes.retain(|p| p.language == choice);
            }
        }
        // Ask every pass to finish before waiting on any, so they flush in parallel.
        passes.iter().for_each(Pass::request_finish);
        let finished: Vec<(Language, Result<Transcript, SttError>)> =
            passes.into_iter().map(|p| (p.language, p.wait_finished())).collect();
        let choice = match finished.as_slice() {
            [(only, _)] => *only,
            _ => {
                let hindi = finished.iter().find_map(|(l, r)| match r {
                    Ok(t) if *l == Language::Hi => Some(t.text.as_str()),
                    _ => None,
                });
                self.decide(hindi.unwrap_or_default())
            }
        };
        let mut finished = finished;
        finished.sort_by_key(|(l, _)| *l != choice); // the choice first, then the other pass
        let mut last_err = None;
        for (language, r) in finished {
            match r {
                Ok(t) => return Ok((t, language)),
                Err(e) => {
                    tracing::warn!(?language, error = %e, "stt pass failed");
                    last_err = Some(e);
                }
            }
        }
        Err(BoloError::Stt(last_err.map_or_else(|| "no stt pass".to_owned(), |e| e.to_string())))
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
                Ok(Cmd::Stop(called)) => self.stop(called),
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
