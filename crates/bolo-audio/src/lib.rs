//! Audio plumbing for Bolo. The whole pipeline runs on 16 kHz mono `f32` in `[-1, 1]`.

#[cfg(feature = "capture")]
pub mod capture;
pub mod codec;
pub mod resample;

/// Pipeline sample rate. Every STT engine receives audio at this rate.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported audio: {0}")]
    Unsupported(String),
    #[error("audio device error: {0}")]
    Device(String),
    #[error("resampler error: {0}")]
    Resample(String),
}

pub type Result<T, E = AudioError> = std::result::Result<T, E>;

/// Root-mean-square level of a frame, for the waveform UI.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Duration in seconds of a 16 kHz mono buffer.
pub fn duration_secs(samples: &[f32]) -> f64 {
    samples.len() as f64 / f64::from(SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_constant() {
        assert!((rms(&[0.5; 100]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }
}
