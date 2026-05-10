// TODO: remove once all scaffolding modules are wired up to the UI
#![allow(dead_code)]

use clap::Parser;

mod app_context;
mod autostart;
mod backend_event;
mod cli;
mod config;
mod ipc;
mod shortcut;
mod tray;
mod ui;

use app_context::AppContext;
use backend_event::BackendEvent;
use cli::CliArgs;
use ipc::IpcAction;

fn main() {
    let args = CliArgs::parse();

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

    // ── Register D-Bus IPC service ────────────────────────────────────────────
    let (_conn, ipc_rx) = rt
        .block_on(ipc::register_service())
        .expect("failed to register D-Bus IPC service");

    // Route IPC actions on the background runtime.
    {
        let ctx = ctx.clone();
        rt.spawn(ipc_dispatch_loop(ipc_rx, ctx));
    }

    // ── Start global shortcut listener ────────────────────────────────────────
    let _shortcut = match shortcut::ShortcutManager::start(ctx.clone(), &settings) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("handy-gtk: shortcut manager failed to start: {e}");
            eprintln!("handy-gtk: global shortcuts will not be available");
            None
        }
    };

    // ── Start system tray icon ────────────────────────────────────────────────
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
    // RelmApp::new initialises GTK and libadwaita; run() blocks until the app exits.
    // Use a distinct app ID to avoid GTK claiming our IPC D-Bus name.
    let app = relm4::RelmApp::new("computer.handy.Handy.Gtk");
    app.run::<ui::app::App>((ctx, event_rx, settings));
}

async fn ipc_dispatch_loop(mut ipc_rx: tokio::sync::mpsc::Receiver<IpcAction>, ctx: AppContext) {
    while let Some(action) = ipc_rx.recv().await {
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
}
