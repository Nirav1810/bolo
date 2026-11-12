//! Per-segment polish for long dictations (ADR-0016).
//!
//! When polish is on, a [`PauseDetector`] listens alongside the STT engine. Once a
//! dictation has run for [`SEGMENT_AFTER`], each pause of [`SEGMENT_PAUSE`] or more
//! closes a **segment**: the text recognised up to that pause and not yet sent. The
//! segment goes to a [`Polisher`], which polishes segments one at a time in the
//! background, each with the previous polished segment as read-only context. At release
//! the rest of the text is polished last and joined with the earlier segments, so
//! release only waits for one short polish call.
//!
//! A short dictation goes through the same [`Polisher`] as a single part, polished at
//! release (ADR-0016's whole-text case).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bolo_format::llm::{CleanupRequest, Script};
use bolo_format::snippets::Snippets;
use bolo_stt::vad::{Vad, VadConfig};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

use crate::polish::{self, LlmClient, POLISH_DEADLINE};
use crate::worker::Layer1;
use crate::{EngineEvents, FormattedOutput, LanguageMode};

/// Dictations at least this long are polished segment by segment.
pub const SEGMENT_AFTER: Duration = Duration::from_secs(20);

/// A pause at least this long closes a segment.
pub const SEGMENT_PAUSE: Duration = Duration::from_millis(700);

/// The energy detector's frame (30 ms at 16 kHz).
const ENERGY_FRAME: usize = 480;
/// Frames quieter than this (≈ −46 dBFS) are silence to the energy detector.
const SILENCE_RMS: f32 = 0.005;

/// Finds the pauses that close segments: Silero VAD when its model is installed, and a
/// plain energy gate otherwise (so the feature, and the engine tests, work without it).
pub(crate) enum PauseDetector {
    Silero(Box<Vad>),
    Energy {
        /// Samples of silence since the last loud frame.
        silent: usize,
        /// Whether speech was heard since the last pause.
        heard: bool,
        frame: Vec<f32>,
    },
}

impl PauseDetector {
    pub(crate) fn new(vad_model: Option<&Path>) -> Self {
        let cfg = VadConfig { min_silence: SEGMENT_PAUSE.as_secs_f32(), max_speech: 30.0, ..VadConfig::default() };
        match vad_model.map(|m| Vad::new(m, cfg)) {
            Some(Ok(vad)) => Self::Silero(Box::new(vad)),
            Some(Err(e)) => {
                tracing::warn!(error = %e, "Silero VAD unavailable, segmenting on energy");
                Self::energy()
            }
            None => Self::energy(),
        }
    }

    fn energy() -> Self {
        Self::Energy { silent: 0, heard: false, frame: Vec::with_capacity(ENERGY_FRAME) }
    }

    /// Feed 16 kHz mono audio. True when a pause of [`SEGMENT_PAUSE`] or more after
    /// speech was completed within it.
    pub(crate) fn accept(&mut self, pcm: &[f32]) -> bool {
        match self {
            Self::Silero(vad) => {
                vad.accept(pcm);
                !vad.pop_segments().is_empty()
            }
            Self::Energy { silent, heard, frame } => {
                let pause = (SEGMENT_PAUSE.as_secs_f32() * bolo_audio::SAMPLE_RATE as f32) as usize;
                let mut found = false;
                for &x in pcm {
                    frame.push(x);
                    if frame.len() < ENERGY_FRAME {
                        continue;
                    }
                    if bolo_audio::rms(frame) >= SILENCE_RMS {
                        *heard = true;
                        *silent = 0;
                    } else {
                        *silent += ENERGY_FRAME;
                        if *heard && *silent >= pause {
                            *heard = false;
                            found = true;
                        }
                    }
                    frame.clear();
                }
                found
            }
        }
    }
}

/// A segment's raw text: ready on streaming engines, still being transcribed on batch
/// engines.
pub(crate) type RawText = Pin<Box<dyn Future<Output = String> + Send>>;

enum Msg {
    /// A closed segment and its language.
    Segment(RawText, LanguageMode),
    /// Release: the rest of the dictation's polish input (Layer 1, rendered for the
    /// target), the instant output it replaces, and when `stop_dictation` was called.
    Finish { rest: String, out: FormattedOutput, called: Instant },
}

/// What one polish needs besides its text.
pub(crate) struct PolishSetup {
    pub client: Arc<dyn LlmClient>,
    pub tone: String,
    pub app: String,
    pub dictionary: Vec<String>,
    /// Applied to the joined polished text; polish is told to keep the triggers.
    pub snippets: Arc<Snippets>,
    pub script: Script,
    pub layer1: Layer1,
    pub events: Arc<dyn EngineEvents>,
    /// The engine's polish generation, and the one this polish started in: it reports
    /// only while they are equal.
    pub gate: Arc<Mutex<u64>>,
    pub generation: u64,
}

/// One dictation's polish, running on the runtime. Dropping it without
/// [`Self::finish`] (or calling [`Self::abort`]) means nothing is ever reported.
pub(crate) struct Polisher {
    tx: UnboundedSender<Msg>,
    task: JoinHandle<()>,
}

impl Polisher {
    pub(crate) fn spawn(rt: &Runtime, setup: PolishSetup) -> Self {
        let (tx, rx) = unbounded_channel();
        Self { tx, task: rt.spawn(run(setup, rx)) }
    }

    /// Queue a closed segment; it is polished after the ones before it.
    pub(crate) fn segment(&self, raw: RawText, language: LanguageMode) {
        let _ = self.tx.send(Msg::Segment(raw, language));
    }

    /// Release: polish `rest` after the queued segments, join everything, and send
    /// `on_polished` if any part was polished. Returns the task, for cancelling.
    pub(crate) fn finish(self, rest: String, out: FormattedOutput, called: Instant) -> JoinHandle<()> {
        let _ = self.tx.send(Msg::Finish { rest, out, called });
        self.task
    }

    pub(crate) fn abort(self) {
        self.task.abort();
    }
}

/// One polished (or kept) part of the text.
struct Part {
    text: String,
    polished: bool,
}

async fn run(setup: PolishSetup, mut rx: UnboundedReceiver<Msg>) {
    let mut parts: Vec<Part> = Vec::new();
    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::Segment(raw, language) => {
                let input = setup.layer1.render(raw.await.trim(), language);
                if !input.trim().is_empty() {
                    let part = polish_part(&setup, &input, parts.last().map(|p| p.text.as_str())).await;
                    parts.push(part);
                }
            }
            Msg::Finish { rest, out, called } => {
                let t0 = Instant::now();
                if !rest.trim().is_empty() {
                    let part = polish_part(&setup, &rest, parts.last().map(|p| p.text.as_str())).await;
                    parts.push(part);
                }
                report(&setup, &parts, out, called, t0);
                return;
            }
        }
    }
}

/// Polish one part within the deadline. A timeout, an error or a failed guard keeps
/// the part's Layer 1 text.
async fn polish_part(setup: &PolishSetup, input: &str, context: Option<&str>) -> Part {
    let triggers: Vec<String> = setup.snippets.triggers_in(input).into_iter().map(str::to_owned).collect();
    let req = CleanupRequest {
        transcript: input,
        script: setup.script,
        tone: &setup.tone,
        app: &setup.app,
        dictionary: &setup.dictionary,
        triggers: &triggers,
        context,
    };
    let kept = || Part { text: input.to_owned(), polished: false };
    match tokio::time::timeout(POLISH_DEADLINE, setup.client.polish(&req, POLISH_DEADLINE)).await {
        Ok(Ok(answer)) => match polish::guard(input, &answer) {
            Some(text) => Part { text, polished: true },
            None => {
                tracing::warn!("polish answer failed the guard, Layer 1 text kept");
                kept()
            }
        },
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "polish failed, Layer 1 text kept");
            kept()
        }
        Err(_) => {
            tracing::warn!(deadline = ?POLISH_DEADLINE, "polish timed out, Layer 1 text kept");
            kept()
        }
    }
}

/// Send the joined text as `on_polished`, unless no part was polished (then the instant
/// text stands) or this polish was cancelled or superseded.
fn report(setup: &PolishSetup, parts: &[Part], out: FormattedOutput, called: Instant, released: Instant) {
    if !parts.iter().any(|p| p.polished) {
        return;
    }
    let text = join(parts);
    // Held while reporting, so after a cancel nothing reports.
    let Ok(current) = setup.gate.lock() else { return };
    if *current != setup.generation {
        return;
    }
    let mut polished = out;
    polished.html = setup.layer1.html_of(&text);
    polished.plain = text.clone();
    polished.markdown = text;
    crate::worker::expand_snippets(&mut polished, &setup.snippets);
    polished.timings.llm_ms = released.elapsed().as_secs_f64() * 1e3;
    polished.timings.total_ms = called.elapsed().as_secs_f64() * 1e3;
    setup.events.on_polished(polished);
}

/// Parts joined in order: a space between sentences, a blank line next to a Markdown
/// block (a list).
fn join(parts: &[Part]) -> String {
    let mut text = String::new();
    let mut previous: Option<&str> = None;
    for part in parts {
        if let Some(prev) = previous {
            text.push_str(if prev.contains('\n') || part.text.contains('\n') { "\n\n" } else { " " });
        }
        text.push_str(&part.text);
        previous = Some(&part.text);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * bolo_audio::SAMPLE_RATE as f32) as usize]
    }

    fn loud(secs: f32) -> Vec<f32> {
        (0..(secs * bolo_audio::SAMPLE_RATE as f32) as usize).map(|i| if i % 2 == 0 { 0.1 } else { -0.1 }).collect()
    }

    #[test]
    fn the_energy_detector_finds_pauses_of_700_ms_after_speech() {
        let mut d = PauseDetector::new(None);
        assert!(!d.accept(&silence(2.0)), "silence before any speech is no pause");
        assert!(!d.accept(&loud(1.0)));
        assert!(!d.accept(&silence(0.5)), "a 500 ms gap is not a pause");
        assert!(!d.accept(&loud(0.3)));
        let pause = silence(0.8);
        let found: Vec<bool> = pause.chunks(160).map(|c| d.accept(c)).collect();
        assert_eq!(found.iter().filter(|f| **f).count(), 1, "one pause, reported once");
        assert!(!d.accept(&silence(3.0)), "a long silence stays one pause");
    }

    #[test]
    fn parts_join_with_spaces_or_around_lists() {
        let part = |t: &str| Part { text: t.into(), polished: true };
        assert_eq!(join(&[part("One."), part("Two.")]), "One. Two.");
        assert_eq!(join(&[part("Buy:"), part("- milk\n- eggs"), part("Done.")]), "Buy:\n\n- milk\n- eggs\n\nDone.");
        assert_eq!(join(&[]), "");
    }
}
