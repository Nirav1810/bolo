//! `bolo`: Bolo from the terminal. Try dictation, replay clips through the live engine,
//! and compare STT engines on the user's own clips.
//!
//! See docs/BENCH.md for the clip layout and methodology.

mod cache;
mod cleanup;
mod dictate;
mod metrics;
mod polish;
mod polish_latency;
mod record;
mod replay;
mod rss;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use bolo_format::rules::{self, FormatContext};
use bolo_models::{Manifest, ModelStatus, Platform};
use bolo_stt::cloud::{self, DeepgramEngine, GroqEngine, SarvamEngine};
use bolo_stt::local::LocalEngine;
use bolo_stt::{ApiKey, HinglishScript, Language, SttEngine, SttOptions};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::cache::{Cache, Cached};
use crate::metrics::{ErrorCount, WerSet, cer, contains_devanagari, percentile};

#[derive(Parser)]
#[command(
    name = "bolo",
    version,
    about = "Bolo from the terminal: dictate, and benchmark STT engines on recorded clips"
)]
struct Cli {
    /// Where models are installed (default: %LOCALAPPDATA%\Bolo\data\models or the platform equivalent).
    #[arg(long, global = true)]
    models_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List models in the bundled manifest and whether they are installed.
    Models,
    /// Download + verify models by id (or `--all` available ones).
    Download {
        ids: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Show RAM and the recommended default model for this machine.
    Sysinfo,
    /// Transcribe one WAV file with one engine.
    Transcribe {
        wav: PathBuf,
        #[arg(long, short)]
        engine: String,
        #[arg(long, value_enum, default_value = "auto")]
        lang: LangArg,
    },
    /// Guided recording of benchmark clips from a prompt list (Enter = start/stop).
    Record {
        /// Which set: `en` or `hinglish` (reads testdata/prompts/<set>.txt by default).
        #[arg(default_value = "en")]
        set: String,
        #[arg(long)]
        prompts: Option<PathBuf>,
        #[arg(long, default_value = "testdata/clips")]
        clips: PathBuf,
    },
    /// Live dictation from the terminal: Enter = start/stop; result is copied to the clipboard.
    Dictate {
        /// Engine: a local model id (`nemotron-stream`, `parakeet-v2-en`, …) or a cloud
        /// provider (`deepgram`, `deepgram-stream`, `groq`, `sarvam`; key from DEEPGRAM_API_KEY etc.).
        #[arg(long, short, default_value = "nemotron-stream", alias = "model")]
        engine: String,
        /// Language mode: `auto` decodes English and Hindi side by side and picks one per
        /// dictation (on engines that support it); `en` and `hi` force one language.
        #[arg(long, value_enum, default_value = "auto")]
        lang: ModeArg,
        /// Turn on the engine's Claude polish: the polished text replaces the instant
        /// text on the clipboard when it arrives within 1.5 s (needs ANTHROPIC_API_KEY).
        #[arg(long)]
        polish: bool,
    },
    /// Evaluate Layer-2 LLM cleanup (Claude) on transcripts from a previous run.
    CleanupEval {
        /// clips.csv written by `run`.
        #[arg(long, default_value = "bench-results/clips.csv")]
        csv: PathBuf,
        /// Engine-id substrings to include.
        #[arg(long, value_delimiter = ',', default_value = "nemotron,deepgram")]
        engines: Vec<String>,
        #[arg(long, default_value = "bench-results/cleanup")]
        out: PathBuf,
        #[arg(long, default_value = "testdata/clips")]
        clips: PathBuf,
    },
    /// End-to-end test of the live engine: replay clips in real time, measure stop→text.
    Replay {
        #[arg(long, short, default_value = "local:nemotron-stream")]
        engine: String,
        /// Language mode: `auto` decodes English and Hindi side by side and picks one per
        /// clip (on engines that support it); `en` and `hi` force one language.
        #[arg(long, value_enum, default_value = "auto")]
        lang: ModeArg,
        /// Play clips this many times faster than real time. Use it for quick WER and
        /// language-choice checks; latency is only realistic at 1.
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        #[arg(long, default_value = "testdata/clips")]
        clips: PathBuf,
        #[arg(long, default_value_t = usize::MAX)]
        limit: usize,
        /// Also polish each output with Claude and score it (needs ANTHROPIC_API_KEY;
        /// costs money). Without it no LLM calls are made.
        #[arg(long)]
        polish: bool,
        /// Output folder for replay.csv and summary.md.
        #[arg(long, default_value = "bench-results/replay")]
        out: PathBuf,
    },
    /// Release → on_polished on a long dictation: clips chained with pauses into one
    /// dictation, polished per segment by the engine, against one whole-text call
    /// (needs ANTHROPIC_API_KEY; costs money).
    PolishLatency {
        #[arg(long, short, default_value = "local:nemotron-stream")]
        engine: String,
        #[arg(long, value_enum, default_value = "en")]
        lang: ModeArg,
        #[arg(long, default_value = "testdata/clips/en")]
        clips: PathBuf,
        /// Minimum dictation length in seconds.
        #[arg(long, default_value_t = 60.0)]
        secs: f64,
        #[arg(long, default_value_t = 5)]
        runs: usize,
    },
    /// Measure microphone start-up delay (leading digital silence).
    MicTest,
    /// Run the benchmark.
    Run(RunArgs),
}

#[derive(clap::Args)]
struct RunArgs {
    /// Folder of `<name>.wav` + `<name>.txt`; sub-folder name is the category (`en`, `hinglish`).
    #[arg(long, default_value = "testdata/clips")]
    clips: PathBuf,
    /// Comma-separated: `local:<model-id>`, `groq[:model]`, `deepgram[:model]`, `sarvam[:model]`.
    #[arg(long, value_delimiter = ',', required = true)]
    engines: Vec<String>,
    /// Output folder for summary.md, summary.json and clips.csv.
    #[arg(long, default_value = "bench-results")]
    out: PathBuf,
    /// Timed runs per clip for local engines (median is reported). Cloud always uses 1 + cache.
    #[arg(long, default_value_t = 3)]
    runs: usize,
    /// Language hint: `by-folder` maps `en` → en and everything else → auto.
    #[arg(long, value_enum, default_value = "by-folder")]
    lang: LangArg,
    /// Ignore the cloud cache (costs money).
    #[arg(long)]
    no_cache: bool,
    #[arg(long, default_value = "bench-cache")]
    cache_dir: PathBuf,
    /// Comma-separated dictionary terms passed to every engine.
    #[arg(long, value_delimiter = ',')]
    dictionary: Vec<String>,
    /// Inference threads for local engines (default min(4, cores)).
    #[arg(long)]
    threads: Option<i32>,
    /// Also polish each output with Claude and score it (needs ANTHROPIC_API_KEY; costs
    /// money). Without it no LLM calls are made.
    #[arg(long)]
    polish: bool,
}

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum LangArg {
    ByFolder,
    Auto,
    En,
    Hi,
}

/// The engine's language mode (`bolo dictate`, `bolo replay`).
#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Auto,
    En,
    Hi,
}

impl From<ModeArg> for bolo_core::LanguageMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Auto => Self::Auto,
            ModeArg::En => Self::English,
            ModeArg::Hi => Self::Hinglish,
        }
    }
}

/// The rules pipeline's language for one STT request's output, as the engine picks it.
fn rules_language(requested: Language, raw: &str) -> rules::Language {
    match requested {
        Language::En => rules::Language::English,
        Language::Hi => rules::Language::Hinglish,
        Language::Auto => rules::Language::of_engine_output(raw),
    }
}

impl LangArg {
    fn resolve(self, category: &str) -> Language {
        match self {
            LangArg::En => Language::En,
            LangArg::Hi => Language::Hi,
            LangArg::Auto => Language::Auto,
            LangArg::ByFolder if category == "en" => Language::En,
            LangArg::ByFolder => Language::Auto,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();
    // Keys from .env (current folder or a parent). Real environment variables win.
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    let manifest = Manifest::bundled()?;
    let models_dir = match cli.models_dir {
        Some(d) => d,
        None => default_models_dir()?,
    };

    match cli.cmd {
        Cmd::Models => {
            println!("models dir: {}\n", models_dir.display());
            for m in &manifest.models {
                let size = m
                    .download
                    .as_ref()
                    .map(|d| format!("{:.0} MB", d.size_bytes as f64 / 1e6))
                    .unwrap_or_else(|| "-".into());
                let state = match (m.status, m.is_installed(&models_dir)) {
                    (_, true) => "installed",
                    (ModelStatus::Available, false) => "available",
                    (ModelStatus::NeedsExport, false) => "needs-export",
                };
                println!("{:<24} {:<13} {:>8}  {:<10} {}", m.id, state, size, m.license.id, m.name);
            }
        }
        Cmd::Download { ids, all } => {
            let ids: Vec<String> = if all {
                manifest.models.iter().filter(|m| m.status == ModelStatus::Available).map(|m| m.id.clone()).collect()
            } else {
                ids
            };
            if ids.is_empty() {
                bail!("give model ids or --all");
            }
            let dl = bolo_models::Downloader::new(cloud::http_client()?, &models_dir);
            for id in ids {
                let entry = manifest.get(&id)?;
                eprintln!("{id}: downloading to {}", entry.install_dir(&models_dir).display());
                let t0 = Instant::now();
                dl.install(entry, &|p| {
                    eprint!("\r  {:?} {:>6.1}/{:.1} MB   ", p.phase, p.downloaded as f64 / 1e6, p.total as f64 / 1e6);
                })
                .await
                .with_context(|| format!("installing {id}"))?;
                eprintln!("\n  ok ({:.1}s, sha256 verified)", t0.elapsed().as_secs_f64());
            }
        }
        Cmd::Sysinfo => {
            let ram_mb = rss::total_ram_mb();
            println!(
                "total RAM: {ram_mb} MB, cores: {}",
                std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)
            );
            for hinglish in [false, true] {
                let rec = manifest
                    .recommend(Platform::Desktop, ram_mb, hinglish)
                    .map(|m| m.id.as_str())
                    .unwrap_or("none (use cloud BYOK)");
                println!("recommended (desktop, hinglish={hinglish}): {rec}");
            }
        }
        Cmd::Transcribe { wav, engine, lang } => {
            let audio = bolo_audio::codec::read_wav_16k_mono(&wav)?;
            let opts = SttOptions { language: lang.resolve("auto"), ..Default::default() };
            let e = build_engine(&engine, &manifest, &models_dir, None)?;
            e.prewarm().await?;
            let t0 = Instant::now();
            let t = e.transcribe(&audio, &opts).await?;
            println!(
                "{}\n[{} | {:.2}s audio | {:.0} ms]",
                t.text,
                t.engine_id,
                bolo_audio::duration_secs(&audio),
                t0.elapsed().as_secs_f64() * 1e3
            );
        }
        Cmd::Record { set, prompts, clips } => {
            let prompts = prompts.unwrap_or_else(|| PathBuf::from(format!("testdata/prompts/{set}.txt")));
            record::run(&prompts, &clips, &set)?;
        }
        Cmd::Dictate { engine, lang, polish } => {
            dictate::run(&manifest, &models_dir, &engine, lang.into(), polish).await?;
        }
        Cmd::CleanupEval { csv, engines, out, clips } => {
            cleanup::run(&csv, &engines, &out, &load_aliases(&clips)).await?;
        }
        Cmd::Replay { engine, lang, speed, clips, limit, polish, out } => {
            if !(speed.is_finite() && speed > 0.0) {
                bail!("--speed must be a positive number");
            }
            // Deepgram's live endpoint accepts audio at no more than 1.25x real time.
            if engine.starts_with("deepgram-stream") && speed > 1.25 {
                bail!("deepgram-stream takes audio at up to 1.25x real time; use --speed 1");
            }
            let aliases = load_aliases(&clips);
            let mut polisher = if polish { Some(polish::Polisher::from_env()?) } else { None };
            let opts = replay::Options { language: lang.into(), speed, limit, out: &out };
            tokio::task::block_in_place(|| {
                replay::run(&engine, &opts, &clips, &models_dir, &aliases, polisher.as_mut())
            })?;
        }
        Cmd::PolishLatency { engine, lang, clips, secs, runs } => {
            let opts = polish_latency::Options { engine: &engine, language: lang.into(), secs, runs };
            polish_latency::run(&opts, &clips, &models_dir).await?;
        }
        Cmd::MicTest => record::mic_test()?,
        Cmd::Run(args) => run(args, &manifest, &models_dir).await?,
    }
    Ok(())
}

fn default_models_dir() -> Result<PathBuf> {
    bolo_models::default_models_root().ok_or_else(|| anyhow!("no home directory"))
}

enum Built {
    Local(LocalEngine),
    Other(Arc<dyn SttEngine>),
}

impl std::ops::Deref for Built {
    type Target = dyn SttEngine;
    fn deref(&self) -> &Self::Target {
        match self {
            Built::Local(l) => l,
            Built::Other(o) => o.as_ref(),
        }
    }
}

/// A non-empty environment variable (an empty `.env` placeholder counts as unset).
pub(crate) fn env_nonempty(var: &str) -> Option<String> {
    std::env::var(var).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// API keys for the engine from `<PROVIDER>_API_KEY` variables. Never printed.
pub(crate) fn api_keys_from_env() -> bolo_core::ApiKeys {
    let mut keys = bolo_core::ApiKeys::default();
    for p in ["deepgram", "groq", "sarvam", bolo_core::ANTHROPIC] {
        if let Some(k) = env_nonempty(&format!("{}_API_KEY", p.to_uppercase())) {
            keys.set(p, k);
        }
    }
    keys
}

fn env_key(var: &str) -> Result<ApiKey> {
    env_nonempty(var).map(ApiKey::new).ok_or_else(|| anyhow!("put your key in .env as {var}=... to use this provider"))
}

fn build_engine(spec: &str, manifest: &Manifest, models_dir: &Path, threads: Option<i32>) -> Result<Built> {
    let (kind, arg) = spec.split_once(':').map_or((spec, None), |(k, a)| (k, Some(a)));
    Ok(match kind {
        "local" => {
            let id = arg.ok_or_else(|| anyhow!("use local:<model-id>"))?;
            let entry = manifest.get(id)?.clone();
            if !entry.is_installed(models_dir) {
                bail!("{id} is not installed; run `bolo download {id}`");
            }
            Built::Local(LocalEngine::new(entry, models_dir, threads)?)
        }
        "groq" => {
            let c = cloud::http_client()?;
            let k = env_key("GROQ_API_KEY")?;
            Built::Other(Arc::new(match arg {
                Some(m) => GroqEngine::with_model(c, k, cloud::groq::DEFAULT_BASE_URL, m),
                None => GroqEngine::new(c, k),
            }))
        }
        "deepgram" => {
            let c = cloud::http_client()?;
            let k = env_key("DEEPGRAM_API_KEY")?;
            Built::Other(Arc::new(match arg {
                Some(m) => DeepgramEngine::with_model(c, k, cloud::deepgram::DEFAULT_BASE_URL, m),
                None => DeepgramEngine::new(c, k),
            }))
        }
        "sarvam" => {
            let c = cloud::http_client()?;
            let k = env_key("SARVAM_API_KEY")?;
            Built::Other(Arc::new(match arg {
                Some(m) => SarvamEngine::with_model(c, k, cloud::sarvam::DEFAULT_BASE_URL, m),
                None => SarvamEngine::new(c, k),
            }))
        }
        other => bail!("unknown engine kind `{other}`"),
    })
}

/// USD per hour of audio for a cloud engine, given the clip durations (Groq bills ≥10 s
/// per request).
fn cloud_cost_per_hour(engine_id: &str, durations: &[f64], has_dictionary: bool) -> Option<f64> {
    let total: f64 = durations.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let billed = |min: f64| durations.iter().map(|d| d.max(min)).sum::<f64>();
    let (rate, billed_secs) = if engine_id.starts_with("groq:whisper-large-v3-turbo") {
        (cloud::groq::PRICE_USD_PER_HOUR, billed(cloud::groq::MIN_BILLED_SECONDS))
    } else if engine_id.starts_with("groq:") {
        (0.111, billed(cloud::groq::MIN_BILLED_SECONDS))
    } else if engine_id.starts_with("deepgram:") {
        let kt = if has_dictionary { cloud::deepgram::PRICE_USD_PER_HOUR_KEYTERM } else { 0.0 };
        (cloud::deepgram::PRICE_USD_PER_HOUR_MULTI + kt, total)
    } else if engine_id.starts_with("sarvam:") {
        (cloud::sarvam::PRICE_USD_PER_HOUR, total)
    } else {
        return None;
    };
    Some(rate * billed_secs / total)
}

pub(crate) struct Clip {
    name: String,
    category: String,
    audio: Vec<f32>,
    /// What was said, word for word (`<name>.txt`).
    reference: String,
    /// What the user would send (`<name>.formatted.txt`); the verbatim reference when
    /// the clip has none.
    formatted: String,
}

/// The suffix of a clip's formatted reference, next to `<name>.txt`.
const FORMATTED_SUFFIX: &str = ".formatted.txt";

/// A clip's verbatim reference and, if it has one, its formatted reference.
fn read_references(wav: &Path) -> Option<(String, Option<String>)> {
    let verbatim = std::fs::read_to_string(wav.with_extension("txt")).ok()?;
    let stem = wav.file_stem()?.to_string_lossy();
    let formatted = std::fs::read_to_string(wav.with_file_name(format!("{stem}{FORMATTED_SUFFIX}"))).ok();
    Some((verbatim.trim().to_owned(), formatted.map(|f| f.trim().to_owned())))
}

pub(crate) fn load_clips(dir: &Path) -> Result<Vec<Clip>> {
    let mut out = Vec::new();
    let mut unformatted = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) {
                let Some((reference, formatted)) = read_references(&p) else {
                    eprintln!("skipping {} (no {})", p.display(), p.with_extension("txt").display());
                    continue;
                };
                unformatted += usize::from(formatted.is_none());
                let category = p
                    .parent()
                    .and_then(|x| x.file_name())
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let name = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push(Clip {
                    name,
                    category,
                    audio: bolo_audio::codec::read_wav_16k_mono(&p)?,
                    formatted: formatted.unwrap_or_else(|| reference.clone()),
                    reference,
                });
            }
        }
    }
    if unformatted > 0 {
        eprintln!("{unformatted} clips have no {FORMATTED_SUFFIX}; their formatted WER uses the verbatim reference");
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// `variant<TAB>canonical` lines; accepted Hinglish spelling variants.
fn load_aliases(dir: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(dir.join("aliases.tsv"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, b)| (metrics::normalize(a), metrics::normalize(b)))
        .collect()
}

#[derive(Serialize)]
struct ClipRow {
    engine: String,
    clip: String,
    category: String,
    audio_s: f64,
    latency_ms: f64,
    rtf: f64,
    wer: f64,
    /// WER against the formatted reference (what the user would send).
    wer_formatted: f64,
    /// With `--polish`: WER of the polished text against each reference.
    wer_polished: Option<f64>,
    wer_formatted_polished: Option<f64>,
    cer: f64,
    devanagari: bool,
    cached: bool,
    hypothesis: String,
    /// The hypothesis actually scored: romanized when the engine emitted Devanagari,
    /// then formatted by the Layer 1 rules.
    hypothesis_scored: String,
    /// With `--polish`: the polished text.
    polished: String,
    reference: String,
    reference_formatted: String,
}

#[derive(Serialize, Default)]
struct EngineSummary {
    engine: String,
    clips: usize,
    failures: usize,
    wer_by_category: BTreeMap<String, f64>,
    wer_formatted_by_category: BTreeMap<String, f64>,
    /// With `--polish` only.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    wer_polished_by_category: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    wer_formatted_polished_by_category: BTreeMap<String, f64>,
    cer_by_category: BTreeMap<String, f64>,
    latency_p50_ms: Option<f64>,
    latency_p95_ms: Option<f64>,
    model_load_ms: Option<f64>,
    peak_rss_mb: Option<f64>,
    download_mb: Option<f64>,
    unpacked_mb: Option<f64>,
    usd_per_hour: Option<f64>,
    devanagari_outputs: usize,
}

async fn run(args: RunArgs, manifest: &Manifest, models_dir: &Path) -> Result<()> {
    let clips = load_clips(&args.clips)?;
    if clips.is_empty() {
        bail!("no clips found in {} (see docs/BENCH.md)", args.clips.display());
    }
    let aliases = load_aliases(&args.clips);
    let mut polisher = if args.polish { Some(polish::Polisher::from_env()?) } else { None };
    let cache = Cache::new(&args.cache_dir);
    std::fs::create_dir_all(&args.out)?;
    eprintln!("{} clips, {} engines", clips.len(), args.engines.len());

    let mut rows: Vec<ClipRow> = Vec::new();
    let mut summaries = Vec::new();

    for spec in &args.engines {
        let engine = match build_engine(spec, manifest, models_dir, args.threads) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("skipping {spec}: {e:#}");
                continue;
            }
        };
        let id = engine.id().to_owned();
        eprintln!("== {id}");
        let mut s = EngineSummary { engine: id.clone(), ..Default::default() };

        let sampler = rss::PeakSampler::start();
        if let Built::Local(l) = &engine {
            let e = l.entry();
            s.download_mb = e.download.as_ref().map(|d| d.size_bytes as f64 / 1e6);
            s.unpacked_mb = e.unpacked_size_bytes.map(|b| b as f64 / 1e6);
            let lc = l.clone();
            let load = tokio::task::spawn_blocking(move || lc.load_blocking()).await??;
            s.model_load_ms = Some(load.as_secs_f64() * 1e3);
        }

        let mut lat = Vec::new();
        let mut errs: BTreeMap<String, (WerSet, ErrorCount)> = BTreeMap::new();
        let mut durations = Vec::new();
        for clip in &clips {
            let opts = SttOptions {
                language: args.lang.resolve(&clip.category),
                script: HinglishScript::Roman,
                dictionary: args.dictionary.clone(),
            };
            let measured = match &engine {
                Built::Local(l) => measure_local(l, &clip.audio, &opts, args.runs).await,
                Built::Other(e) => measure_cloud(e.as_ref(), &cache, !args.no_cache, &clip.audio, &opts).await,
            };
            let (text, latency_ms, cached) = match measured {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("  {}: FAILED {e:#}", clip.name);
                    s.failures += 1;
                    continue;
                }
            };
            let audio_s = bolo_audio::duration_secs(&clip.audio);
            // Devanagari (or Urdu-script) output vs a Roman reference: score the
            // transliteration (what a Roman-Hinglish user gets from the app's translit pass).
            let dev = bolo_format::translit::needs_romanizing(&text);
            let romanized = if dev && !contains_devanagari(&clip.reference) {
                bolo_format::loanwords::to_roman_hinglish(&text)
            } else {
                text.clone()
            };
            // Layer 1 rules, as the engine applies them to a final (plain target), so
            // both WERs and polish see what the user would get.
            let scored = rules::apply(romanized.trim(), rules_language(opts.language, &text), FormatContext::plain());
            let polished = match polisher.as_mut() {
                Some(p) => Some(p.polish(&scored).await),
                None => None,
            };
            let w = WerSet::score(&clip.reference, &clip.formatted, &scored, polished.as_deref(), &aliases);
            let c = cer(&clip.reference, &scored);
            let entry = errs.entry(clip.category.clone()).or_default();
            entry.0.add(w);
            entry.1.add(c);
            lat.push(latency_ms);
            durations.push(audio_s);
            s.devanagari_outputs += usize::from(dev);
            eprintln!(
                "  {:<32} {:>6.0} ms  WER {:>5.1}%  fmt {:>5.1}%  {}",
                clip.name,
                latency_ms,
                w.verbatim.rate() * 100.0,
                w.formatted.rate() * 100.0,
                truncate(&text, 60)
            );
            rows.push(ClipRow {
                engine: id.clone(),
                clip: clip.name.clone(),
                category: clip.category.clone(),
                audio_s,
                latency_ms,
                rtf: latency_ms / 1e3 / audio_s.max(1e-6),
                wer: w.verbatim.rate(),
                wer_formatted: w.formatted.rate(),
                wer_polished: w.polished.map(|e| e.rate()),
                wer_formatted_polished: w.formatted_polished.map(|e| e.rate()),
                cer: c.rate(),
                devanagari: dev,
                cached,
                hypothesis_scored: scored,
                hypothesis: text,
                polished: polished.unwrap_or_default(),
                reference: clip.reference.clone(),
                reference_formatted: clip.formatted.clone(),
            });
        }

        if let Built::Local(l) = &engine {
            l.release();
            s.peak_rss_mb = Some(sampler.stop_delta_mb());
        } else {
            drop(sampler);
            s.usd_per_hour = cloud_cost_per_hour(&id, &durations, !args.dictionary.is_empty());
        }
        s.clips = lat.len();
        s.latency_p50_ms = percentile(&lat, 50.0);
        s.latency_p95_ms = percentile(&lat, 95.0);
        for (cat, (w, c)) in errs {
            s.wer_by_category.insert(cat.clone(), w.verbatim.rate());
            s.wer_formatted_by_category.insert(cat.clone(), w.formatted.rate());
            if let Some(p) = w.polished {
                s.wer_polished_by_category.insert(cat.clone(), p.rate());
            }
            if let Some(p) = w.formatted_polished {
                s.wer_formatted_polished_by_category.insert(cat.clone(), p.rate());
            }
            s.cer_by_category.insert(cat, c.rate());
        }
        summaries.push(s);
    }

    write_outputs(&args.out, &rows, &summaries)?;
    println!("{}", std::fs::read_to_string(args.out.join("summary.md"))?);
    if let Some(p) = &polisher {
        println!("{}", p.report());
    }
    Ok(())
}

/// Local latency = release-to-text. Streaming engines are fed in 100 ms chunks first
/// (that work happens while the user speaks); only `finish()` is timed. Batch engines
/// time the full decode. Median of `runs`.
async fn measure_local(l: &LocalEngine, audio: &[f32], opts: &SttOptions, runs: usize) -> Result<(String, f64, bool)> {
    let (l, audio, opts) = (l.clone(), audio.to_vec(), opts.clone());
    tokio::task::spawn_blocking(move || -> Result<(String, f64, bool)> {
        let mut times = Vec::new();
        let mut text = String::new();
        for _ in 0..runs.max(1) {
            if let Some(mut stream) = l.stream(&opts) {
                for chunk in audio.chunks(bolo_audio::SAMPLE_RATE as usize / 10) {
                    stream.accept(chunk);
                    let _ = stream.partial();
                }
                let t0 = Instant::now();
                text = stream.finish()?.text;
                times.push(t0.elapsed().as_secs_f64() * 1e3);
            } else {
                let t0 = Instant::now();
                text = l.transcribe_blocking(&audio, &opts)?.text;
                times.push(t0.elapsed().as_secs_f64() * 1e3);
            }
        }
        Ok((text, percentile(&times, 50.0).unwrap_or(0.0), false))
    })
    .await?
}

/// Cloud latency = request round-trip on a pre-warmed connection (as after hotkey-down).
async fn measure_cloud(
    e: &dyn SttEngine,
    cache: &Cache,
    use_cache: bool,
    audio: &[f32],
    opts: &SttOptions,
) -> Result<(String, f64, bool)> {
    let fp = format!("{:?}|{:?}|{}", opts.language, opts.script, opts.dictionary.join(","));
    let key = Cache::key(audio, &fp);
    if use_cache && let Some(c) = cache.get(e.id(), &key) {
        return Ok((c.text, c.latency_ms, true));
    }
    e.prewarm().await.ok();
    let t0 = Instant::now();
    let t = e.transcribe(audio, opts).await?;
    let latency_ms = t0.elapsed().as_secs_f64() * 1e3;
    cache.put(e.id(), &key, &Cached { text: t.text.clone(), latency_ms })?;
    Ok((t.text, latency_ms, false))
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

fn write_outputs(out: &Path, rows: &[ClipRow], summaries: &[EngineSummary]) -> Result<()> {
    let mut w = csv::Writer::from_path(out.join("clips.csv"))?;
    for r in rows {
        w.serialize(r)?;
    }
    w.flush()?;
    std::fs::write(out.join("summary.json"), serde_json::to_vec_pretty(summaries)?)?;

    let cats: Vec<String> = {
        let mut c: Vec<String> = summaries.iter().flat_map(|s| s.wer_by_category.keys().cloned()).collect();
        c.sort();
        c.dedup();
        c
    };
    let f = |v: Option<f64>, d: usize| v.map_or("–".to_owned(), |x| format!("{x:.d$}"));
    let pct = |v: Option<&f64>| v.map_or("–".to_owned(), |x| format!("{:.1}%", x * 100.0));
    let polished = summaries.iter().any(|s| !s.wer_polished_by_category.is_empty());
    let per_cat = if polished { 5 } else { 3 };
    let mut md = String::from("| engine | clips | fail |");
    for c in &cats {
        md += &format!(" WER {c} | fmt WER {c} |");
        if polished {
            md += &format!(" +polish WER {c} | +polish fmt WER {c} |");
        }
        md += &format!(" CER {c} |");
    }
    md += " p50 ms | p95 ms | load ms | peak RSS MB | download MB | $/hr | Devanagari |\n|---|---|---|";
    md += &"---|".repeat(per_cat * cats.len());
    md += "---|---|---|---|---|---|---|\n";
    for s in summaries {
        md += &format!("| {} | {} | {} |", s.engine, s.clips, s.failures);
        for c in &cats {
            md += &format!(" {} | {} |", pct(s.wer_by_category.get(c)), pct(s.wer_formatted_by_category.get(c)));
            if polished {
                md += &format!(
                    " {} | {} |",
                    pct(s.wer_polished_by_category.get(c)),
                    pct(s.wer_formatted_polished_by_category.get(c))
                );
            }
            md += &format!(" {} |", pct(s.cer_by_category.get(c)));
        }
        md += &format!(
            " {} | {} | {} | {} | {} | {} | {} |\n",
            f(s.latency_p50_ms, 0),
            f(s.latency_p95_ms, 0),
            f(s.model_load_ms, 0),
            f(s.peak_rss_mb, 0),
            f(s.download_mb, 0),
            s.usd_per_hour.map_or("local".to_owned(), |x| format!("{x:.3}")),
            s.devanagari_outputs
        );
    }
    std::fs::write(out.join("summary.md"), md)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groq_cost_applies_minimum() {
        // Two 2 s clips are billed as 20 s → 5× the list price per audio hour.
        let c = cloud_cost_per_hour("groq:whisper-large-v3-turbo", &[2.0, 2.0], false).unwrap();
        assert!((c - 0.04 * 5.0).abs() < 1e-9);
        assert!(cloud_cost_per_hour("local:x", &[2.0], false).is_none());
    }

    #[test]
    fn reads_formatted_reference_next_to_verbatim() {
        let dir = std::env::temp_dir().join(format!("bolo-refs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("01-x.wav");
        assert_eq!(read_references(&wav), None, "no .txt, no clip");
        std::fs::write(dir.join("01-x.txt"), "twelve fifty dollars\n").unwrap();
        assert_eq!(read_references(&wav), Some(("twelve fifty dollars".into(), None)));
        std::fs::write(dir.join("01-x.formatted.txt"), "$1,250\n").unwrap();
        assert_eq!(read_references(&wav), Some(("twelve fifty dollars".into(), Some("$1,250".into()))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_bench_clip_has_a_formatted_reference() {
        let clips = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/clips");
        let mut n = 0;
        for cat in ["en", "hinglish"] {
            for e in std::fs::read_dir(clips.join(cat)).unwrap() {
                let p = e.unwrap().path();
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                if let Some(stem) = name.strip_suffix(".txt").filter(|s| !s.ends_with(".formatted")) {
                    let f = p.with_file_name(format!("{stem}{FORMATTED_SUFFIX}"));
                    let text = std::fs::read_to_string(&f).unwrap_or_else(|_| panic!("missing {}", f.display()));
                    assert!(!text.trim().is_empty(), "{} is empty", f.display());
                    n += 1;
                }
            }
        }
        assert_eq!(n, 40);
    }

    #[test]
    fn lang_by_folder() {
        assert_eq!(LangArg::ByFolder.resolve("en"), Language::En);
        assert_eq!(LangArg::ByFolder.resolve("hinglish"), Language::Auto);
    }
}
