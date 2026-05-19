use std::sync::Arc;

use crate::recording_coordinator::{RecordingCoordinator, RecordingState};

use super::audio::AudioRecordingManager;
use super::transcription::TranscriptionManager;

/// Wraps `RecordingCoordinator` and drives the audio + transcription managers
/// so shortcut and IPC callers get end-to-end recording behaviour without
/// needing to know about the underlying managers.
#[derive(Clone)]
pub struct RecordingPipeline {
    coordinator: RecordingCoordinator,
    audio: Arc<AudioRecordingManager>,
    transcription: Arc<TranscriptionManager>,
}

impl RecordingPipeline {
    pub fn new(
        coordinator: RecordingCoordinator,
        audio: Arc<AudioRecordingManager>,
        transcription: Arc<TranscriptionManager>,
    ) -> Self {
        Self {
            coordinator,
            audio,
            transcription,
        }
    }

    pub fn coordinator(&self) -> &RecordingCoordinator {
        &self.coordinator
    }

    /// Toggle-mode primary shortcut: Idle→start recording, Recording→transcribe.
    pub fn toggle(&self) {
        match self.coordinator.state() {
            RecordingState::Idle => {
                if let Err(e) = self.audio.try_start_recording() {
                    self.coordinator.notify_error(e);
                    return;
                }
                self.transcription.initiate_model_load();
                self.coordinator.toggle();
            }
            RecordingState::Recording { .. } => {
                self.coordinator.toggle();
                self.spawn_transcription();
            }
            _ => {}
        }
    }

    /// Toggle-mode post-process shortcut.
    pub fn toggle_with_post_process(&self) {
        match self.coordinator.state() {
            RecordingState::Idle => {
                if let Err(e) = self.audio.try_start_recording() {
                    self.coordinator.notify_error(e);
                    return;
                }
                self.transcription.initiate_model_load();
                self.coordinator.toggle_with_post_process();
            }
            RecordingState::Recording { .. } => {
                self.coordinator.toggle_with_post_process();
                self.spawn_transcription();
            }
            _ => {}
        }
    }

    /// Push-to-talk key-down.
    pub fn start_ptt(&self, with_post_process: bool) {
        if self.coordinator.state() == RecordingState::Idle {
            if let Err(e) = self.audio.try_start_recording() {
                self.coordinator.notify_error(e);
                return;
            }
            self.transcription.initiate_model_load();
            self.coordinator.start_ptt(with_post_process);
        }
    }

    /// Push-to-talk key-up.
    pub fn stop_ptt(&self) {
        if let RecordingState::Recording { .. } = self.coordinator.state() {
            self.coordinator.stop_ptt();
            self.spawn_transcription();
        }
    }

    /// Cancel any in-progress recording or transcription.
    pub fn cancel(&self) {
        self.audio.cancel_recording();
        self.coordinator.cancel();
    }

    /// Stops audio recording and spawns an async task that transcribes the
    /// captured audio and notifies the coordinator when done.
    fn spawn_transcription(&self) {
        let audio = Arc::clone(&self.audio);
        let transcription = Arc::clone(&self.transcription);
        let coordinator = self.coordinator.clone();

        tokio::spawn(async move {
            let samples = tokio::task::spawn_blocking(move || audio.stop_recording())
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("stop_recording task panicked: {e}");
                    None
                });

            let samples = match samples {
                Some(s) if !s.is_empty() => s,
                _ => {
                    coordinator.notify_transcription_complete(String::new());
                    return;
                }
            };

            let result =
                tokio::task::spawn_blocking(move || transcription.transcribe(samples)).await;

            match result {
                Ok(Ok(text)) => coordinator.notify_transcription_complete(text),
                Ok(Err(e)) => coordinator.notify_error(e.to_string()),
                Err(e) => coordinator.notify_error(format!("transcription task panicked: {e}")),
            }
        });
    }
}
