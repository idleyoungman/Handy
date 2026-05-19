// TODO: remove once all scaffolding modules are wired up to the UI
#![allow(dead_code)]

use clap::Parser;

mod app_context;
mod audio_feedback;
mod audio_toolkit;
mod autostart;
mod backend_event;
mod cli;
mod config;
mod ipc;
mod managers;
mod paste;
mod recording_coordinator;
mod shortcut;
mod tray;
mod ui;

use app_context::AppContext;
use backend_event::BackendEvent;
use cli::CliArgs;
use ipc::IpcAction;
use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::pipeline::RecordingPipeline;
use managers::transcription::TranscriptionManager;
use recording_coordinator::RecordingCoordinator;
use std::sync::Arc;

fn main() {
    let args = CliArgs::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── Background Tokio runtime ──────────────────────────────────────────────
    // GTK must own the main thread; Tokio runs on background threads.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Enter the runtime so that tokio::spawn works from non-async code (e.g.
    // inside AppContext::new → spawn_debounced_saver).
    let _guard = rt.enter();

    // ── Single-instance check ─────────────────────────────────────────────────
    if rt.block_on(ipc::is_primary_running()) {
        if let Err(e) = rt.block_on(ipc::dispatch_to_primary(
            args.toggle_transcription,
            args.toggle_post_process,
            args.cancel,
        )) {
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
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<BackendEvent>(64);
    let config_path = config::config_path().expect("XDG config dir must be available");
    let ctx = AppContext::new(settings.clone(), event_tx, config_path);

    // ── Initialize history manager ────────────────────────────────────────────
    let history_manager = match HistoryManager::new(ctx.clone()) {
        Ok(m) => {
            tracing::info!("History manager initialized");
            Arc::new(m)
        }
        Err(e) => {
            eprintln!("handy-gtk: failed to initialize history manager: {e}");
            std::process::exit(1);
        }
    };

    // ── Initialize model manager ──────────────────────────────────────────────
    let model_manager = match ModelManager::new(ctx.clone()) {
        Ok(m) => {
            tracing::info!("Model manager initialized");
            m
        }
        Err(e) => {
            eprintln!("handy-gtk: failed to initialize model manager: {e}");
            std::process::exit(1);
        }
    };

    // ── Initialize audio recording manager ───────────────────────────────────
    let audio_manager = match AudioRecordingManager::new(ctx.clone()) {
        Ok(m) => {
            tracing::info!("Audio recording manager initialized");
            Arc::new(m)
        }
        Err(e) => {
            tracing::warn!("Audio recording manager failed to initialize: {e}");
            tracing::warn!("Recording will not be available");
            // Continue without audio rather than crashing — user may not have a mic
            Arc::new(
                AudioRecordingManager::new(ctx.clone())
                    .unwrap_or_else(|_| panic!("failed to create fallback audio manager")),
            )
        }
    };

    // ── Initialize transcription manager ─────────────────────────────────────
    let transcription_manager = match TranscriptionManager::new(ctx.clone(), model_manager.clone())
    {
        Ok(m) => {
            tracing::info!("Transcription manager initialized");
            Arc::new(m)
        }
        Err(e) => {
            eprintln!("handy-gtk: failed to initialize transcription manager: {e}");
            std::process::exit(1);
        }
    };

    // ── Build RecordingCoordinator + Pipeline ─────────────────────────────────
    let coordinator = RecordingCoordinator::new(ctx.clone());
    let pipeline = RecordingPipeline::new(
        coordinator.clone(),
        Arc::clone(&audio_manager),
        Arc::clone(&transcription_manager),
    );

    // ── Register D-Bus IPC service ────────────────────────────────────────────
    let (_conn, ipc_rx) = rt
        .block_on(ipc::register_service())
        .expect("failed to register D-Bus IPC service");

    // ── Start global shortcut listener ────────────────────────────────────────
    let _shortcut = match shortcut::ShortcutManager::start(pipeline.clone(), &settings) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("handy-gtk: shortcut manager failed to start: {e}");
            eprintln!("handy-gtk: global shortcuts will not be available");
            None
        }
    };

    // Route IPC actions on the background runtime.
    {
        let p = pipeline.clone();
        let mm = model_manager.clone();
        let tm = transcription_manager.clone();
        rt.spawn(ipc_dispatch_loop(ipc_rx, p, mm, tm));
    }

    // ── Start system tray icon ───────────────────────────────────────────────────────
    let _tray = match rt.block_on(tray::spawn(ctx.clone())) {
        Ok(h) => {
            tracing::info!("System tray icon registered");
            Some(h)
        }
        Err(e) => {
            tracing::warn!("System tray unavailable: {e}");
            None
        }
    };

    // ── Run Relm4 / GTK main loop ─────────────────────────────────────────────
    let start_hidden = args.start_hidden || settings.start_hidden;

    let app = relm4::RelmApp::new("computer.handy.Handy.Gtk");
    app.run::<ui::app::App>((
        ctx,
        event_rx,
        settings,
        history_manager,
        model_manager,
        start_hidden,
    ));
}

async fn ipc_dispatch_loop(
    mut ipc_rx: tokio::sync::mpsc::Receiver<IpcAction>,
    pipeline: RecordingPipeline,
    model_manager: Arc<ModelManager>,
    transcription_manager: Arc<TranscriptionManager>,
) {
    while let Some(action) = ipc_rx.recv().await {
        match action {
            IpcAction::FocusWindow => {
                tracing::info!("ipc: FocusWindow");
                pipeline.coordinator().ctx().emit(BackendEvent::FocusWindow);
            }
            IpcAction::ToggleTranscription => {
                tracing::info!("ipc: ToggleTranscription");
                pipeline.toggle();
            }
            IpcAction::TogglePostProcess => {
                tracing::info!("ipc: TogglePostProcess");
                pipeline.toggle_with_post_process();
            }
            IpcAction::Cancel => {
                tracing::info!("ipc: Cancel");
                pipeline.cancel();
            }
            IpcAction::UnloadModel => {
                tracing::info!("ipc: UnloadModel");
                if let Err(e) = transcription_manager.unload_model() {
                    tracing::warn!("Failed to unload model via IPC (transcription): {e}");
                }
                if let Err(e) = model_manager.unload_model() {
                    tracing::warn!("Failed to unload model via IPC (model manager): {e}");
                }
            }
        }
    }
}
