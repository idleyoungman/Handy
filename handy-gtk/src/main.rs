use clap::Parser;

mod cli;
mod ipc;

use cli::CliArgs;
use ipc::IpcAction;

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();

    if ipc::is_primary_running().await {
        // A primary instance is already running — dispatch and exit.
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

    // We are the primary instance.
    let (_conn, mut rx) = ipc::register_service()
        .await
        .expect("failed to register D-Bus service");

    // TODO: initialise backend managers and GTK UI here.
    // For now, loop processing IPC actions so the binary stays alive
    // and second-instance detection works end-to-end.
    while let Some(action) = rx.recv().await {
        match action {
            IpcAction::FocusWindow => {
                // TODO: bring settings window to front
                eprintln!("ipc: FocusWindow (UI not yet implemented)");
            }
            IpcAction::ToggleTranscription => {
                eprintln!("ipc: ToggleTranscription (not yet implemented)");
            }
            IpcAction::TogglePostProcess => {
                eprintln!("ipc: TogglePostProcess (not yet implemented)");
            }
            IpcAction::Cancel => {
                eprintln!("ipc: Cancel (not yet implemented)");
            }
        }
    }
}
