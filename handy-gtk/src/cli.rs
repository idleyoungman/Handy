use clap::Parser;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "handy-gtk", about = "Handy - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden (tray only)
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle recording on/off on the running instance
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle recording with post-processing on/off on the running instance
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation on the running instance
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,
}

impl CliArgs {
    /// Returns true if any flag that targets a running instance was passed.
    pub fn is_remote_control(&self) -> bool {
        self.toggle_transcription || self.toggle_post_process || self.cancel
    }
}
