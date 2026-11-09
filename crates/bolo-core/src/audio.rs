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
}

/// Opens a source. Called on the engine thread when the mic needs to (re)open.
pub type AudioFactory = Arc<dyn Fn() -> Result<Box<dyn AudioSource>, BoloError> + Send + Sync>;

pub struct MicSource(Capture);

impl MicSource {
    pub fn open() -> Result<Self, BoloError> {
        Capture::start_default().map(Self).map_err(|e| BoloError::Audio(e.to_string()))
    }

    pub fn factory() -> AudioFactory {
        Arc::new(|| Ok(Box::new(MicSource::open()?) as Box<dyn AudioSource>))
    }
}

impl AudioSource for MicSource {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }
    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        self.0.drain_mono(out);
    }
    fn level(&self) -> f32 {
        self.0.level()
    }
}

/// Replays 16 kHz mono samples at real-time speed from the first `drain_mono` call,
/// then delivers silence (like a mic in a quiet room).
pub struct WavSource {
    samples: Vec<f32>,
    pos: usize,
    started: Option<Instant>,
    level: f32,
}

impl WavSource {
    pub fn new(samples: Vec<f32>) -> Self {
        Self { samples, pos: 0, started: None, level: 0.0 }
    }

    /// A factory that replays `samples` each time the engine opens the "mic".
    pub fn factory(samples: Arc<Vec<f32>>) -> AudioFactory {
        Arc::new(move || Ok(Box::new(WavSource::new(samples.as_ref().clone())) as Box<dyn AudioSource>))
    }
}

impl AudioSource for WavSource {
    fn sample_rate(&self) -> u32 {
        bolo_audio::SAMPLE_RATE
    }

    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        let started = *self.started.get_or_insert_with(Instant::now);
        let due = (started.elapsed().as_secs_f64() * f64::from(bolo_audio::SAMPLE_RATE)) as usize;
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
