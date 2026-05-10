use clap::Parser;

mod app_context;
mod autostart;
mod backend_event;
mod cli;
mod config;
mod ipc;
mod shortcut;

use app_context::AppContext;
use backend_event::BackendEvent;
use cli::CliArgs;
use ipc::IpcAction;

#[tokio::main]
async fn main() {
    // ── Parse CLI ─────────────────────────────────────────────────────────────
    let args = CliArgs::parse();

    // ── Single-instance check ─────────────────────────────────────────────────
    if ipc::is_primary_running().await {
        if let Err(e) = ipc::dispatch_to_primary(
            args.toggle_transcription,
            args.toggle_post_process,
            args.cancel,
        )
        .await
        {
            eprintln!("handy-gtk: failed to contact running instance: {e}");
            std::process::exit(1);
        }
        return;
    }

    // ── Load settings ─────────────────────────────────────────────────────────
    let settings = match config::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("handy-gtk: failed to load config: {e} — using defaults");
            config::AppSettings::default()
        }
    };

    // ── Build AppContext ──────────────────────────────────────────────────────
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<BackendEvent>(64);
    let config_path = config::config_path().expect("XDG config dir must be available");
    let ctx = AppContext::new(settings.clone(), event_tx, config_path);

    // ── Register D-Bus IPC service ────────────────────────────────────────────
    let (_conn, mut ipc_rx) = ipc::register_service()
        .await
        .expect("failed to register D-Bus IPC service");

    // ── Start global shortcut listener ────────────────────────────────────────
    let shortcut_manager = shortcut::ShortcutManager::start(ctx.clone(), &settings);
    if let Err(ref e) = shortcut_manager {
        eprintln!("handy-gtk: shortcut manager failed to start: {e}");
        eprintln!("handy-gtk: global shortcuts will not be available");
    }

    // ── Main event loop ───────────────────────────────────────────────────────
    // TODO: replace with GTK/Relm4 main loop.  For now this loop keeps the
    // process alive and logs events so the steel thread can be validated.
    loop {
        tokio::select! {
            Some(action) = ipc_rx.recv() => {
                handle_ipc_action(&ctx, action);
            }
            Some(event) = event_rx.recv() => {
                handle_backend_event(event);
            }
        }
    }
}

fn handle_ipc_action(ctx: &AppContext, action: IpcAction) {
    match action {
        IpcAction::FocusWindow => {
            tracing::info!("ipc: FocusWindow");
            ctx.emit(BackendEvent::FocusWindow);
        }
        IpcAction::ToggleTranscription => {
            tracing::info!("ipc: ToggleTranscription");
            ctx.emit(BackendEvent::RecordingStarted);
        }
        IpcAction::TogglePostProcess => {
            tracing::info!("ipc: TogglePostProcess");
            ctx.emit(BackendEvent::PostProcessingStarted);
        }
        IpcAction::Cancel => {
            tracing::info!("ipc: Cancel");
            ctx.emit(BackendEvent::RecordingStopped);
        }
    }
}

fn handle_backend_event(event: BackendEvent) {
    // TODO: route to GTK overlay and settings window.
    match event {
        BackendEvent::RecordingStarted => tracing::info!("Recording started"),
        BackendEvent::RecordingStopped => tracing::info!("Recording stopped"),
        BackendEvent::TranscriptionStarted => tracing::info!("Transcription started"),
        BackendEvent::TranscriptionCompleted { text } => {
            tracing::info!("Transcription: {text}")
        }
        BackendEvent::PostProcessingStarted => tracing::info!("Post-processing started"),
        BackendEvent::PostProcessingCompleted { text } => {
            tracing::info!("Post-processing: {text}")
        }
        BackendEvent::ShowOverlay => tracing::debug!("Show overlay"),
        BackendEvent::HideOverlay => tracing::debug!("Hide overlay"),
        BackendEvent::MicLevel(level) => tracing::trace!("Mic level: {level:.2}"),
        BackendEvent::FocusWindow => tracing::info!("Focus window"),
        BackendEvent::PasteError(e) => tracing::error!("Paste error: {e}"),
        BackendEvent::RecordingError(e) => tracing::error!("Recording error: {e}"),
        BackendEvent::ModelStateChanged { model_id, loaded } => {
            tracing::info!("Model {model_id} loaded={loaded}")
        }
        BackendEvent::ModelDownloadProgress {
            model_id,
            progress,
            speed_bps,
            eta_secs,
        } => tracing::debug!(
            "Model {model_id} download: {:.0}% @ {speed_bps}B/s ETA {eta_secs}s",
            progress * 100.0
        ),
        BackendEvent::ModelDownloadCompleted { model_id } => {
            tracing::info!("Model {model_id} download complete")
        }
        BackendEvent::ModelDownloadFailed { model_id, error } => {
            tracing::error!("Model {model_id} download failed: {error}")
        }
        BackendEvent::ModelDeleted { model_id } => tracing::info!("Model {model_id} deleted"),
        BackendEvent::HistoryUpdated => tracing::debug!("History updated"),
    }
}
