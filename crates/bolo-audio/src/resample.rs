//! Offline resampling of whole clips to 16 kHz (rubato FFT resampler).
//! Live capture uses a streaming resampler (added with `capture` in M1 phase 2).

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::{AudioError, Result, SAMPLE_RATE};

/// Resample a mono clip from `rate_in` Hz to [`SAMPLE_RATE`].
pub fn to_16k(mono: &[f32], rate_in: u32) -> Result<Vec<f32>> {
    if rate_in == SAMPLE_RATE || mono.is_empty() {
        return Ok(mono.to_vec());
    }
    if rate_in == 0 {
        return Err(AudioError::Unsupported("sample rate 0".into()));
    }
    let mut rs = Fft::<f32>::new(rate_in as usize, SAMPLE_RATE as usize, 1024, 1, FixedSync::Input)
        .map_err(|e| AudioError::Resample(e.to_string()))?;
    let input = InterleavedSlice::new(mono, 1, mono.len()).map_err(|e| AudioError::Resample(e.to_string()))?;
    let out = rs.process_all(&input, mono.len(), None).map_err(|e| AudioError::Resample(e.to_string()))?;
    Ok(out.take_data())
}

/// Stateful resampler for live capture: push device-rate chunks of any size, get 16 kHz
/// out with no seams between chunks (the FFT resampler keeps its overlap state).
pub struct StreamResampler {
    rs: Option<Fft<f32>>,
    pending: Vec<f32>,
}

impl StreamResampler {
    pub fn new(rate_in: u32) -> Result<Self> {
        if rate_in == 0 {
            return Err(AudioError::Unsupported("sample rate 0".into()));
        }
        let rs = if rate_in == SAMPLE_RATE {
            None
        } else {
            Some(
                Fft::<f32>::new(rate_in as usize, SAMPLE_RATE as usize, 1024, 1, FixedSync::Input)
                    .map_err(|e| AudioError::Resample(e.to_string()))?,
            )
        };
        Ok(Self { rs, pending: Vec::new() })
    }

    /// Feed mono samples at the input rate; appends 16 kHz samples to `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<()> {
        let Some(rs) = self.rs.as_mut() else {
            out.extend_from_slice(input);
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        loop {
            let need = rs.input_frames_next();
            if self.pending.len() < need {
                return Ok(());
            }
            let chunk = InterleavedSlice::new(&self.pending[..need], 1, need)
                .map_err(|e| AudioError::Resample(e.to_string()))?;
            let o = rs.process(&chunk, None).map_err(|e| AudioError::Resample(e.to_string()))?;
            out.extend(o.take_data());
            self.pending.drain(..need);
        }
    }

    /// Process whatever is buffered (zero-padded to a full chunk).
    pub fn flush(&mut self, out: &mut Vec<f32>) -> Result<()> {
        let Some(need) = self.rs.as_ref().map(|r| r.input_frames_next()) else { return Ok(()) };
        if self.pending.is_empty() {
            return Ok(());
        }
        let missing = need.saturating_sub(self.pending.len());
        self.push(&vec![0.0; missing], out)
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn streaming_matches_rate() {
        let mut r = StreamResampler::new(48_000).unwrap();
        let mut out = Vec::new();
        // 1 s of 48 kHz audio in uneven chunks
        let x: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin()).collect();
        for c in x.chunks(733) {
            r.push(c, &mut out).unwrap();
        }
        r.flush(&mut out).unwrap();
        assert!((out.len() as i64 - 16_000).abs() < 1100, "len {}", out.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_48k_length() {
        let x: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin()).collect();
        let y = to_16k(&x, 48_000).unwrap();
        assert!((y.len() as i64 - 16_000).abs() <= 2, "len {}", y.len());
    }

    #[test]
    fn passthrough_16k() {
        assert_eq!(to_16k(&[0.1, 0.2], 16_000).unwrap(), vec![0.1, 0.2]);
    }
}
