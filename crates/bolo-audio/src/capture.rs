//! Live microphone capture (cpal: WASAPI on Windows, AAudio on Android).
//!
//! The audio callback only converts samples to `f32` and pushes them into a lock-free
//! SPSC ring buffer, plus an RMS level in an atomic. No allocation and no locks happen
//! on the audio thread. The consumer side down-mixes to mono.
//!
//! [`Capture::start`] opens the input device the user chose, by name, and falls back to
//! the default when it isn't plugged in. The device list is read again on every open, so
//! an unplugged mic falls back and a re-plugged one is used again. A stream whose device
//! goes away mid-session reports [`Capture::failed`]; its owner opens a new one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapRb};

use crate::device::{Pick, pick};
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
    failed: Arc<AtomicBool>,
    fell_back: bool,
    scratch: Vec<f32>,
}

/// The input devices plugged in now, by name, in the system's order.
pub fn input_devices() -> Result<Vec<String>> {
    Ok(named_inputs(&cpal::default_host())?.into_iter().map(|(name, _)| name).collect())
}

/// The default input device's name, if there is one.
pub fn default_input_device() -> Option<String> {
    cpal::default_host().default_input_device().map(|d| name_of(&d))
}

/// Whether the device named `wanted` is plugged in now.
pub fn is_available(wanted: &str) -> bool {
    input_devices().is_ok_and(|names| matches!(pick(Some(wanted), &names), Pick::Device(_)))
}

fn named_inputs(host: &cpal::Host) -> Result<Vec<(String, cpal::Device)>> {
    let devices = host.input_devices().map_err(|e| AudioError::Device(e.to_string()))?;
    Ok(devices.map(|d| (name_of(&d), d)).collect())
}

fn name_of(device: &cpal::Device) -> String {
    device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| "unknown".into())
}

impl Capture {
    /// Open the default input device and start capturing immediately.
    pub fn start_default() -> Result<Self> {
        Self::start(None)
    }

    /// Open the input device named `wanted` (`None`: the default) and start capturing
    /// immediately. When it isn't plugged in, or won't open, the default is opened instead
    /// and [`Self::fell_back`] says so.
    pub fn start(wanted: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let chosen = match wanted.map(str::trim).filter(|w| !w.is_empty()) {
            None => None,
            Some(wanted) => {
                let mut devices = named_inputs(&host)?;
                let names: Vec<&str> = devices.iter().map(|(n, _)| n.as_str()).collect();
                match pick(Some(wanted), &names) {
                    Pick::Device(i) => Some((wanted, devices.swap_remove(i).1)),
                    Pick::Default => None,
                    Pick::Missing => {
                        tracing::info!(wanted, "the chosen microphone isn't plugged in; using the default");
                        return Self::open_default(&host).map(Self::fallen_back);
                    }
                }
            }
        };
        match chosen {
            Some((wanted, device)) => Self::open(device).or_else(|e| {
                tracing::warn!(wanted, error = %e, "the chosen microphone won't open; using the default");
                Self::open_default(&host).map(Self::fallen_back)
            }),
            None => Self::open_default(&host),
        }
    }

    fn fallen_back(self) -> Self {
        Self { fell_back: true, ..self }
    }

    fn open_default(host: &cpal::Host) -> Result<Self> {
        Self::open(host.default_input_device().ok_or_else(|| AudioError::Device("no input device".into()))?)
    }

    fn open(device: cpal::Device) -> Result<Self> {
        let device_name = name_of(&device);
        let config = device.default_input_config().map_err(|e| AudioError::Device(e.to_string()))?;
        let channels = usize::from(config.channels().max(1));
        let sample_rate = config.sample_rate();

        let rb = HeapRb::<f32>::new(sample_rate as usize * channels * RING_SECONDS);
        let (prod, cons) = rb.split();
        let level = Arc::new(AtomicU32::new(0));
        let dropped = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let shared = Shared { level: level.clone(), dropped: dropped.clone(), failed: failed.clone() };

        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config.into(), prod, shared),
            SampleFormat::I16 => build::<i16>(&device, config.into(), prod, shared),
            SampleFormat::I32 => build::<i32>(&device, config.into(), prod, shared),
            SampleFormat::U16 => build::<u16>(&device, config.into(), prod, shared),
            other => return Err(AudioError::Unsupported(format!("sample format {other}"))),
        }?;
        stream.play().map_err(|e| AudioError::Device(e.to_string()))?;
        tracing::info!(device = %device_name, sample_rate, channels, "capture started");
        Ok(Self {
            stream,
            cons,
            channels,
            sample_rate,
            device_name,
            level,
            dropped,
            failed,
            fell_back: false,
            scratch: Vec::new(),
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The default device was opened because the chosen one isn't plugged in (or
    /// wouldn't open).
    pub fn fell_back(&self) -> bool {
        self.fell_back
    }

    /// The stream stopped for good, typically because its device was unplugged. It
    /// delivers nothing more; open a new one.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
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

/// What the audio callbacks share with [`Capture`].
struct Shared {
    level: Arc<AtomicU32>,
    dropped: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
}

/// Whether a stream error means the stream is gone, rather than a glitch it survives.
fn fatal(kind: cpal::ErrorKind) -> bool {
    !matches!(kind, cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied | cpal::ErrorKind::DeviceChanged)
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut prod: ringbuf::HeapProd<f32>,
    shared: Shared,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let Shared { level, dropped, failed } = shared;
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
    let on_err = move |e: cpal::Error| {
        if fatal(e.kind()) {
            tracing::warn!(error = %e, "capture stream failed; a new one opens for the next dictation");
            failed.store(true, Ordering::Relaxed);
        } else {
            tracing::debug!(error = %e, "capture stream glitch");
        }
    };
    device.build_input_stream(config, on_data, on_err, None).map_err(|e| AudioError::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_lasting_errors_fail_the_stream() {
        use cpal::ErrorKind::*;
        assert!(super::fatal(DeviceNotAvailable));
        assert!(super::fatal(StreamInvalidated));
        assert!(super::fatal(BackendError));
        assert!(!super::fatal(Xrun));
        assert!(!super::fatal(DeviceChanged), "rerouted, still running");
    }

    /// The real input devices. Run by hand: `cargo test -p bolo-audio --features capture
    /// list_devices -- --ignored --nocapture`. Lists names only; records nothing.
    #[test]
    #[ignore = "reads the system's audio devices"]
    fn list_devices() {
        println!("default: {:?}", super::default_input_device());
        for name in super::input_devices().unwrap() {
            println!("input: {name} (available: {})", super::is_available(&name));
        }
    }
}
