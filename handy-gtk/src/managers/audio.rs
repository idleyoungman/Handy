use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app_context::AppContext;
use crate::audio_toolkit::{
    list_input_devices,
    vad::{ensure_vad_model, SmoothedVad},
    AudioRecorder, SileroVad,
};
use crate::backend_event::BackendEvent;

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const WHISPER_SAMPLE_RATE: usize = 16000;

fn create_audio_recorder(ctx: &AppContext) -> anyhow::Result<AudioRecorder> {
    let vad_path = ensure_vad_model()?;
    let silero = SileroVad::new(&vad_path, 0.3)
        .map_err(|e| anyhow::anyhow!("Failed to create SileroVad: {e}"))?;
    let smoothed_vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);

    let ctx_clone = ctx.clone();
    let recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {e}"))?
        .with_vad(Box::new(smoothed_vad))
        .with_level_callback(move |buckets| {
            let avg = buckets.iter().sum::<f32>() / buckets.len() as f32;
            ctx_clone.emit(BackendEvent::MicLevel(avg));
        });

    Ok(recorder)
}

#[derive(Clone, Debug)]
enum RecordingState {
    Idle,
    Recording,
}

#[derive(Clone)]
pub struct AudioRecordingManager {
    ctx: AppContext,
    state: Arc<Mutex<RecordingState>>,
    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    close_generation: Arc<AtomicU64>,
}

impl AudioRecordingManager {
    pub fn new(ctx: AppContext) -> anyhow::Result<Self> {
        let manager = Self {
            ctx: ctx.clone(),
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            recorder: Arc::new(Mutex::new(None)),
            is_open: Arc::new(Mutex::new(false)),
            is_recording: Arc::new(Mutex::new(false)),
            close_generation: Arc::new(AtomicU64::new(0)),
        };

        if ctx.settings().always_on_microphone {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    fn get_selected_device(&self) -> Option<cpal::Device> {
        let settings = self.ctx.settings();
        let name = settings.selected_microphone.as_ref()?;
        match list_input_devices() {
            Ok(devices) => devices
                .into_iter()
                .find(|d| &d.name == name)
                .map(|d| d.device),
            Err(e) => {
                tracing::debug!("Failed to list devices, using default: {e}");
                None
            }
        }
    }

    fn schedule_lazy_close(self: &Arc<Self>) {
        let gen = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mgr = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(STREAM_IDLE_TIMEOUT);
            let state = mgr.state.lock().unwrap();
            if mgr.close_generation.load(Ordering::SeqCst) == gen
                && matches!(*state, RecordingState::Idle)
            {
                tracing::info!(
                    "Closing idle microphone stream after {:?}",
                    STREAM_IDLE_TIMEOUT
                );
                mgr.stop_microphone_stream();
            }
        });
    }

    pub fn start_microphone_stream(&self) -> anyhow::Result<()> {
        let mut open_flag = self.is_open.lock().unwrap();
        if *open_flag {
            tracing::debug!("Microphone stream already active");
            return Ok(());
        }

        let start_time = Instant::now();
        let selected_device = self.get_selected_device();

        if selected_device.is_none() {
            let has_any = list_input_devices().map(|d| !d.is_empty()).unwrap_or(false);
            if !has_any {
                anyhow::bail!("No input device found");
            }
        }

        let mut recorder_opt = self.recorder.lock().unwrap();
        if recorder_opt.is_none() {
            *recorder_opt = Some(
                create_audio_recorder(&self.ctx)
                    .map_err(|e| anyhow::anyhow!("Failed to create recorder: {e}"))?,
            );
        }

        if let Some(rec) = recorder_opt.as_mut() {
            rec.open(selected_device)
                .map_err(|e| anyhow::anyhow!("Failed to open recorder: {e}"))?;
        }

        *open_flag = true;
        tracing::info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub fn stop_microphone_stream(&self) {
        let mut open_flag = self.is_open.lock().unwrap();
        if !*open_flag {
            return;
        }

        if let Some(rec) = self.recorder.lock().unwrap().as_mut() {
            if *self.is_recording.lock().unwrap() {
                let _ = rec.stop();
                *self.is_recording.lock().unwrap() = false;
            }
            let _ = rec.close();
        }

        *open_flag = false;
        tracing::debug!("Microphone stream stopped");
    }

    pub fn try_start_recording(self: &Arc<Self>) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, RecordingState::Idle) {
            let settings = self.ctx.settings();
            if !settings.always_on_microphone {
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                if let Err(e) = self.start_microphone_stream() {
                    return Err(format!("{e}"));
                }
            }

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                if rec.start().is_ok() {
                    *self.is_recording.lock().unwrap() = true;
                    *state = RecordingState::Recording;
                    tracing::debug!("Recording started");
                    return Ok(());
                }
            }
            Err("Recorder not available".to_string())
        } else {
            Err("Already recording".to_string())
        }
    }

    pub fn stop_recording(self: &Arc<Self>) -> Option<Vec<f32>> {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, RecordingState::Recording) {
            *state = RecordingState::Idle;
            drop(state);

            let settings = self.ctx.settings();
            if settings.extra_recording_buffer_ms > 0 {
                std::thread::sleep(Duration::from_millis(settings.extra_recording_buffer_ms));
            }

            let samples = if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                match rec.stop() {
                    Ok(buf) => buf,
                    Err(e) => {
                        tracing::error!("stop() failed: {e}");
                        Vec::new()
                    }
                }
            } else {
                tracing::error!("Recorder not available");
                Vec::new()
            };

            *self.is_recording.lock().unwrap() = false;

            let settings = self.ctx.settings();
            if !settings.always_on_microphone {
                if settings.lazy_stream_close {
                    self.schedule_lazy_close();
                } else {
                    self.stop_microphone_stream();
                }
            }

            let s_len = samples.len();
            if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                let mut padded = samples;
                padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                Some(padded)
            } else {
                Some(samples)
            }
        } else {
            None
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(*self.state.lock().unwrap(), RecordingState::Recording)
    }

    pub fn cancel_recording(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, RecordingState::Recording) {
            *state = RecordingState::Idle;
            drop(state);

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                let _ = rec.stop();
            }
            *self.is_recording.lock().unwrap() = false;

            let settings = self.ctx.settings();
            if !settings.always_on_microphone {
                if settings.lazy_stream_close {
                    // No Arc<Self> here, just close immediately on cancel
                    self.stop_microphone_stream();
                } else {
                    self.stop_microphone_stream();
                }
            }
        }
    }
}
