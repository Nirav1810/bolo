//! Audio sources for the engine: the live microphone, or a WAV replayed in real time
//! (for end-to-end latency tests without a human speaking).

use std::sync::Arc;
use std::time::Instant;

use bolo_audio::capture::Capture;

use crate::BoloError;

/// Something that produces mono audio. Created and used on the engine thread only.
pub trait AudioSource {
    fn sample_rate(&self) -> u32;
    /// Append everything captured since the last call (mono, at `sample_rate`).
    fn drain_mono(&mut self, out: &mut Vec<f32>);
    /// Recent input level (RMS, 0..1) for the waveform UI.
    fn level(&self) -> f32;
    /// The device this source records from, for the user.
    fn device_name(&self) -> String {
        String::new()
    }
    /// Opened on the default device because the one asked for isn't plugged in.
    fn fell_back(&self) -> bool {
        false
    }
    /// The source stopped for good (its device went away): the engine opens a new one.
    fn failed(&self) -> bool {
        false
    }
    /// After [`Self::fell_back`]: whether the device asked for is plugged in again. The
    /// engine asks every few seconds while the mic is warm; it may list the devices.
    fn wanted_is_back(&self) -> bool {
        false
    }
}

/// Opens a source on the input device named by its argument (`None`: the default).
/// Called on the engine thread when the mic needs to (re)open.
pub type AudioFactory = Arc<dyn Fn(Option<&str>) -> Result<Box<dyn AudioSource>, BoloError> + Send + Sync>;

/// The live microphone: the device chosen by name, or the default.
pub struct MicSource {
    capture: Capture,
    wanted: Option<String>,
}

impl MicSource {
    /// Open `device` (`None`: the default), falling back to the default when it isn't
    /// plugged in.
    pub fn open(device: Option<&str>) -> Result<Self, BoloError> {
        let capture = Capture::start(device).map_err(|e| BoloError::Audio(e.to_string()))?;
        Ok(Self { capture, wanted: device.map(str::to_owned) })
    }

    pub fn factory() -> AudioFactory {
        Arc::new(|device| Ok(Box::new(MicSource::open(device)?) as Box<dyn AudioSource>))
    }
}

impl AudioSource for MicSource {
    fn sample_rate(&self) -> u32 {
        self.capture.sample_rate()
    }
    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        self.capture.drain_mono(out);
    }
    fn level(&self) -> f32 {
        self.capture.level()
    }
    fn device_name(&self) -> String {
        self.capture.device_name().to_owned()
    }
    fn fell_back(&self) -> bool {
        self.capture.fell_back()
    }
    fn failed(&self) -> bool {
        self.capture.failed()
    }
    fn wanted_is_back(&self) -> bool {
        self.wanted.as_deref().is_some_and(bolo_audio::capture::is_available)
    }
}

/// Replays 16 kHz mono samples at real-time speed from the first `drain_mono` call,
/// then delivers silence (like a mic in a quiet room).
pub struct WavSource {
    samples: Vec<f32>,
    pos: usize,
    started: Option<Instant>,
    level: f32,
    speed: f64,
}

impl WavSource {
    pub fn new(samples: Vec<f32>) -> Self {
        Self::with_speed(samples, 1.0)
    }

    /// Replays `speed` times faster than real time (quick accuracy checks; latency is
    /// only realistic at 1). Above real time nothing is delivered after the samples end:
    /// sped-up silence would pile up faster than the engine can decode it.
    pub fn with_speed(samples: Vec<f32>, speed: f64) -> Self {
        Self { samples, pos: 0, started: None, level: 0.0, speed: speed.max(f64::MIN_POSITIVE) }
    }

    /// A factory that replays `samples` each time the engine opens the "mic".
    pub fn factory(samples: Arc<Vec<f32>>) -> AudioFactory {
        Arc::new(move |_| Ok(Box::new(WavSource::new(samples.as_ref().clone())) as Box<dyn AudioSource>))
    }
}

impl AudioSource for WavSource {
    fn sample_rate(&self) -> u32 {
        bolo_audio::SAMPLE_RATE
    }

    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        let started = *self.started.get_or_insert_with(Instant::now);
        let due = (started.elapsed().as_secs_f64() * self.speed * f64::from(bolo_audio::SAMPLE_RATE)) as usize;
        if due <= self.pos {
            return;
        }
        let due = if self.speed > 1.0 { due.min(self.samples.len()) } else { due };
        if due <= self.pos {
            return;
        }
        let n = due - self.pos;
        let from_file = self.samples.len().saturating_sub(self.pos).min(n);
        let chunk = &self.samples[self.pos.min(self.samples.len())..self.pos.min(self.samples.len()) + from_file];
        self.level = bolo_audio::rms(chunk);
        out.extend_from_slice(chunk);
        out.extend(std::iter::repeat_n(0.0, n - from_file));
        self.pos = due;
    }

    fn level(&self) -> f32 {
        self.level
    }
}
