//! `bolo replay`: end-to-end test of `bolo_core::Engine`. Each clip is played
//! into the engine in real time as if spoken into the mic, then `stop_dictation` is
//! called. We measure stop → `on_final`, the latency a user feels. Each output is
//! scored against the verbatim and the formatted reference; `--polish` also polishes
//! it and scores that. Results go to `replay.csv` and `summary.md`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bolo_core::{
    AudioFactory, AudioSource, BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput,
    LanguageMode, TargetContext, WavSource,
};
use serde::Serialize;

use crate::metrics::{ErrorCount, WerSet, percentile};
use crate::polish::Polisher;

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

pub struct Options<'a> {
    pub language: LanguageMode,
    /// Playback speed; latency is only realistic at 1.
    pub speed: f64,
    pub limit: usize,
    /// Folder for replay.csv and summary.md.
    pub out: &'a Path,
}

#[derive(Serialize)]
struct Row {
    engine: String,
    clip: String,
    category: String,
    language: String,
    stop_ms: f64,
    engine_ms: f64,
    wer: f64,
    wer_formatted: f64,
    wer_polished: Option<f64>,
    wer_formatted_polished: Option<f64>,
    text: String,
    polished: String,
    reference: String,
    reference_formatted: String,
}

/// Silence after the clip before "releasing the key", like a person letting go.
const TRAILING_SILENCE: Duration = Duration::from_millis(300);

pub fn run(
    engine_spec: &str,
    opts: &Options<'_>,
    clips_dir: &Path,
    models_dir: &Path,
    aliases: &HashMap<String, String>,
    mut polisher: Option<&mut Polisher>,
) -> Result<()> {
    let Options { language, speed, limit, out: out_dir } = *opts;
    let clips = crate::load_clips(clips_dir)?;
    let clips: Vec<_> = clips.into_iter().take(limit).collect();
    std::fs::create_dir_all(out_dir)?;
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
    // The engine's own release→text (`Timings::total_ms`): from the `stop_dictation` call to
    // the output being ready. Stop→text adds only the event's trip back to this thread.
    let mut engine_lat = Vec::new();
    let mut errs: BTreeMap<String, WerSet> = BTreeMap::new();
    let mut rows = Vec::new();
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
                // Polish runs after the latency is taken, while the engine sits idle.
                let polished =
                    polisher.as_mut().map(|p| tokio::runtime::Handle::current().block_on(p.polish(&o.plain)));
                let w = WerSet::score(&clip.reference, &clip.formatted, &o.plain, polished.as_deref(), aliases);
                errs.entry(clip.category.clone()).or_default().add(w);
                *chosen.entry(clip.category.clone()).or_default().entry(o.language).or_default() += 1;
                lat.push(ms);
                engine_lat.push(o.timings.total_ms);
                println!(
                    "  {:<44} {:>5.0} ms (engine {:>4.0})  WER {:>5.1}%  fmt {:>5.1}%  {:<8}  {}",
                    clip.name,
                    ms,
                    o.timings.total_ms,
                    w.verbatim.rate() * 100.0,
                    w.formatted.rate() * 100.0,
                    format!("{:?}", o.language),
                    o.plain
                );
                if let Some(p) = &polished {
                    println!("  {:<44} polished: {p}", "");
                }
                rows.push(Row {
                    engine: engine_spec.to_owned(),
                    clip: clip.name.clone(),
                    category: clip.category.clone(),
                    language: format!("{:?}", o.language),
                    stop_ms: ms,
                    engine_ms: o.timings.total_ms,
                    wer: w.verbatim.rate(),
                    wer_formatted: w.formatted.rate(),
                    wer_polished: w.polished.map(|e| e.rate()),
                    wer_formatted_polished: w.formatted_polished.map(|e| e.rate()),
                    text: o.plain,
                    polished: polished.unwrap_or_default(),
                    reference: clip.reference.clone(),
                    reference_formatted: clip.formatted.clone(),
                });
            }
            Err(e) => println!("  {:<44} FAILED {e}", clip.name),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut md = format!(
        "{engine_spec} ({language:?}): stop→text p50 {:.0} ms, p95 {:.0} ms (engine-side p50 {:.0} ms) over {} clips\n",
        percentile(&lat, 50.0).unwrap_or(0.0),
        percentile(&lat, 95.0).unwrap_or(0.0),
        percentile(&engine_lat, 50.0).unwrap_or(0.0),
        lat.len()
    );
    if speed != 1.0 {
        md += &format!("(played at {speed}× real time: latency is not representative)\n");
    }
    md += &summary_table(&errs, &chosen);
    if let Some(p) = &polisher {
        md += &format!("\n{}\n", p.report());
    }
    println!("\n{md}");

    let mut w = csv::Writer::from_path(out_dir.join("replay.csv"))?;
    for r in &rows {
        w.serialize(r)?;
    }
    w.flush()?;
    std::fs::write(out_dir.join("summary.md"), md)?;
    Ok(())
}

/// Per category: verbatim and formatted WER (plus the polished pair with `--polish`)
/// and which language the engine chose.
fn summary_table(errs: &BTreeMap<String, WerSet>, chosen: &HashMap<String, HashMap<LanguageMode, usize>>) -> String {
    let polished = errs.values().any(|w| w.polished.is_some());
    let mut md = String::from("\n| category | WER | fmt WER |");
    if polished {
        md += " +polish WER | +polish fmt WER |";
    }
    md += " language |\n|---|---|---|";
    if polished {
        md += "---|---|";
    }
    md += "---|\n";
    let pct = |e: Option<ErrorCount>| e.map_or("–".to_owned(), |e| format!("{:.1}%", e.rate() * 100.0));
    for (cat, w) in errs {
        let mut picks: Vec<_> = chosen.get(cat).into_iter().flatten().map(|(l, n)| format!("{l:?} {n}")).collect();
        picks.sort();
        md += &format!("| {cat} | {} | {} |", pct(Some(w.verbatim)), pct(Some(w.formatted)));
        if polished {
            md += &format!(" {} | {} |", pct(w.polished), pct(w.formatted_polished));
        }
        md += &format!(" {} |\n", picks.join(", "));
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(errors: usize, polished: Option<usize>) -> WerSet {
        let e = |n| ErrorCount { errors: n, ref_len: 10 };
        WerSet {
            verbatim: e(errors),
            formatted: e(errors + 1),
            polished: polished.map(e),
            formatted_polished: polished.map(e),
        }
    }

    #[test]
    fn table_has_polish_columns_only_with_polish() {
        let chosen = HashMap::from([("en".to_owned(), HashMap::from([(LanguageMode::English, 2)]))]);
        let plain = summary_table(&BTreeMap::from([("en".to_owned(), set(1, None))]), &chosen);
        assert!(plain.contains("| en | 10.0% | 20.0% | English 2 |"), "{plain}");
        assert!(!plain.contains("polish"));
        let pol = summary_table(&BTreeMap::from([("en".to_owned(), set(1, Some(0)))]), &chosen);
        assert!(pol.contains("| en | 10.0% | 20.0% | 0.0% | 0.0% | English 2 |"), "{pol}");
    }
}
