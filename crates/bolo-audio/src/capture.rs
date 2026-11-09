//! Live microphone capture (cpal: WASAPI on Windows, AAudio on Android).
//!
//! The audio callback only converts samples to `f32` and pushes them into a lock-free
//! SPSC ring buffer, plus an RMS level in an atomic. No allocation and no locks happen
//! on the audio thread. The consumer side down-mixes to mono.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapRb};

use crate::{AudioError, Result};

/// Ring buffer capacity in seconds of device audio. The consumer should drain at least
/// every few hundred ms; overflowing samples are dropped and counted.
const RING_SECONDS: usize = 4;

pub struct Capture {
    stream: cpal::Stream,
    cons: HeapCons<f32>,
    channels: usize,
    sample_rate: u32,
    device_name: String,
    level: Arc<AtomicU32>,
    dropped: Arc<AtomicU64>,
    scratch: Vec<f32>,
}

impl Capture {
    /// Open the default input device and start capturing immediately.
    pub fn start_default() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| AudioError::Device("no input device".into()))?;
        let device_name = device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| "unknown".into());
        let config = device.default_input_config().map_err(|e| AudioError::Device(e.to_string()))?;
        let channels = usize::from(config.channels().max(1));
        let sample_rate = config.sample_rate();

        let rb = HeapRb::<f32>::new(sample_rate as usize * channels * RING_SECONDS);
        let (prod, cons) = rb.split();
        let level = Arc::new(AtomicU32::new(0));
        let dropped = Arc::new(AtomicU64::new(0));

        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config.into(), prod, level.clone(), dropped.clone()),
            SampleFormat::I16 => build::<i16>(&device, config.into(), prod, level.clone(), dropped.clone()),
            SampleFormat::I32 => build::<i32>(&device, config.into(), prod, level.clone(), dropped.clone()),
            SampleFormat::U16 => build::<u16>(&device, config.into(), prod, level.clone(), dropped.clone()),
            other => return Err(AudioError::Unsupported(format!("sample format {other}"))),
        }?;
        stream.play().map_err(|e| AudioError::Device(e.to_string()))?;
        tracing::info!(device = %device_name, sample_rate, channels, "capture started");
        Ok(Self { stream, cons, channels, sample_rate, device_name, level, dropped, scratch: Vec::new() })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Device sample rate (resample with [`crate::resample`] to reach 16 kHz).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// RMS of the most recent callback buffer, 0..1.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Samples dropped because the consumer fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Move everything captured so far into `out` as mono at the device rate.
    pub fn drain_mono(&mut self, out: &mut Vec<f32>) {
        let ch = self.channels;
        self.scratch.resize(4096 * ch, 0.0);
        loop {
            let n = self.cons.pop_slice(&mut self.scratch);
            if n == 0 {
                break;
            }
            if ch == 1 {
                out.extend_from_slice(&self.scratch[..n]);
            } else {
                out.extend(self.scratch[..n - n % ch].chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
            }
        }
    }

    /// Stop capturing and return the rest of the audio as mono at the device rate.
    pub fn stop(mut self, out: &mut Vec<f32>) {
        let _ = self.stream.pause();
        self.drain_mono(out);
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut prod: ringbuf::HeapProd<f32>,
    level: Arc<AtomicU32>,
    dropped: Arc<AtomicU64>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let on_data = move |data: &[T], _: &cpal::InputCallbackInfo| {
        let mut sum = 0.0f32;
        let pushed = prod.push_iter(data.iter().map(|&s| {
            let v: f32 = s.to_sample();
            sum += v * v;
            v
        }));
        if pushed < data.len() {
            dropped.fetch_add((data.len() - pushed) as u64, Ordering::Relaxed);
        }
        if !data.is_empty() {
            level.store((sum / data.len() as f32).sqrt().to_bits(), Ordering::Relaxed);
        }
    };
    let on_err = |e: cpal::Error| tracing::warn!(error = %e, "capture stream error");
    device.build_input_stream(config, on_data, on_err, None).map_err(|e| AudioError::Device(e.to_string()))
}
