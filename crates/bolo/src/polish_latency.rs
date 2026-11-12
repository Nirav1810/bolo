//! `bolo polish-latency`: how long release → `on_polished` takes on a long dictation
//! (M2 ticket 09, ADR-0016).
//!
//! Clips are chained with 1 s pauses into one dictation of at least `--secs`, played
//! into the engine in real time with polish on. The engine polishes it per segment while
//! it plays. For comparison the same instant text is then polished in one whole-text
//! call, the way a dictation is polished without segments: release → `on_final`, plus
//! that call.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bolo_core::{
    AudioFactory, AudioSource, BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput,
    LanguageMode, Polish, TargetContext, WavSource,
};
use bolo_format::llm::{self, CleanupRequest, Script};

use crate::metrics::percentile;

/// The pause between chained clips: long enough to close a segment (700 ms).
const PAUSE: Duration = Duration::from_secs(1);
/// Silence after the last clip before release.
const TRAILING_SILENCE: Duration = Duration::from_millis(300);
/// How long to wait for `on_polished` after `on_final`.
const POLISH_WAIT: Duration = Duration::from_secs(10);
/// The whole-text call's timeout: generous, to see how long it really takes.
const WHOLE_TIMEOUT: Duration = Duration::from_secs(10);

enum Ev {
    Final(FormattedOutput),
    Polished(FormattedOutput),
    Error(BoloError),
}

struct Events(Mutex<Sender<Ev>>);

impl Events {
    fn send(&self, ev: Ev) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(ev);
        }
    }
}

impl EngineEvents for Events {
    fn on_level(&self, _: f32) {}
    fn on_partial(&self, _: String) {}
    fn on_final(&self, out: FormattedOutput) {
        self.send(Ev::Final(out));
    }
    fn on_polished(&self, out: FormattedOutput) {
        self.send(Ev::Polished(out));
    }
    fn on_error(&self, err: BoloError) {
        self.send(Ev::Error(err));
    }
    fn on_state(&self, _: DictationState) {}
}

pub struct Options<'a> {
    pub engine: &'a str,
    pub language: LanguageMode,
    /// Minimum dictation length, in seconds.
    pub secs: f64,
    pub runs: usize,
}

/// One dictation's timings, in ms from release.
struct Run {
    audio_s: f64,
    final_ms: f64,
    /// Per-segment polish in the engine; `None` when it sent no `on_polished`.
    segmented_ms: Option<f64>,
    /// `final_ms` plus one whole-text polish call; `None` when that call failed.
    whole_ms: Option<f64>,
}

pub async fn run(opts: &Options<'_>, clips_dir: &Path, models_dir: &Path) -> Result<()> {
    let key = crate::env_nonempty("ANTHROPIC_API_KEY").ok_or_else(|| anyhow!("put ANTHROPIC_API_KEY in .env"))?;
    let clips = crate::load_clips(clips_dir)?;
    if clips.is_empty() {
        return Err(anyhow!("no clips in {}", clips_dir.display()));
    }
    let config = EngineConfig {
        engines: vec![opts.engine.to_owned()],
        language: opts.language,
        models_dir: models_dir.to_path_buf(),
        mic_warm_secs: 0,
        idle_unload_secs: 3600,
        polish: Polish::On { tone: "neutral".into(), app: "Notes".into(), dictionary: vec![] },
        api_keys: crate::api_keys_from_env(),
        ..Default::default()
    };
    let current: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let cur = current.clone();
    let factory: AudioFactory = Arc::new(move || {
        let samples = cur.lock().map(|g| g.clone()).unwrap_or_default();
        Ok(Box::new(WavSource::new(samples)) as Box<dyn AudioSource>)
    });
    let (tx, rx) = mpsc::channel();
    let engine =
        tokio::task::block_in_place(|| Engine::with_audio_source(config, Box::new(Events(Mutex::new(tx))), factory))?;
    engine.prewarm();
    std::thread::sleep(Duration::from_secs(3)); // let the model load
    let http = bolo_stt::cloud::http_client()?;

    let rate = f64::from(bolo_audio::SAMPLE_RATE);
    let mut runs = Vec::new();
    let mut next = 0;
    for i in 0..opts.runs {
        // Chain clips, starting where the last run stopped, until the dictation is long enough.
        let mut audio = Vec::new();
        while (audio.len() as f64) < opts.secs * rate {
            audio.extend_from_slice(&clips[next % clips.len()].audio);
            audio.extend(std::iter::repeat_n(0.0, (PAUSE.as_secs_f64() * rate) as usize));
            next += 1;
        }
        audio.extend(std::iter::repeat_n(0.0, (TRAILING_SILENCE.as_secs_f64() * rate) as usize));
        let audio_s = audio.len() as f64 / rate;
        if let Ok(mut g) = current.lock() {
            *g = audio;
        }
        engine.start_dictation(TargetContext::default())?;
        std::thread::sleep(Duration::from_secs_f64(audio_s));
        let released = Instant::now();
        engine.stop_dictation()?;
        let (out, final_ms) = wait_final(&rx, released)?;
        let segmented_ms = wait_polished(&rx).map(|(_, at)| at.duration_since(released).as_secs_f64() * 1e3);

        let req = CleanupRequest {
            transcript: &out.plain,
            script: Script::Roman,
            tone: "neutral",
            app: "Notes",
            dictionary: &[],
            triggers: &[],
            context: None,
        };
        let t0 = Instant::now();
        let whole_ms = match llm::cleanup(&http, &key, &req, WHOLE_TIMEOUT).await {
            Ok(_) => Some(final_ms + t0.elapsed().as_secs_f64() * 1e3),
            Err(e) => {
                eprintln!("  whole-text polish failed: {e}");
                None
            }
        };
        let show = |ms: Option<f64>| ms.map_or_else(|| "none".to_owned(), |m| format!("{m:.0} ms"));
        println!(
            "run {}: {audio_s:.1} s, {} words; release → on_final {final_ms:.0} ms, on_polished (per segment) {}, whole-text {}",
            i + 1,
            out.plain.split_whitespace().count(),
            show(segmented_ms),
            show(whole_ms),
        );
        runs.push(Run { audio_s, final_ms, segmented_ms, whole_ms });
    }

    let p50 = |v: Vec<f64>| percentile(&v, 50.0).unwrap_or(f64::NAN);
    println!(
        "\n{} runs of {:.0}–{:.0} s ({}, {:?}): release → on_final p50 {:.0} ms; release → on_polished p50: per segment {:.0} ms ({} of {} polished), whole text {:.0} ms",
        runs.len(),
        runs.iter().map(|r| r.audio_s).fold(f64::INFINITY, f64::min),
        runs.iter().map(|r| r.audio_s).fold(0.0, f64::max),
        opts.engine,
        opts.language,
        p50(runs.iter().map(|r| r.final_ms).collect()),
        p50(runs.iter().filter_map(|r| r.segmented_ms).collect()),
        runs.iter().filter(|r| r.segmented_ms.is_some()).count(),
        runs.len(),
        p50(runs.iter().filter_map(|r| r.whole_ms).collect()),
    );
    Ok(())
}

fn wait_final(rx: &Receiver<Ev>, released: Instant) -> Result<(FormattedOutput, f64)> {
    loop {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ev::Final(o)) => return Ok((o, released.elapsed().as_secs_f64() * 1e3)),
            Ok(Ev::Polished(_)) => {} // a late one from the previous run
            Ok(Ev::Error(e)) => return Err(anyhow!("{e}")),
            Err(_) => return Err(anyhow!("timed out waiting for on_final")),
        }
    }
}

fn wait_polished(rx: &Receiver<Ev>) -> Option<(FormattedOutput, Instant)> {
    match rx.recv_timeout(POLISH_WAIT) {
        Ok(Ev::Polished(o)) => Some((o, Instant::now())),
        _ => None,
    }
}
