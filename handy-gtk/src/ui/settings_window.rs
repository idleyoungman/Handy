use relm4::adw::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

use crate::app_context::AppContext;
use crate::managers::history::HistoryUpdatePayload;

use super::pages::general::GeneralPage;
use super::pages::history::{HistoryInput, HistoryPage};
use super::pages::output::OutputPage;

pub struct SettingsWindow {
    general_page: Controller<GeneralPage>,
    history_page: Controller<HistoryPage>,
    output_page: Controller<OutputPage>,
    toast_overlay: adw::ToastOverlay,
}

#[derive(Debug)]
pub enum SettingsWindowInput {
    HistoryUpdated(HistoryUpdatePayload),
    PasteError(String),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsWindow {
    type Init = (AppContext, Arc<crate::managers::history::HistoryManager>);
    type Input = SettingsWindowInput;
    type Output = ();

    view! {
        adw::Window {
            set_title: Some("Handy Settings"),
            set_default_width: 700,
            set_default_height: 600,
            set_hide_on_close: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    #[wrap(Some)]
                    set_title_widget = &adw::WindowTitle {
                        set_title: "Handy Settings",
                    },
                },

                set_content: Some(local_toast_overlay),
            }
        }
    }

    fn init(
        (ctx, history_manager): Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let output_page = OutputPage::builder().launch(ctx.clone()).detach();
        let general_page = GeneralPage::builder().launch(ctx).detach();
        let history_page = HistoryPage::builder().launch(history_manager).detach();

        let notebook = gtk::Notebook::new();
        notebook.append_page(
            general_page.widget(),
            Some(&gtk::Label::new(Some("General"))),
        );
        notebook.append_page(output_page.widget(), Some(&gtk::Label::new(Some("Output"))));
        notebook.append_page(
            history_page.widget(),
            Some(&gtk::Label::new(Some("History"))),
        );

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&notebook));
        let local_toast_overlay = &toast_overlay.clone();
        let model = SettingsWindow {
            general_page,
            history_page,
            output_page,
            toast_overlay,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: SettingsWindowInput, _sender: ComponentSender<Self>) {
        match msg {
            SettingsWindowInput::HistoryUpdated(payload) => {
                self.history_page.emit(HistoryInput::Update(payload));
            }
            SettingsWindowInput::PasteError(msg) => {
                let toast = adw::Toast::builder()
                    .title(format!("Paste failed: {msg}"))
                    .timeout(5)
                    .build();
                self.toast_overlay.add_toast(toast);
            }
        }
    }
}
