//! In-memory WAV encoding (for cloud uploads) and WAV file decoding (bench, tests).

use crate::SAMPLE_RATE;

/// Encode 16 kHz mono `f32` samples as a 16-bit PCM WAV file in memory.
///
/// Hand-rolled (44-byte header) so cloud builds don't need `hound`.
pub fn encode_wav_pcm16(samples: &[f32]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX - 36);
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples.iter().take(data_len as usize / 2) {
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Read any PCM/float WAV file and return 16 kHz mono `f32`.
/// Multi-channel input is down-mixed; other sample rates are resampled.
#[cfg(feature = "wav-file")]
pub fn read_wav_16k_mono(path: &std::path::Path) -> crate::Result<Vec<f32>> {
    use crate::AudioError;

    let mut reader = hound::WavReader::open(path).map_err(|e| AudioError::Unsupported(e.to_string()))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().collect::<Result<_, _>>().map_err(|e| AudioError::Unsupported(e.to_string()))?
        }
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| AudioError::Unsupported(e.to_string()))?
        }
    };
    let mono: Vec<f32> = if channels == 1 {
        interleaved
    } else {
        interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
    };
    crate::resample::to_16k(&mono, spec.sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let wav = encode_wav_pcm16(&[0.0, 1.0, -1.0, 0.5]);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 16_000);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), i16::MAX);
    }

    #[cfg(feature = "wav-file")]
    #[test]
    fn roundtrip_through_file() {
        let samples: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let dir = std::env::temp_dir().join(format!("bolo-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        std::fs::write(&p, encode_wav_pcm16(&samples)).unwrap();
        let back = read_wav_16k_mono(&p).unwrap();
        assert_eq!(back.len(), samples.len());
        assert!(back.iter().zip(&samples).all(|(a, b)| (a - b).abs() < 1e-3));
        std::fs::remove_dir_all(&dir).ok();
    }
}
