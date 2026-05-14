use std::sync::{Arc, Mutex};

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;

/// Current state of the recording pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    /// Audio is being captured; `with_post_process` determines which pipeline
    /// runs when recording stops.
    Recording {
        with_post_process: bool,
    },
    Transcribing,
    PostProcessing,
}

/// Serialises all recording-lifecycle transitions and drives the backend event
/// channel.
///
/// Both the shortcut thread and the IPC dispatch task share a single
/// coordinator via `Arc`-clone so state is consistent across callers.
#[derive(Clone)]
pub struct RecordingCoordinator {
    state: Arc<Mutex<RecordingState>>,
    ctx: AppContext,
}

impl RecordingCoordinator {
    pub fn new(ctx: AppContext) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            ctx,
        }
    }

    /// Returns a snapshot of the current recording state.
    pub fn state(&self) -> RecordingState {
        *self.state.lock().expect("coordinator lock poisoned")
    }

    /// Returns a reference to the underlying `AppContext` for callers that
    /// need to emit non-recording events (e.g. `FocusWindow`).
    pub fn ctx(&self) -> &AppContext {
        &self.ctx
    }

    /// Toggle-mode press of the primary shortcut (or `--toggle-transcription`).
    ///
    /// - Idle        → start recording (no post-process)
    /// - Recording   → stop recording, begin transcription
    /// - Any other   → ignored (pipeline already in flight)
    pub fn toggle(&self) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        match *guard {
            RecordingState::Idle => {
                *guard = RecordingState::Recording {
                    with_post_process: false,
                };
                drop(guard);
                self.ctx.emit(BackendEvent::ShowOverlay);
                self.ctx.emit(BackendEvent::RecordingStarted);
            }
            RecordingState::Recording { .. } => {
                *guard = RecordingState::Transcribing;
                drop(guard);
                self.ctx.emit(BackendEvent::RecordingStopped);
                self.ctx.emit(BackendEvent::TranscriptionStarted);
            }
            _ => {}
        }
    }

    /// Toggle-mode press of the post-process shortcut (or
    /// `--toggle-post-process`).
    ///
    /// - Idle        → start recording (with post-process)
    /// - Recording   → stop recording, begin post-processing
    /// - Any other   → ignored
    pub fn toggle_with_post_process(&self) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        match *guard {
            RecordingState::Idle => {
                *guard = RecordingState::Recording {
                    with_post_process: true,
                };
                drop(guard);
                self.ctx.emit(BackendEvent::ShowOverlay);
                self.ctx.emit(BackendEvent::RecordingStarted);
            }
            RecordingState::Recording { .. } => {
                *guard = RecordingState::PostProcessing;
                drop(guard);
                self.ctx.emit(BackendEvent::RecordingStopped);
                self.ctx.emit(BackendEvent::PostProcessingStarted);
            }
            _ => {}
        }
    }

    /// Push-to-talk key-down for the primary shortcut.
    ///
    /// Only starts recording when the pipeline is idle; silently ignored if
    /// already active (e.g. key repeat events).
    pub fn start_ptt(&self, with_post_process: bool) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        if *guard == RecordingState::Idle {
            *guard = RecordingState::Recording { with_post_process };
            drop(guard);
            self.ctx.emit(BackendEvent::ShowOverlay);
            self.ctx.emit(BackendEvent::RecordingStarted);
        }
    }

    /// Push-to-talk key-up.
    ///
    /// Stops an active recording and transitions to transcription or
    /// post-processing according to `with_post_process`.
    pub fn stop_ptt(&self) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        if let RecordingState::Recording { with_post_process } = *guard {
            if with_post_process {
                *guard = RecordingState::PostProcessing;
                drop(guard);
                self.ctx.emit(BackendEvent::RecordingStopped);
                self.ctx.emit(BackendEvent::PostProcessingStarted);
            } else {
                *guard = RecordingState::Transcribing;
                drop(guard);
                self.ctx.emit(BackendEvent::RecordingStopped);
                self.ctx.emit(BackendEvent::TranscriptionStarted);
            }
        }
    }

    /// Cancel key press or `--cancel` CLI flag.
    ///
    /// Aborts any in-flight operation and returns to idle.
    pub fn cancel(&self) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        if *guard != RecordingState::Idle {
            *guard = RecordingState::Idle;
            drop(guard);
            self.ctx.emit(BackendEvent::RecordingStopped);
            self.ctx.emit(BackendEvent::HideOverlay);
        }
    }

    /// Called by the transcription backend when it has produced a result.
    pub fn notify_transcription_complete(&self, text: String) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        *guard = RecordingState::Idle;
        drop(guard);
        self.ctx.emit(BackendEvent::TranscriptionCompleted { text });
    }

    /// Called by the post-processing backend when it has produced a result.
    pub fn notify_post_processing_complete(&self, text: String) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        *guard = RecordingState::Idle;
        drop(guard);
        self.ctx
            .emit(BackendEvent::PostProcessingCompleted { text });
    }

    /// Called by a backend manager when a recording or transcription error
    /// occurs.  Resets to idle and surfaces the error via `BackendEvent`.
    pub fn notify_error(&self, msg: String) {
        let mut guard = self.state.lock().expect("coordinator lock poisoned");
        *guard = RecordingState::Idle;
        drop(guard);
        self.ctx.emit(BackendEvent::RecordingError(msg));
        self.ctx.emit(BackendEvent::HideOverlay);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppSettings;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    fn make_coordinator(dir: &TempDir) -> (RecordingCoordinator, mpsc::Receiver<BackendEvent>) {
        let (event_tx, event_rx) = mpsc::channel(64);
        let config_path = dir.path().join("handy").join("config.toml");
        let ctx = AppContext::new(AppSettings::default(), event_tx, config_path);
        (RecordingCoordinator::new(ctx), event_rx)
    }

    async fn drain(rx: &mut mpsc::Receiver<BackendEvent>) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn toggle_idle_starts_recording() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        assert_eq!(
            coord.state(),
            RecordingState::Recording {
                with_post_process: false
            }
        );
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::ShowOverlay)));
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::RecordingStarted)));
    }

    #[tokio::test]
    async fn toggle_recording_stops_and_starts_transcription() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        drain(&mut rx).await;

        coord.toggle();
        assert_eq!(coord.state(), RecordingState::Transcribing);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::RecordingStopped)));
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::TranscriptionStarted)));
    }

    #[tokio::test]
    async fn toggle_ignored_while_transcribing() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        coord.toggle(); // now transcribing
        drain(&mut rx).await;

        coord.toggle(); // should be ignored
        assert_eq!(coord.state(), RecordingState::Transcribing);
        assert!(drain(&mut rx).await.is_empty());
    }

    #[tokio::test]
    async fn toggle_with_post_process_starts_then_posts() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle_with_post_process();
        assert_eq!(
            coord.state(),
            RecordingState::Recording {
                with_post_process: true
            }
        );
        drain(&mut rx).await;

        coord.toggle_with_post_process();
        assert_eq!(coord.state(), RecordingState::PostProcessing);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::RecordingStopped)));
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::PostProcessingStarted)));
    }

    #[tokio::test]
    async fn cancel_from_recording_resets_to_idle() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        drain(&mut rx).await;

        coord.cancel();
        assert_eq!(coord.state(), RecordingState::Idle);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::RecordingStopped)));
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::HideOverlay)));
    }

    #[tokio::test]
    async fn cancel_from_idle_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.cancel();
        assert_eq!(coord.state(), RecordingState::Idle);
        assert!(drain(&mut rx).await.is_empty());
    }

    #[tokio::test]
    async fn cancel_from_transcribing_resets_to_idle() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        coord.toggle(); // transcribing
        drain(&mut rx).await;

        coord.cancel();
        assert_eq!(coord.state(), RecordingState::Idle);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::HideOverlay)));
    }

    #[tokio::test]
    async fn ptt_start_and_stop_transitions_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.start_ptt(false);
        assert_eq!(
            coord.state(),
            RecordingState::Recording {
                with_post_process: false
            }
        );
        drain(&mut rx).await;

        coord.stop_ptt();
        assert_eq!(coord.state(), RecordingState::Transcribing);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::TranscriptionStarted)));
    }

    #[tokio::test]
    async fn ptt_start_ignored_when_already_recording() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.start_ptt(false);
        drain(&mut rx).await;
        coord.start_ptt(false); // key-repeat — must be ignored
        assert!(drain(&mut rx).await.is_empty());
    }

    #[tokio::test]
    async fn notify_transcription_complete_resets_state_and_emits_event() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        coord.toggle(); // transcribing
        drain(&mut rx).await;

        coord.notify_transcription_complete("hello world".into());
        assert_eq!(coord.state(), RecordingState::Idle);
        let events = drain(&mut rx).await;
        assert!(events.iter().any(|e| matches!(
            e,
            BackendEvent::TranscriptionCompleted { text } if text == "hello world"
        )));
    }

    #[tokio::test]
    async fn notify_error_resets_to_idle_and_hides_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let (coord, mut rx) = make_coordinator(&dir);
        coord.toggle();
        drain(&mut rx).await;

        coord.notify_error("mic unavailable".into());
        assert_eq!(coord.state(), RecordingState::Idle);
        let events = drain(&mut rx).await;
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::RecordingError(_))));
        assert!(events
            .iter()
            .any(|e| matches!(e, BackendEvent::HideOverlay)));
    }
}
