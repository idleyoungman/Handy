/// Events emitted by backend managers and delivered to the UI layer.
///
/// This enum is the sole mechanism for backend-to-UI communication, replacing
/// all Tauri event emissions. The UI root component routes each variant to the
/// appropriate child (overlay, settings window, toast overlay).
#[derive(Debug, Clone)]
pub enum BackendEvent {
    // ── Overlay / recording state ─────────────────────────────────────────────
    ShowOverlay,
    HideOverlay,
    RecordingStarted,
    RecordingStopped,
    TranscriptionStarted,
    TranscriptionCompleted { text: String },
    PostProcessingStarted,
    PostProcessingCompleted { text: String },

    // ── Microphone level (0.0 – 1.0) — high frequency ────────────────────────
    MicLevel(f32),

    // ── Model lifecycle ───────────────────────────────────────────────────────
    ModelStateChanged {
        model_id: String,
        loaded: bool,
    },
    ModelDownloadProgress {
        model_id: String,
        /// 0.0 – 1.0
        progress: f32,
        speed_bps: u64,
        eta_secs: u64,
    },
    ModelDownloadCompleted {
        model_id: String,
    },
    ModelDownloadFailed {
        model_id: String,
        error: String,
    },
    ModelDeleted {
        model_id: String,
    },

    // ── History ───────────────────────────────────────────────────────────────
    HistoryUpdated,

    // ── Errors ────────────────────────────────────────────────────────────────
    PasteError(String),
    RecordingError(String),

    // ── Window management ─────────────────────────────────────────────────────
    FocusWindow,
}
