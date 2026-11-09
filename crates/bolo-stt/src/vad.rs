//! Silero VAD (sherpa-onnx) for cutting dictation into segments at pauses, so batch
//! engines can transcribe finished segments while the user is still speaking.

use std::path::Path;

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::{Result, SttError};

#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// Speech probability threshold.
    pub threshold: f32,
    /// A pause at least this long ends a segment (seconds).
    pub min_silence: f32,
    /// Ignore blips shorter than this (seconds).
    pub min_speech: f32,
    /// Force a cut in long monologues (seconds); keeps cloud requests under limits.
    pub max_speech: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        // 0.5 s pause ≈ a sentence boundary in dictation; 25 s cap stays under Sarvam's 30 s.
        Self { threshold: 0.5, min_silence: 0.5, min_speech: 0.25, max_speech: 25.0 }
    }
}

pub struct Vad {
    inner: VoiceActivityDetector,
}

impl Vad {
    pub fn new(model: &Path, cfg: VadConfig) -> Result<Self> {
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model.to_string_lossy().into_owned()),
                threshold: cfg.threshold,
                min_silence_duration: cfg.min_silence,
                min_speech_duration: cfg.min_speech,
                window_size: 512,
                max_speech_duration: cfg.max_speech,
            },
            sample_rate: bolo_audio::SAMPLE_RATE as i32,
            num_threads: 1,
            ..Default::default()
        };
        let inner = VoiceActivityDetector::create(&config, 60.0)
            .ok_or_else(|| SttError::Model(format!("could not load VAD model {}", model.display())))?;
        Ok(Self { inner })
    }

    /// Feed 16 kHz mono audio (any length).
    pub fn accept(&mut self, samples: &[f32]) {
        self.inner.accept_waveform(samples);
    }

    /// True while the detector currently hears speech.
    pub fn speaking(&self) -> bool {
        self.inner.detected()
    }

    /// Completed speech segments since the last call.
    pub fn pop_segments(&mut self) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        while !self.inner.is_empty() {
            if let Some(seg) = self.inner.front() {
                out.push(seg.samples().to_vec());
            }
            self.inner.pop();
        }
        out
    }

    /// End of input: close any open segment and return the remaining segments.
    pub fn flush(&mut self) -> Vec<Vec<f32>> {
        self.inner.flush();
        self.pop_segments()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
