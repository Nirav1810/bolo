//! `bolo record`: guided recording of benchmark clips.
//!
//! Shows one prompt at a time; Enter starts, Enter stops. Saves
//! `<clips>/<set>/NN-slug.wav` (16 kHz mono) and the prompt as `NN-slug.txt`.
//! Already-recorded prompts are skipped, so it can be stopped and resumed anytime.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bolo_audio::capture::Capture;

pub(crate) enum Key {
    Line(String),
    Eof,
}

/// Read stdin lines on a helper thread so the level meter can keep updating.
pub(crate) fn stdin_lines() -> mpsc::Receiver<Key> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(l) = line else { break };
            if tx.send(Key::Line(l.trim().to_lowercase())).is_err() {
                return;
            }
        }
        let _ = tx.send(Key::Eof);
    });
    rx
}

fn slug(text: &str) -> String {
    let words: Vec<String> = text
        .split_whitespace()
        .take(4)
        .map(|w| w.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    words.join("-")
}

pub(crate) fn meter(level: f32) -> String {
    // ~-60 dBFS .. 0 dBFS mapped to 30 cells.
    let db = 20.0 * level.max(1e-6).log10();
    let cells = (((db + 60.0) / 60.0).clamp(0.0, 1.0) * 30.0) as usize;
    format!("[{}{}]", "#".repeat(cells), " ".repeat(30 - cells))
}

pub fn run(prompts: &Path, clips: &Path, set: &str) -> Result<()> {
    let text = std::fs::read_to_string(prompts).with_context(|| format!("reading {}", prompts.display()))?;
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if lines.is_empty() {
        bail!("no prompts in {}", prompts.display());
    }
    let dir = clips.join(set);
    std::fs::create_dir_all(&dir)?;
    let keys = stdin_lines();

    println!("Recording set `{set}` → {}", dir.display());
    println!("Speak naturally, the way you'd dictate. If you say something different from the");
    println!("prompt, type `r` to redo (or fix the .txt afterwards).\n");

    let mut i = 0;
    while i < lines.len() {
        let prompt = lines[i];
        let stem = format!("{:02}-{}", i + 1, slug(prompt));
        let wav = dir.join(format!("{stem}.wav"));
        if wav.exists() {
            i += 1;
            continue;
        }
        println!("[{}/{}]  {}", i + 1, lines.len(), prompt);
        print!("        Enter = start   s = skip   q = quit > ");
        std::io::stdout().flush()?;
        match keys.recv() {
            Ok(Key::Line(k)) if k == "q" => break,
            Ok(Key::Line(k)) if k == "s" => {
                i += 1;
                println!();
                continue;
            }
            Ok(Key::Line(_)) => {}
            Ok(Key::Eof) | Err(_) => break,
        }

        let mut cap = Capture::start_default()?;
        let mut device_audio = Vec::new();
        // Many mic drivers emit ~0.5 s of digital silence after the stream opens (see
        // `mic-test`). Don't say "speak now" until real samples flow, and drop the dead
        // lead-in so clips start with genuine room tone.
        let warm = Instant::now();
        print!("\r        … opening mic");
        std::io::stdout().flush()?;
        loop {
            cap.drain_mono(&mut device_audio);
            if let Some(i) = device_audio.iter().position(|s| *s != 0.0) {
                device_audio.drain(..i);
                break;
            }
            device_audio.clear();
            if warm.elapsed() > Duration::from_secs(3) {
                bail!(
                    "microphone delivered only silence for 3 s — is it muted or blocked in Windows privacy settings?"
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let t0 = Instant::now();
        let stop = loop {
            cap.drain_mono(&mut device_audio);
            print!(
                "\r        ● REC — speak now {:>4.1}s {}  Enter = stop ",
                t0.elapsed().as_secs_f32(),
                meter(cap.level())
            );
            std::io::stdout().flush()?;
            match keys.recv_timeout(Duration::from_millis(50)) {
                Ok(Key::Line(_)) => break false,
                Ok(Key::Eof) => break true,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break true,
            }
        };
        let rate = cap.sample_rate();
        let dropped = cap.dropped();
        cap.stop(&mut device_audio);
        let audio = bolo_audio::resample::to_16k(&device_audio, rate)?;
        let secs = bolo_audio::duration_secs(&audio);
        let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        println!();

        if secs < 0.5 || peak < 0.01 {
            println!("        too short or silent (peak {peak:.3}) — is the right mic selected? Let's redo.\n");
            continue;
        }
        if dropped > 0 {
            println!("        warning: {dropped} samples dropped");
        }
        std::fs::write(&wav, bolo_audio::codec::encode_wav_pcm16(&audio))?;
        std::fs::write(dir.join(format!("{stem}.txt")), format!("{prompt}\n"))?;
        print!(
            "        saved {secs:.1}s{}   Enter = next   r = redo > ",
            if peak > 0.99 { " (clipping! move back a bit)" } else { "" }
        );
        std::io::stdout().flush()?;
        match keys.recv() {
            Ok(Key::Line(k)) if k == "r" => {
                std::fs::remove_file(&wav).ok();
                std::fs::remove_file(dir.join(format!("{stem}.txt"))).ok();
            }
            Ok(Key::Line(_)) => i += 1,
            Ok(Key::Eof) | Err(_) => break,
        }
        println!();
        if stop {
            break;
        }
    }
    let done = std::fs::read_dir(&dir)?
        .filter(|e| e.as_ref().is_ok_and(|e| e.path().extension().is_some_and(|x| x == "wav")))
        .count();
    println!("\n{done}/{} clips recorded in {}", lines.len(), dir.display());
    Ok(())
}

/// Measure microphone start-up delay: how long the device delivers digital silence after
/// the stream opens. Cold (first open), warm re-open, and a kept-open stream.
pub fn mic_test() -> Result<()> {
    fn first_signal(cap: &mut Capture, limit: Duration) -> (Option<Duration>, Duration) {
        let t0 = Instant::now();
        let mut seen = 0usize;
        let mut buf = Vec::new();
        while t0.elapsed() < limit {
            cap.drain_mono(&mut buf);
            if let Some(i) = buf.iter().position(|s| *s != 0.0) {
                let secs = (seen + i) as f64 / f64::from(cap.sample_rate());
                return (Some(Duration::from_secs_f64(secs)), t0.elapsed());
            }
            seen += buf.len();
            buf.clear();
            std::thread::sleep(Duration::from_millis(5));
        }
        (None, t0.elapsed())
    }
    for (label, pause) in [("cold open", 0u64), ("re-open after 0.2 s", 200), ("re-open after 3 s", 3000)] {
        std::thread::sleep(Duration::from_millis(pause));
        let t_open = Instant::now();
        let mut cap = Capture::start_default()?;
        let open_ms = t_open.elapsed().as_millis();
        let (zeros, _) = first_signal(&mut cap, Duration::from_secs(3));
        println!(
            "{label:<22} device `{}` @ {} Hz: open() {open_ms} ms, leading digital silence {}",
            cap.device_name(),
            cap.sample_rate(),
            zeros.map_or("> 3 s (muted?)".to_owned(), |d| format!("{} ms", d.as_millis()))
        );
        let mut rest = Vec::new();
        cap.stop(&mut rest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Can you send me the report?"), "can-you-send-me");
        assert_eq!(slug("Haan theek hai, kal milte hain."), "haan-theek-hai-kal");
    }

    #[test]
    fn meter_bounds() {
        assert_eq!(meter(0.0), format!("[{}]", " ".repeat(30)));
        assert_eq!(meter(1.0), format!("[{}]", "#".repeat(30)));
    }
}
