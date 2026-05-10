use ksni::{menu::StandardItem, MenuItem, Tray, TrayMethods};

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;

pub struct HandyTray {
    ctx: AppContext,
}

impl HandyTray {
    fn new(ctx: AppContext) -> Self {
        Self { ctx }
    }
}

impl Tray for HandyTray {
    fn id(&self) -> String {
        "handy-gtk".into()
    }

    fn icon_name(&self) -> String {
        "audio-input-microphone".into()
    }

    fn title(&self) -> String {
        "Handy".into()
    }

    /// Left-click brings the settings window to the front.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.ctx.emit(BackendEvent::FocusWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            MenuItem::Standard(StandardItem {
                label: "Show Settings".into(),
                activate: Box::new(|this: &mut HandyTray| {
                    this.ctx.emit(BackendEvent::FocusWindow);
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "Toggle Recording".into(),
                activate: Box::new(|this: &mut HandyTray| {
                    this.ctx.emit(BackendEvent::RecordingStarted);
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }),
        ]
    }
}

/// Spawns the system tray icon as a background tokio task.
///
/// The returned handle must be kept alive for the duration of the process.
/// Dropping it removes the tray icon.  Returns an error if the desktop does
/// not support the StatusNotifierItem specification.
pub async fn spawn(ctx: AppContext) -> Result<ksni::Handle<HandyTray>, ksni::Error> {
    HandyTray::new(ctx).assume_sni_available(true).spawn().await
}
