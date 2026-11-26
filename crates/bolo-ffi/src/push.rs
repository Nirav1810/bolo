//! Audio pushed in from the host (Android's `AudioRecord`, ADR-0023).
//!
//! Kotlin calls [`PushRing::push`] with ~20 ms PCM16 frames from its recording thread. The
//! engine opens a [`PushSource`] on the ring when it needs the mic and drains it on its own
//! thread. Frames pushed while no source is open are dropped, as a closed mic would hear
//! nothing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bolo_core::{AudioFactory, AudioSource};

/// The rate Kotlin records at; the engine's own rate, so nothing is resampled.
pub const SAMPLE_RATE: u32 = 16_000;
/// Samples kept when the engine falls behind (4 s, as the desktop capture ring).
const CAPACITY: usize = SAMPLE_RATE as usize * 4;

/// The buffer shared by the host's recording thread and the engine thread.
#[derive(Default)]
pub struct PushRing {
    samples: Mutex<VecDeque<f32>>,
    open: AtomicBool,
    /// The last frame's RMS as `f32` bits.
    level: AtomicU32,
    /// Samples thrown away because the engine didn't drain in time.
    dropped: AtomicU64,
}

impl PushRing {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Append one frame of mono PCM16 at [`SAMPLE_RATE`]. Ignored while no source is open.
    pub fn push(&self, frame: &[i16]) {
        if !self.open.load(Ordering::Acquire) || frame.is_empty() {
            return;
        }
        let mut sum = 0.0f32;
        let mut q = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        for &s in frame {
            let x = f32::from(s) / 32768.0;
            sum += x * x;
            q.push_back(x);
        }
        let over = q.len().saturating_sub(CAPACITY);
        if over > 0 {
            q.drain(..over);
            self.dropped.fetch_add(over as u64, Ordering::Relaxed);
        }
        drop(q);
        let rms = (sum / frame.len() as f32).sqrt();
        self.level.store(rms.to_bits(), Ordering::Relaxed);
    }

    /// [`Self::push`] for little-endian PCM16 bytes; a trailing odd byte is ignored.
    pub fn push_pcm16le(&self, bytes: &[u8]) {
        let frame: Vec<i16> = bytes.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b)).collect();
        self.push(&frame);
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// A factory the engine calls whenever it opens the "mic". The device name is ignored:
    /// the host decides where audio comes from.
    pub fn factory(self: &Arc<Self>) -> AudioFactory {
        let ring = self.clone();
        Arc::new(move |_device| Ok(Box::new(PushSource::open(ring.clone())) as Box<dyn AudioSource>))
    }

    fn reset(&self) {
        self.samples.lock().unwrap_or_else(PoisonError::into_inner).clear();
        self.level.store(0f32.to_bits(), Ordering::Relaxed);
    }
}

/// The engine's view of the ring while its mic is open.
pub struct PushSource {
    ring: Arc<PushRing>,
}

impl PushSource {
    fn open(ring: Arc<PushRing>) -> Self {
        ring.reset();
        ring.open.store(true, Ordering::Release);
        Self { ring }
    }
}

impl Drop for PushSource {
    fn drop(&mut self) {
        self.ring.open.store(false, Ordering::Release);
        self.ring.reset();
    }
}

impl AudioSource for PushSource {
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        let mut q = self.ring.samples.lock().unwrap_or_else(PoisonError::into_inner);
        out.extend(q.drain(..));
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.ring.level.load(Ordering::Relaxed))
    }

    fn device_name(&self) -> String {
        "Android microphone".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(ring: &Arc<PushRing>) -> Box<dyn AudioSource> {
        (ring.factory())(None).expect("opens")
    }

    #[test]
    fn frames_before_the_mic_opens_are_dropped() {
        let ring = PushRing::new();
        ring.push(&[1000; 320]);
        let mut src = open(&ring);
        let mut out = Vec::new();
        src.drain_mono(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn pushed_pcm16_comes_out_as_mono_f32_with_a_level() {
        let ring = PushRing::new();
        let mut src = open(&ring);
        ring.push(&[16384, -16384, 16384, -16384]);
        let mut out = Vec::new();
        src.drain_mono(&mut out);
        assert_eq!(out, vec![0.5, -0.5, 0.5, -0.5]);
        assert!((src.level() - 0.5).abs() < 1e-6);
        assert_eq!(src.sample_rate(), 16_000);
        // Drained: nothing comes out twice.
        out.clear();
        src.drain_mono(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn little_endian_bytes_decode_to_samples() {
        let ring = PushRing::new();
        let mut src = open(&ring);
        // 0x4000 = 16384 (0.5), 0xC000 = -16384 (-0.5), then a stray byte.
        ring.push_pcm16le(&[0x00, 0x40, 0x00, 0xC0, 0x7F]);
        let mut out = Vec::new();
        src.drain_mono(&mut out);
        assert_eq!(out, vec![0.5, -0.5]);
    }

    #[test]
    fn an_undrained_ring_keeps_the_newest_four_seconds_and_counts_the_rest() {
        let ring = PushRing::new();
        let mut src = open(&ring);
        let frames = CAPACITY / 320 + 10;
        for i in 0..frames {
            ring.push(&[i as i16; 320]);
        }
        let mut out = Vec::new();
        src.drain_mono(&mut out);
        assert_eq!(out.len(), CAPACITY);
        assert_eq!(ring.dropped(), 10 * 320);
        assert_eq!(out.last().copied(), Some(f32::from((frames - 1) as i16) / 32768.0));
    }

    #[test]
    fn closing_the_mic_stops_buffering() {
        let ring = PushRing::new();
        let src = open(&ring);
        ring.push(&[1; 320]);
        drop(src);
        ring.push(&[1; 320]);
        let mut src = open(&ring);
        let mut out = Vec::new();
        src.drain_mono(&mut out);
        assert!(out.is_empty(), "a reopened mic starts empty");
    }
}
