use relm4::adw::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::app_context::AppContext;
use crate::audio_feedback::{self, SoundType};
use crate::backend_event::BackendEvent;
use crate::config::AppSettings;
use crate::managers::history::HistoryManager;
use crate::paste;

use super::overlay::{Overlay, OverlayInput, OverlayStatus};
use super::settings_window::{SettingsWindow, SettingsWindowInput};

pub struct App {
    ctx: AppContext,
    overlay: Controller<Overlay>,
    settings_window: Controller<SettingsWindow>,
}

#[derive(Debug)]
pub enum AppInput {
    BackendEvent(BackendEvent),
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = (
        AppContext,
        mpsc::Receiver<BackendEvent>,
        AppSettings,
        Arc<HistoryManager>,
        bool, // start_hidden
    );
    type Input = AppInput;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_default_width: 1,
            set_default_height: 1,
            set_decorated: false,
            set_visible: false,
        }
    }

    fn init(
        (ctx, mut event_rx, settings, history_manager, start_hidden): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let overlay = Overlay::builder()
            .launch(settings.overlay_position)
            .detach();

        let settings_window = SettingsWindow::builder()
            .launch((ctx.clone(), history_manager))
            .detach();

        if !start_hidden {
            settings_window.widget().present();
        }

        let sender_clone = sender.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                sender_clone.input(AppInput::BackendEvent(event));
            }
        });

        let model = App {
            ctx,
            overlay,
            settings_window,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AppInput, _sender: ComponentSender<Self>) {
        match msg {
            AppInput::BackendEvent(event) => self.route_event(event),
        }
    }
}

impl App {
    fn route_event(&self, event: BackendEvent) {
        match event {
            BackendEvent::ShowOverlay => {
                self.overlay.emit(OverlayInput::Show);
            }
            BackendEvent::HideOverlay => {
                self.overlay.emit(OverlayInput::Hide);
            }
            BackendEvent::RecordingStarted => {
                audio_feedback::play_feedback_sound(&self.ctx, SoundType::Start);
                self.overlay
                    .emit(OverlayInput::SetStatus(OverlayStatus::Recording));
            }
            BackendEvent::RecordingStopped => {
                audio_feedback::play_feedback_sound(&self.ctx, SoundType::Stop);
                self.overlay.emit(OverlayInput::Hide);
            }
            BackendEvent::TranscriptionCompleted { text }
            | BackendEvent::PostProcessingCompleted { text } => {
                audio_feedback::play_feedback_sound(&self.ctx, SoundType::Stop);
                self.overlay.emit(OverlayInput::Hide);
                let ctx = self.ctx.clone();
                tokio::spawn(async move { paste::execute(&ctx, text).await });
            }
            BackendEvent::PasteError(msg) => {
                tracing::warn!("Paste error: {msg}");
                self.settings_window.widget().present();
                self.settings_window
                    .emit(SettingsWindowInput::PasteError(msg));
            }
            BackendEvent::TranscriptionStarted => {
                self.overlay
                    .emit(OverlayInput::SetStatus(OverlayStatus::Transcribing));
            }
            BackendEvent::PostProcessingStarted => {
                self.overlay
                    .emit(OverlayInput::SetStatus(OverlayStatus::Processing));
            }
            BackendEvent::MicLevel(level) => {
                self.overlay.emit(OverlayInput::MicLevel(level));
            }
            BackendEvent::FocusWindow => {
                self.settings_window.widget().present();
            }
            BackendEvent::HistoryUpdated(payload) => {
                self.settings_window
                    .emit(SettingsWindowInput::HistoryUpdated(payload));
            }
            _ => {}
        }
    }
}
