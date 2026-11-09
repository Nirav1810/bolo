//! `bolo dictate`: try Bolo from the terminal on the same engine the apps use.
//!
//! The command drives a [`bolo_core::Engine`] and renders its events. The engine brings
//! the warm mic and pre-roll, VAD segment-ahead decoding for batch engines, and the
//! language mode, so what you try here is what ships.
//!
//! - Press Enter to start a dictation and Enter again to stop it (`c` + Enter cancels).
//! - Partials show live. The final text goes to the clipboard, ready to paste.
//! - `--polish` runs the Claude rewrite only after the instant text is on the clipboard,
//!   then replaces it (ADR-012). Each result is shown with its own latency.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use bolo_core::{
    BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput, LanguageMode, TargetContext,
};
use bolo_models::Manifest;

use crate::record::{Key, meter, stdin_lines};

/// How long to wait for the final text after stopping before giving up.
const FINAL_TIMEOUT: Duration = Duration::from_secs(20);
/// Polish's hard deadline. The app uses 1.5 s; the terminal shows the latency anyway.
const POLISH_TIMEOUT: Duration = Duration::from_millis(3000);

enum Ev {
    Level(f32),
    Partial(String),
    Final(FormattedOutput),
    Error(BoloError),
}

/// Forwards engine callbacks (on the engine thread) to the terminal loop.
struct Events(Mutex<Sender<Ev>>);

impl Events {
    fn send(&self, ev: Ev) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(ev);
        }
    }
}

impl EngineEvents for Events {
    fn on_level(&self, rms: f32) {
        self.send(Ev::Level(rms));
    }
    fn on_partial(&self, text: String) {
        self.send(Ev::Partial(text));
    }
    fn on_final(&self, out: FormattedOutput) {
        self.send(Ev::Final(out));
    }
    fn on_error(&self, err: BoloError) {
        self.send(Ev::Error(err));
    }
    fn on_state(&self, _: DictationState) {}
}

/// `-e` accepts a bare local model id (`nemotron-stream`) as well as full engine specs
/// (`local:<id>`, `deepgram`, `deepgram-stream`, `groq[:model]`, `sarvam[:model]`).
pub(crate) fn engine_spec(arg: &str) -> String {
    let kind = arg.split(':').next().unwrap_or_default();
    if arg.contains(':') || ["deepgram", "deepgram-stream", "groq", "sarvam"].contains(&kind) {
        arg.to_owned()
    } else {
        format!("local:{arg}")
    }
}

fn tail(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n { s.to_owned() } else { format!("…{}", c[c.len() - n..].iter().collect::<String>()) }
}

fn clear_line() {
    print!("\r{:<100}\r", "");
}

pub async fn run(
    manifest: &Manifest,
    models_dir: &Path,
    engine_arg: &str,
    language: LanguageMode,
    polish: bool,
) -> Result<()> {
    let spec = engine_spec(engine_arg);
    let config = EngineConfig {
        engines: vec![spec.clone()],
        language,
        models_dir: models_dir.to_path_buf(),
        api_keys: crate::api_keys_from_env(),
        ..Default::default()
    };
    // Fail now, with a useful message, rather than on the first Enter. Building does not
    // load the model or open a connection.
    if let Err(e) = bolo_core::build_stt(&config, manifest, &bolo_stt::cloud::http_client()?) {
        return Err(match e {
            BoloError::ModelMissing(id) => anyhow!("{id} is not installed; run `bolo download {id}`"),
            BoloError::Config(m) if m.starts_with("no API key") => {
                let provider = spec.split(':').next().unwrap_or_default().to_uppercase();
                anyhow!("{m}: put it in .env as {provider}_API_KEY=...")
            }
            other => anyhow!("{other}"),
        });
    }
    let anthropic = if polish {
        let k = crate::env_nonempty("ANTHROPIC_API_KEY").context("--polish needs ANTHROPIC_API_KEY in .env")?;
        Some((k, bolo_stt::cloud::http_client()?))
    } else {
        None
    };

    let (tx, events) = mpsc::channel();
    let engine = Engine::new(config, Box::new(Events(Mutex::new(tx))))?;
    engine.prewarm(); // load the model in the background while the prompt is up
    let warming = if spec.starts_with("local:") {
        "the model loads in the background"
    } else {
        "the connection opens as each dictation starts"
    };
    println!("{spec}, language mode {language:?}: {warming}.");

    let mut clipboard = arboard::Clipboard::new().ok();
    let keys = stdin_lines();
    loop {
        print!("\nPress Enter and speak (q + Enter to quit) > ");
        std::io::stdout().flush()?;
        match keys.recv() {
            Ok(Key::Line(k)) if k == "q" => break,
            Ok(Key::Line(_)) => {}
            Ok(Key::Eof) | Err(_) => break,
        }
        // Errors from prewarm or a stale event from the last dictation.
        for ev in events.try_iter() {
            if let Ev::Error(e) = ev {
                println!("  (engine: {e})");
            }
        }
        if let Err(e) = engine.start_dictation(TargetContext::default()) {
            println!("  could not start: {e}");
            continue;
        }

        let Some(stopped) = listen(&keys, &events)? else {
            engine.cancel_dictation();
            clear_line();
            println!("  (cancelled)");
            continue;
        };
        // ── key released: everything below is user-visible latency ──
        let t_release = Instant::now();
        engine.stop_dictation()?;
        let out = wait_final(&events)?;
        let instant_ms = t_release.elapsed().as_secs_f64() * 1e3;
        clear_line();
        let out = match out {
            Ok(o) => o,
            Err(e) => {
                println!("  failed: {e}");
                continue;
            }
        };
        if out.plain.is_empty() {
            println!("  (nothing heard)");
            continue;
        }
        let copied = clipboard.as_mut().is_some_and(|c| c.set_text(out.plain.clone()).is_ok());
        println!("  {}", out.plain);
        println!(
            "  [{} · {:?} · {instant_ms:.0} ms after you stopped (engine {:.0} ms){}]",
            out.engine_id,
            out.language,
            out.timings.total_ms,
            if copied { " · copied to clipboard, paste with Ctrl+V" } else { "" }
        );
        if let Some((key, client)) = &anthropic {
            let req = bolo_format::llm::CleanupRequest {
                transcript: &out.plain,
                script: bolo_format::llm::Script::Roman,
                tone: "neutral",
                app: "Notes",
                dictionary: &[],
            };
            match bolo_format::llm::cleanup(client, key, &req, POLISH_TIMEOUT).await {
                Ok(r) => {
                    let copied = clipboard.as_mut().is_some_and(|c| c.set_text(r.text.clone()).is_ok());
                    println!("  ✨ {}", r.text);
                    println!("  [polish +{:.0} ms{}]", r.latency_ms, if copied { " · clipboard updated" } else { "" });
                }
                Err(e) => println!("  (polish skipped, instant text kept: {e})"),
            }
        }
        if stopped == Stop::Quit {
            break;
        }
    }
    drop(engine); // closes the mic and unloads the model
    Ok(())
}

#[derive(PartialEq)]
enum Stop {
    Stop,
    /// Stdin closed while listening: finish this dictation, then exit.
    Quit,
}

/// Render the dictation live until Enter. `None` means cancel (`c` + Enter).
fn listen(keys: &Receiver<Key>, events: &Receiver<Ev>) -> Result<Option<Stop>> {
    let started = Instant::now();
    let (mut shown, mut level) = (String::new(), 0.0_f32);
    loop {
        for ev in events.try_iter() {
            match ev {
                Ev::Level(l) => level = l,
                Ev::Partial(p) => shown = p,
                Ev::Error(e) => println!("\n  (engine: {e})"),
                Ev::Final(_) => {}
            }
        }
        print!(
            "\r● {:>4.1}s  {:<80}",
            started.elapsed().as_secs_f32(),
            if shown.is_empty() {
                format!("{} listening… (Enter to stop, c + Enter to cancel)", meter(level))
            } else {
                tail(&shown, 78)
            }
        );
        std::io::stdout().flush()?;
        match keys.recv_timeout(Duration::from_millis(20)) {
            Ok(Key::Line(k)) if k == "c" => return Ok(None),
            Ok(Key::Line(_)) => return Ok(Some(Stop::Stop)),
            Ok(Key::Eof) | Err(RecvTimeoutError::Disconnected) => return Ok(Some(Stop::Quit)),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// The dictation's final text, or the error the engine reported instead.
fn wait_final(events: &Receiver<Ev>) -> Result<Result<FormattedOutput, BoloError>> {
    let deadline = Instant::now() + FINAL_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(Ev::Final(o)) => return Ok(Ok(o)),
            Ok(Ev::Error(e)) => return Ok(Err(e)),
            Ok(Ev::Level(_) | Ev::Partial(_)) => {}
            Err(RecvTimeoutError::Timeout) => return Err(anyhow!("no result {FINAL_TIMEOUT:?} after stopping")),
            Err(RecvTimeoutError::Disconnected) => return Err(anyhow!("the engine stopped")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::engine_spec;

    #[test]
    fn engine_specs_accept_bare_model_ids() {
        assert_eq!(engine_spec("nemotron-stream"), "local:nemotron-stream");
        assert_eq!(engine_spec("local:parakeet-v2-en"), "local:parakeet-v2-en");
        assert_eq!(engine_spec("deepgram"), "deepgram");
        assert_eq!(engine_spec("deepgram-stream"), "deepgram-stream");
        assert_eq!(engine_spec("groq:whisper-large-v3"), "groq:whisper-large-v3");
        assert_eq!(engine_spec("sarvam"), "sarvam");
    }
}
