//! `bolo dictate`: try Bolo's instant path from the terminal.
//!
//! Enter starts, Enter stops. While you speak, mic audio streams (lock-free capture →
//! streaming resampler) into a streaming STT engine that decodes as you go. After you
//! stop, only the last chunk is flushed, then Devanagari is romanized with English
//! loanwords restored. The result is copied to the clipboard, ready to paste anywhere.
//!
//! With `--polish` and `ANTHROPIC_API_KEY`, the Claude cleanup runs afterwards and
//! replaces the clipboard text. It is shown separately so its extra latency is visible.

use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bolo_audio::capture::Capture;
use bolo_audio::resample::StreamResampler;
use bolo_models::Manifest;
use bolo_stt::{Language, SttOptions};

use crate::Built;
use crate::record::{Key, meter, stdin_lines};

fn romanize(text: &str) -> String {
    if bolo_format::translit::has_devanagari(text) {
        bolo_format::loanwords::to_roman_hinglish(text)
    } else {
        text.to_owned()
    }
}

fn tail(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n { s.to_owned() } else { format!("…{}", c[c.len() - n..].iter().collect::<String>()) }
}

pub async fn run(
    manifest: &Manifest,
    models_dir: &std::path::Path,
    engine_spec: &str,
    lang: Language,
    polish: bool,
) -> Result<()> {
    // "deepgram" / "groq[:model]" / "sarvam" / "local:<id>" / bare local model id.
    let kind = engine_spec.split(':').next().unwrap_or_default();
    let spec = if engine_spec.contains(':') || ["deepgram", "groq", "sarvam"].contains(&kind) {
        engine_spec.to_owned()
    } else {
        format!("local:{engine_spec}")
    };
    let engine = crate::build_engine(&spec, manifest, models_dir, None)?;
    match &engine {
        Built::Local(l) => {
            print!("Loading {}… ", l.entry().id);
            std::io::stdout().flush()?;
            let load = l.load_blocking()?;
            println!("ready ({:.1} s).", load.as_secs_f64());
        }
        Built::Other(e) => {
            println!("Cloud engine {}: audio is sent when you stop; the connection is opened as you start.", e.id());
        }
    }

    let anthropic_key = if polish {
        let k = crate::env_nonempty("ANTHROPIC_API_KEY").context("--polish needs ANTHROPIC_API_KEY in .env")?;
        Some((k, bolo_stt::cloud::http_client()?))
    } else {
        None
    };
    let mut clipboard = arboard::Clipboard::new().ok();
    let opts = SttOptions { language: lang, ..Default::default() };
    let keys = stdin_lines();

    loop {
        print!("\nPress Enter and speak (q + Enter to quit) > ");
        std::io::stdout().flush()?;
        match keys.recv() {
            Ok(Key::Line(k)) if k == "q" => break,
            Ok(Key::Line(_)) => {}
            Ok(Key::Eof) | Err(_) => break,
        }

        let mut cap = Capture::start_default()?;
        // Like hotkey-down in the app: open the TLS connection while the user speaks.
        if let Built::Other(e) = &engine {
            let e = e.clone();
            tokio::spawn(async move { e.prewarm().await });
        }
        let mut rs = StreamResampler::new(cap.sample_rate())?;
        let streaming = engine.supports_streaming();
        let mut stream = if streaming { engine.stream(&opts) } else { None };
        let mut batch: Vec<f32> = Vec::new(); // non-streaming engines decode at release
        let (mut dev, mut pcm) = (Vec::new(), Vec::new());
        let mut shown = String::new();
        let t_start = Instant::now();

        loop {
            cap.drain_mono(&mut dev);
            rs.push(&dev, &mut pcm)?;
            dev.clear();
            if !pcm.is_empty() {
                match stream.as_mut() {
                    Some(s) => {
                        s.accept(&pcm);
                        if let Some(p) = s.partial() {
                            shown = romanize(&p);
                        }
                    }
                    None => batch.extend_from_slice(&pcm),
                }
                pcm.clear();
            }
            print!(
                "\r● {:>4.1}s  {:<70}",
                t_start.elapsed().as_secs_f32(),
                if !shown.is_empty() {
                    tail(&shown, 68)
                } else if streaming {
                    "listening… (Enter to stop)".to_owned()
                } else {
                    format!("{} listening… (Enter to stop)", meter(cap.level()))
                }
            );
            std::io::stdout().flush()?;
            match keys.recv_timeout(Duration::from_millis(20)) {
                Ok(_) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }

        // ── key released: everything below is user-visible latency ──
        let t_release = Instant::now();
        cap.stop(&mut dev);
        rs.push(&dev, &mut pcm)?;
        rs.flush(&mut pcm)?;
        let raw = match stream.take() {
            Some(mut s) => {
                s.accept(&pcm);
                s.finish()?.text
            }
            None => {
                batch.extend_from_slice(&pcm);
                engine.transcribe(&batch, &opts).await?.text
            }
        };
        let text = romanize(raw.trim());
        let label = engine.id().to_owned();
        let instant_ms = t_release.elapsed().as_secs_f64() * 1e3;
        let copied = clipboard.as_mut().is_some_and(|c| c.set_text(text.clone()).is_ok());
        println!("\r{:<80}", "");
        if text.is_empty() {
            println!("(nothing heard)");
            continue;
        }
        println!("  {text}");
        println!(
            "  [{label} · {instant_ms:.0} ms after you stopped{}]",
            if copied { " · copied to clipboard, paste with Ctrl+V" } else { "" }
        );

        if let Some((key, client)) = &anthropic_key {
            let req = bolo_format::llm::CleanupRequest {
                transcript: &raw,
                script: bolo_format::llm::Script::Roman,
                tone: "neutral",
                app: "Notes",
                dictionary: &[],
            };
            match bolo_format::llm::cleanup(client, key, &req, Duration::from_millis(3000)).await {
                Ok(r) => {
                    let copied = clipboard.as_mut().is_some_and(|c| c.set_text(r.text.clone()).is_ok());
                    println!("  ✨ {}", r.text);
                    println!("  [polish +{:.0} ms{}]", r.latency_ms, if copied { " · clipboard updated" } else { "" });
                }
                Err(e) => println!("  (polish skipped: {e})"),
            }
        }
    }
    engine.release();
    Ok(())
}
