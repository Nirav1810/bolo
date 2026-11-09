//! `bolo replay`: end-to-end test of `bolo_core::Engine`. Each clip is played
//! into the engine in real time as if spoken into the mic, then `stop_dictation` is
//! called. We measure stop → `on_final`, the latency a user feels.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bolo_core::{
    AudioFactory, AudioSource, BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput,
    LanguageMode, TargetContext, WavSource,
};

use crate::metrics::{ErrorCount, percentile, wer};

enum Ev {
    Final(FormattedOutput),
    Error(BoloError),
}

struct Events(Mutex<Sender<Ev>>);

impl EngineEvents for Events {
    fn on_level(&self, _: f32) {}
    fn on_partial(&self, _: String) {}
    fn on_final(&self, out: FormattedOutput) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(Ev::Final(out));
        }
    }
    fn on_error(&self, err: BoloError) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(Ev::Error(err));
        }
    }
    fn on_state(&self, _: DictationState) {}
}

/// Silence after the clip before "releasing the key", like a person letting go.
const TRAILING_SILENCE: Duration = Duration::from_millis(300);

pub fn run(
    engine_spec: &str,
    language: LanguageMode,
    speed: f64,
    clips_dir: &Path,
    models_dir: &Path,
    aliases: &HashMap<String, String>,
    limit: usize,
) -> Result<()> {
    let clips = crate::load_clips(clips_dir)?;
    let clips: Vec<_> = clips.into_iter().take(limit).collect();
    let config = EngineConfig {
        engines: vec![engine_spec.to_owned()],
        language,
        models_dir: models_dir.to_path_buf(),
        mic_warm_secs: 0, // reopen the "mic" (next clip) every dictation
        idle_unload_secs: 3600,
        api_keys: crate::api_keys_from_env(),
        ..Default::default()
    };
    // The "mic" plays whichever clip is current.
    let current: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let cur = current.clone();
    let factory: AudioFactory = Arc::new(move || {
        let samples = cur.lock().map(|g| g.clone()).unwrap_or_default();
        Ok(Box::new(WavSource::with_speed(samples, speed)) as Box<dyn AudioSource>)
    });
    let (tx, rx) = mpsc::channel();
    let engine = Engine::with_audio_source(config, Box::new(Events(Mutex::new(tx))), factory)?;
    engine.prewarm();
    std::thread::sleep(Duration::from_secs(3)); // let the model load, as the app does at startup

    let mut lat = Vec::new();
    // The engine's own release→text (`Timings::total_ms`). It leaves out any wait for the
    // engine thread to finish a chunk it was decoding when the stop arrived.
    let mut engine_lat = Vec::new();
    let mut errs: HashMap<String, ErrorCount> = HashMap::new();
    // Per category: how often each language was chosen.
    let mut chosen: HashMap<String, HashMap<LanguageMode, usize>> = HashMap::new();
    for clip in &clips {
        if let Ok(mut g) = current.lock() {
            g.clone_from(&clip.audio);
            if speed > 1.0 {
                // A sped-up source stops at the end of its samples, so pad them here.
                let pad = (TRAILING_SILENCE.as_secs_f64() * f64::from(bolo_audio::SAMPLE_RATE)) as usize;
                g.extend(std::iter::repeat_n(0.0, pad));
            }
        }
        engine.start_dictation(TargetContext::default())?;
        let spoken = Duration::from_secs_f64(bolo_audio::duration_secs(&clip.audio)) + TRAILING_SILENCE;
        std::thread::sleep(spoken.div_f64(speed));
        let t_stop = Instant::now();
        engine.stop_dictation()?;
        let out = match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ev::Final(o)) => Ok(o),
            Ok(Ev::Error(e)) => Err(anyhow!("{e}")),
            Err(_) => Err(anyhow!("timed out waiting for final")),
        };
        let ms = t_stop.elapsed().as_secs_f64() * 1e3;
        match out {
            Ok(o) => {
                let w = wer(&clip.reference, &o.plain, aliases);
                errs.entry(clip.category.clone()).or_default().add(w);
                *chosen.entry(clip.category.clone()).or_default().entry(o.language).or_default() += 1;
                lat.push(ms);
                engine_lat.push(o.timings.total_ms);
                println!(
                    "  {:<44} {:>5.0} ms (engine {:>4.0})  WER {:>5.1}%  {:<8}  {}",
                    clip.name,
                    ms,
                    o.timings.total_ms,
                    w.rate() * 100.0,
                    format!("{:?}", o.language),
                    o.plain
                );
            }
            Err(e) => println!("  {:<44} FAILED {e}", clip.name),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!(
        "\n{engine_spec} ({language:?}): stop→text p50 {:.0} ms, p95 {:.0} ms (engine-side p50 {:.0} ms) over {} clips",
        percentile(&lat, 50.0).unwrap_or(0.0),
        percentile(&lat, 95.0).unwrap_or(0.0),
        percentile(&engine_lat, 50.0).unwrap_or(0.0),
        lat.len()
    );
    if speed != 1.0 {
        println!("  (played at {speed}× real time: latency is not representative)");
    }
    let mut cats: Vec<_> = errs.into_iter().collect();
    cats.sort_by(|a, b| a.0.cmp(&b.0));
    for (cat, e) in cats {
        let mut picks: Vec<_> = chosen.get(&cat).into_iter().flatten().map(|(l, n)| format!("{l:?} {n}")).collect();
        picks.sort();
        println!("  WER {cat}: {:.1}%  (language: {})", e.rate() * 100.0, picks.join(", "));
    }
    Ok(())
}
