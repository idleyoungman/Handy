use anyhow::Result;
use std::path::{Path, PathBuf};

use vad_rs::Vad;

use super::{VadFrame, VoiceActivityDetector};
use crate::audio_toolkit::constants;

const SILERO_FRAME_MS: u32 = 30;
const SILERO_FRAME_SAMPLES: usize =
    (constants::WHISPER_SAMPLE_RATE * SILERO_FRAME_MS / 1000) as usize;

static VAD_MODEL_BYTES: &[u8] = include_bytes!("../../../resources/silero_vad_v4.onnx");

/// Extracts the embedded Silero VAD model to the user data directory on first
/// use and returns the path. Subsequent calls return early if the file exists.
pub fn ensure_vad_model() -> Result<PathBuf> {
    let vad_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("XDG data dir unavailable"))?
        .join("handy")
        .join("vad");

    std::fs::create_dir_all(&vad_dir)?;
    let path = vad_dir.join("silero_vad_v4.onnx");

    if !path.exists() {
        std::fs::write(&path, VAD_MODEL_BYTES)?;
        tracing::info!("Extracted VAD model to {}", path.display());
    }

    Ok(path)
}

pub struct SileroVad {
    engine: Vad,
    threshold: f32,
}

impl SileroVad {
    pub fn new<P: AsRef<Path>>(model_path: P, threshold: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&threshold) {
            anyhow::bail!("threshold must be between 0.0 and 1.0");
        }

        Ok(Self {
            engine: Vad::new(&model_path, constants::WHISPER_SAMPLE_RATE as usize)
                .map_err(|e| anyhow::anyhow!("Failed to create VAD: {e}"))?,
            threshold,
        })
    }
}

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != SILERO_FRAME_SAMPLES {
            anyhow::bail!(
                "expected {SILERO_FRAME_SAMPLES} samples, got {}",
                frame.len()
            );
        }

        let result = self
            .engine
            .compute(frame)
            .map_err(|e| anyhow::anyhow!("Silero VAD error: {e}"))?;

        if result.prob > self.threshold {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }
}
