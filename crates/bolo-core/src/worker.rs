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
//!
//! With polish on, the instant text goes out first and the polish call runs on the
//! runtime afterwards (ADR-0016), so the engine thread is free for the next dictation.
//! A long dictation is polished segment by segment while the user speaks
//! ([`crate::segment`]). A generation counter makes sure a cancelled or superseded
//! polish never reports.

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
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use tokio::runtime::Runtime;
use tokio::task::{AbortHandle, JoinHandle};

use crate::audio::{AudioFactory, AudioSource};
use crate::build::{BuiltStt, build_stt};
use crate::polish::{self, LlmClient, Polish};
use crate::segment::{PauseDetector, PolishSetup, Polisher, RawText, SEGMENT_AFTER};
use crate::{
    BoloError, DictationState, EngineConfig, EngineEvents, FormattedOutput, HinglishScript, LanguageMode, OutputFormat,
    TargetContext, Timings,
};

/// Audio kept from just before the hotkey while the mic is warm (seconds).
const PREROLL_SECS: f32 = 0.3;
// The desktop app starts a dictation only once push-to-talk has been held for 150 ms
// (ADR-0017); the pre-roll must cover that with room to spare.
const _: () = assert!(PREROLL_SECS >= 0.3, "pre-roll must cover the 150 ms push-to-talk hold twice over");
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
    SetPolish(Polish),
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

/// Layer 1 for one target: romanize (with the loanword lexicon), then the rules
/// pipeline (ADR-0015).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layer1 {
    pub script: HinglishScript,
    pub code_mode: bool,
    /// The target's format. Polish works on the Markdown rendering for a Markdown
    /// target and on plain text otherwise; an HTML target also gets `html`.
    pub format: OutputFormat,
}

impl Layer1 {
    fn new(script: HinglishScript, target: &TargetContext) -> Self {
        Self { script, code_mode: target.code_mode, format: target.format }
    }

    pub(crate) fn romanize(&self, text: &str) -> String {
        if self.script == HinglishScript::Roman && bolo_format::translit::needs_romanizing(text) {
            bolo_format::loanwords::to_roman_hinglish(text)
        } else {
            text.to_owned()
        }
    }

    /// Every rendering of a final (`raw` is the engine's text).
    pub(crate) fn format(&self, raw: &str, language: LanguageMode) -> rules::Formatted {
        let language = match language {
            LanguageMode::English => rules::Language::English,
            LanguageMode::Hinglish => rules::Language::Hinglish,
            LanguageMode::Auto => rules::Language::of_engine_output(raw),
        };
        // One pipeline run gives every rendering (ADR-0015).
        rules::format(&self.romanize(raw), language, self.code_mode)
    }

    /// The rendering the target gets, which is the one polish works on.
    pub(crate) fn render(&self, raw: &str, language: LanguageMode) -> String {
        let f = self.format(raw, language);
        if self.format == OutputFormat::Markdown { f.markdown } else { f.plain }
    }

    /// The `html` field for already formatted text (polish output): the text read back
    /// through the HTML renderer, never trusted as markup. `None` unless the target
    /// takes HTML.
    pub(crate) fn html_of(&self, formatted: &str) -> Option<String> {
        (self.format == OutputFormat::Html).then(|| rules::to_html(formatted))
    }
}

/// One VAD segment on a batch engine, being transcribed on the runtime. The result is
/// shared: the final text and a polish segment may both wait on it.
struct BatchSegment {
    abort: AbortHandle,
    text: Shared<BoxFuture<'static, Result<Transcript, String>>>,
}

/// Per-segment polish state for a dictation with polish on (ADR-0016).
struct SegmentPolish {
    pauses: PauseDetector,
    /// What was already sent as segments: words of the streaming text, or batch
    /// segments.
    cut: usize,
    /// The pass the segments were cut from (streaming engines).
    pass: Option<Language>,
    /// Started at the first segment.
    polisher: Option<Polisher>,
}

struct Dictation {
    /// Where the text is going: picks the Layer 1 format context.
    target: TargetContext,
    /// Streaming decodes: none (batch engine), one, or English + Hindi in Auto mode.
    passes: Vec<Pass>,
    vad: Option<Vad>,
    segments: Vec<BatchSegment>,
    /// With polish on: the pauses that close polish segments, and their polisher.
    polish: Option<SegmentPolish>,
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
    /// Set by `Engine::with_llm_client`: used for polish instead of Claude.
    injected_llm: Option<Arc<dyn LlmClient>>,
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
    /// The polish still running for the last dictation, if any.
    polishing: Option<JoinHandle<()>>,
    /// Bumped to cancel polish: a polish task reports only while the generation it
    /// started in is current. Held while reporting, so after a bump nothing reports.
    polish_generation: Arc<Mutex<u64>>,
}

/// The non-empty texts, space-separated.
fn join_nonempty(parts: &[String]) -> String {
    parts.iter().filter(|p| !p.is_empty()).map(String::as_str).collect::<Vec<_>>().join(" ")
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
        injected_llm: Option<Arc<dyn LlmClient>>,
    ) -> Result<Self, BoloError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("bolo-rt")
            .enable_all()
            .build()
            .map_err(|e| BoloError::Config(e.to_string()))?;
        let manifest = Manifest::bundled().map_err(|e| BoloError::Config(e.to_string()))?;
        let http = bolo_stt::cloud::http_client().map_err(|e| BoloError::Config(e.to_string()))?;
        // Polish on without a key: fail now rather than at the first dictation.
        polish::client(&config.polish, &config.api_keys, injected_llm.as_ref(), &http)?;
        Ok(Self {
            config,
            events,
            state,
            factory,
            injected_stt,
            injected_llm,
            manifest,
            http,
            rt,
            stt: None,
            model_loaded: false,
            unload_at: None,
            mic: None,
            dictation: None,
            previous_language: None,
            polishing: None,
            polish_generation: Arc::new(Mutex::new(0)),
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
        // A new dictation supersedes the last one's polish.
        self.cancel_polish();
        // Checked again here: `update_config` may have turned polish on without a key.
        self.polish_client()?;
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
        // With polish on, find pauses alongside every engine (ADR-0016).
        let polish = (self.config.polish != Polish::Off).then(|| SegmentPolish {
            pauses: PauseDetector::new(self.stt.as_ref().and_then(|b| b.vad_model.as_deref())),
            cut: 0,
            pass: None,
            polisher: None,
        });
        let mut dictation = Dictation {
            target: ctx,
            passes,
            vad,
            segments: Vec::new(),
            polish,
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
                self.events.on_partial(Layer1::new(self.config.script, &s.target).romanize(&pass.partial));
            }
        } else if let Some(vad) = s.vad.as_mut() {
            vad.accept(pcm);
            for seg in vad.pop_segments() {
                s.segments.push(self.spawn_transcribe(pad(&seg)));
            }
        }
        let long = s.audio.len() as f32 >= SEGMENT_AFTER.as_secs_f32() * bolo_audio::SAMPLE_RATE as f32;
        if s.polish.as_mut().is_some_and(|p| p.pauses.accept(pcm)) && long {
            self.cut_segment(s);
        }
    }

    /// A pause in a long dictation: send the text since the last segment to polish.
    /// On a streaming engine that is the shown pass's text so far; on a batch engine,
    /// the VAD segments cut since (they may still be transcribing).
    fn cut_segment(&self, s: &mut Dictation) {
        let Some(sp) = s.polish.as_mut() else { return };
        let (raw, language): (RawText, LanguageMode) = if !s.passes.is_empty() {
            let Some(pass) = self.favoured(&s.passes) else { return };
            // Segments come from one pass: their word counts must line up.
            if sp.pass.is_some_and(|l| l != pass.language) {
                return;
            }
            let words: Vec<&str> = pass.partial.split_whitespace().collect();
            if words.len() <= sp.cut {
                return;
            }
            let raw = words[sp.cut..].join(" ");
            sp.cut = words.len();
            sp.pass = Some(pass.language);
            (Box::pin(std::future::ready(raw)), LanguageMode::from_stt_language(pass.language))
        } else {
            if s.segments.len() <= sp.cut {
                return;
            }
            let texts: Vec<_> = s.segments[sp.cut..].iter().map(|b| b.text.clone()).collect();
            sp.cut = s.segments.len();
            let raw = async move {
                let parts = futures::future::join_all(texts).await;
                let parts: Vec<String> =
                    parts.into_iter().filter_map(Result::ok).map(|t| t.text.trim().to_owned()).collect();
                parts.join(" ")
            };
            (Box::pin(raw), self.config.language)
        };
        if sp.polisher.is_none() {
            sp.polisher = self.polisher(&s.target);
        }
        if let Some(p) = &sp.polisher {
            tracing::debug!(segment_end = sp.cut, "polish segment closed");
            p.segment(raw, language);
        }
    }

    fn spawn_transcribe(&self, audio: Vec<f32>) -> BatchSegment {
        let opts = self.opts(self.config.language.stt_language());
        let engine = self.stt.as_ref().map(|b| b.engine.clone());
        let handle = self.rt.spawn(async move {
            match engine {
                Some(e) => e.transcribe(&audio, &opts).await,
                None => Err(SttError::Model("no engine".into())),
            }
        });
        let abort = handle.abort_handle();
        let text = async move {
            match handle.await {
                Ok(r) => r.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        BatchSegment { abort, text: text.boxed().shared() }
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

        // The raw text, its engine and language, and its units for per-segment polish:
        // words on a streaming engine (with the pass), one text per segment on batch.
        type Recognized = (String, String, LanguageMode, Vec<String>, Option<Language>);
        let result: Result<Recognized, BoloError> = if !s.passes.is_empty() {
            self.finish_passes(std::mem::take(&mut s.passes)).map(|(t, language)| {
                let words = t.text.split_whitespace().map(str::to_owned).collect();
                (t.text, t.engine_id, LanguageMode::from_stt_language(language), words, Some(language))
            })
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
                        let t = h.text.await.map_err(BoloError::Stt)?;
                        engine_id = t.engine_id;
                        parts.push(t.text.trim().to_owned());
                    }
                    Ok((parts, engine_id))
                })
                .map(|(parts, engine_id)| (join_nonempty(&parts), engine_id, self.config.language, parts, None))
        };
        let stt_ms = called.elapsed().as_secs_f64() * 1e3;

        match result {
            Ok((raw, engine_id, language, units, pass)) => {
                self.previous_language = match language {
                    LanguageMode::Auto => self.previous_language,
                    known => Some(known.stt_language()),
                };
                let t_rules = Instant::now();
                let layer1 = Layer1::new(self.config.script, &s.target);
                let text = layer1.format(raw.trim(), language);
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
                let out = FormattedOutput {
                    plain: text.plain,
                    markdown: text.markdown,
                    html: (layer1.format == OutputFormat::Html).then_some(text.html),
                    timings: Timings { stt_ms, rules_ms, llm_ms: 0.0, format_ms: 0.0, total_ms },
                    engine_id,
                    language,
                };
                self.events.on_final(out.clone());
                // With segments sent, polish only the rest. When they no longer line
                // up with the final (another pass won, or the text shrank), drop them
                // and polish the whole text.
                let segmented = s.polish.take().and_then(|sp| {
                    let polisher = sp.polisher?;
                    if sp.cut <= units.len() && (sp.pass.is_none() || sp.pass == pass) {
                        Some((polisher, layer1.render(&join_nonempty(&units[sp.cut..]), language)))
                    } else {
                        tracing::debug!("polish segments dropped, polishing the whole text");
                        polisher.abort();
                        None
                    }
                });
                self.spawn_polish(out, &s.target, called, segmented);
                self.set_state(DictationState::Done);
            }
            Err(e) => {
                // No text: stop polishing the segments already sent.
                if let Some(p) = s.polish.take().and_then(|sp| sp.polisher) {
                    p.abort();
                }
                self.events.on_error(e);
            }
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

    fn polish_client(&self) -> Result<Option<Arc<dyn LlmClient>>, BoloError> {
        polish::client(&self.config.polish, &self.config.api_keys, self.injected_llm.as_ref(), &self.http)
    }

    /// A polisher for a dictation into `target`, or none when polish is off or can't
    /// run (no key: logged, the instant text stands).
    fn polisher(&self, target: &TargetContext) -> Option<Polisher> {
        let Polish::On { tone, app, dictionary } = self.config.polish.clone() else { return None };
        let client = match self.polish_client() {
            Ok(c) => c?,
            Err(e) => {
                tracing::warn!(error = %e, "polish skipped");
                return None;
            }
        };
        let script = match self.config.script {
            HinglishScript::Roman => bolo_format::llm::Script::Roman,
            HinglishScript::Devanagari => bolo_format::llm::Script::Devanagari,
        };
        let gate = self.polish_generation.clone();
        let generation = gate.lock().map(|g| *g).unwrap_or_default();
        let setup = PolishSetup {
            client,
            tone,
            app,
            dictionary,
            script,
            layer1: Layer1::new(self.config.script, target),
            events: self.events.clone(),
            gate,
            generation,
        };
        Some(Polisher::spawn(&self.rt, setup))
    }

    /// With polish on, polish in the background and send `on_polished` once the parts
    /// are in. `segmented` is the polisher already holding this dictation's segments,
    /// with the rest of the text; without it `out` (the instant text just sent) is
    /// polished whole. When no part is polished in time and through the guard, nothing
    /// more is sent, and the Layer 1 text stands.
    fn spawn_polish(
        &mut self,
        out: FormattedOutput,
        target: &TargetContext,
        called: Instant,
        segmented: Option<(Polisher, String)>,
    ) {
        let (polisher, rest) = match segmented {
            Some(s) => s,
            None => {
                let whole =
                    if target.format == OutputFormat::Markdown { out.markdown.clone() } else { out.plain.clone() };
                if whole.trim().is_empty() {
                    return;
                }
                let Some(p) = self.polisher(target) else { return };
                (p, whole)
            }
        };
        self.polishing = Some(polisher.finish(rest, out, called));
    }

    /// Stop the last dictation's polish: after this, it never reports.
    fn cancel_polish(&mut self) {
        if let Ok(mut g) = self.polish_generation.lock() {
            *g += 1;
        }
        if let Some(h) = self.polishing.take() {
            h.abort();
        }
    }

    fn cancel(&mut self) {
        self.cancel_polish();
        if let Some(s) = self.dictation.take() {
            for seg in s.segments {
                seg.abort.abort();
            }
            if let Some(p) = s.polish.and_then(|sp| sp.polisher) {
                p.abort();
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
                // Only polish changed: the model and connections stay.
                Ok(Cmd::SetPolish(p)) => self.config.polish = p,
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
