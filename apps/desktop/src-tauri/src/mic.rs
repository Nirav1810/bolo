//! Onboarding's mic check: open the chosen microphone cold, the way a dictation's
//! first open does, and measure how long it delivers digital silence (exact zeros)
//! before real sound. Windows Audio enhancements often add 200 ms or more, which clips
//! the first word (ADR-009); `bolo mic-test` measures the same thing.

use std::time::{Duration, Instant};

use bolo_audio::capture::Capture;
use serde::Serialize;

/// Above this much leading silence onboarding warns about Audio enhancements.
pub const LATE_MS: u64 = 200;

/// How long the check listens.
pub const LISTEN: Duration = Duration::from_secs(3);

/// RMS above this means the mic hears something (speech sits around 0.01–0.2).
const HEARD_RMS: f32 = 0.003;

/// What the check found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MicCheck {
    pub device: String,
    /// The chosen microphone isn't plugged in, so `device` is the Windows default.
    pub fell_back: bool,
    /// Digital silence after the stream opened, in ms; `None` if it never ended.
    pub leading_ms: Option<u64>,
    /// The loudest moment, as RMS over ~50 ms.
    pub peak_rms: f32,
    /// Something above room noise was heard.
    pub heard: bool,
    /// The leading silence is long enough to clip a first word.
    pub late: bool,
}

/// The index of the first sample that isn't exactly zero.
pub fn first_signal(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|s| *s != 0.0)
}

/// The verdict from the leading silence and the loudest moment.
pub fn verdict(device: String, leading: Option<Duration>, peak_rms: f32) -> MicCheck {
    let leading_ms = leading.map(|d| d.as_millis() as u64);
    MicCheck {
        device,
        fell_back: false,
        leading_ms,
        peak_rms,
        heard: peak_rms >= HEARD_RMS,
        late: leading_ms.is_none_or(|ms| ms > LATE_MS),
    }
}

/// Run the check on `device` (`None`: the Windows default), calling `level` with the RMS
/// as it listens. Blocks for [`LISTEN`].
pub fn check(device: Option<&str>, level: impl Fn(f32)) -> Result<MicCheck, String> {
    let mut cap = Capture::start(device).map_err(|e| e.to_string())?;
    let rate = f64::from(cap.sample_rate());
    let t0 = Instant::now();
    let (mut seen, mut leading, mut peak) = (0usize, None, 0f32);
    let mut buf = Vec::new();
    while t0.elapsed() < LISTEN {
        std::thread::sleep(Duration::from_millis(50));
        buf.clear();
        cap.drain_mono(&mut buf);
        if leading.is_none()
            && let Some(i) = first_signal(&buf)
        {
            leading = Some(Duration::from_secs_f64((seen + i) as f64 / rate));
        }
        seen += buf.len();
        let rms = bolo_audio::rms(&buf);
        peak = peak.max(rms);
        level(rms);
    }
    let (device, fell_back) = (cap.device_name().to_owned(), cap.fell_back());
    let mut rest = Vec::new();
    cap.stop(&mut rest);
    Ok(MicCheck { fell_back, ..verdict(device, leading, peak) })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{first_signal, verdict};

    #[test]
    fn leading_silence_is_the_run_of_exact_zeros() {
        assert_eq!(first_signal(&[0.0, 0.0, 1e-6, 0.0]), Some(2));
        assert_eq!(first_signal(&[0.0; 4]), None);
        assert_eq!(first_signal(&[]), None);
    }

    #[test]
    fn late_over_200_ms_or_never() {
        let v = |ms: Option<u64>, peak| verdict("Mic".into(), ms.map(Duration::from_millis), peak);
        assert!(!v(Some(40), 0.05).late);
        assert!(!v(Some(200), 0.05).late);
        assert!(v(Some(340), 0.05).late);
        assert!(v(None, 0.0).late);
        assert!(v(Some(40), 0.05).heard);
        assert!(!v(Some(40), 0.0005).heard);
        assert_eq!(v(Some(340), 0.05).leading_ms, Some(340));
    }

    /// The real default microphone. Run by hand: `cargo test -p bolo-desktop mic -- --ignored`.
    #[test]
    #[ignore = "opens the real microphone"]
    fn live_check_on_the_default_mic() {
        let levels = std::sync::atomic::AtomicUsize::new(0);
        let r = super::check(None, |_| {
            levels.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .unwrap();
        println!("{r:?}");
        assert!(levels.into_inner() >= 40, "about one level per 50 ms");
    }
}
