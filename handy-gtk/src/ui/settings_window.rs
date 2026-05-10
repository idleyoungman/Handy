use relm4::adw::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

use crate::managers::history::HistoryUpdatePayload;

use super::pages::history::{HistoryInput, HistoryPage};

pub struct SettingsWindow {
    history_page: Controller<HistoryPage>,
}

#[derive(Debug)]
pub enum SettingsWindowInput {
    HistoryUpdated(HistoryUpdatePayload),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsWindow {
    type Init = Arc<crate::managers::history::HistoryManager>;
    type Input = SettingsWindowInput;
    type Output = ();

    view! {
        adw::Window {
            set_title: Some("Handy Settings"),
            set_default_width: 680,
            set_default_height: 580,
            set_hide_on_close: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    #[wrap(Some)]
                    set_title_widget = &adw::WindowTitle {
                        set_title: "Handy Settings",
                        set_subtitle: "History",
                    },
                },

                #[wrap(Some)]
                set_content = model.history_page.widget(),
            }
        }
    }

    fn init(
        history_manager: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let history_page = HistoryPage::builder().launch(history_manager).detach();

        let model = SettingsWindow { history_page };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: SettingsWindowInput, _sender: ComponentSender<Self>) {
        match msg {
            SettingsWindowInput::HistoryUpdated(payload) => {
                self.history_page.emit(HistoryInput::Update(payload));
            }
        }
    }
}
