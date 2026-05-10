use relm4::adw::prelude::*;
use relm4::prelude::*;

pub struct SettingsWindow;

#[relm4::component(pub)]
impl SimpleComponent for SettingsWindow {
    type Init = ();
    type Input = ();
    type Output = ();

    view! {
        adw::Window {
            set_title: Some("Handy Settings"),
            set_default_width: 640,
            set_default_height: 560,
            set_hide_on_close: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {},

                #[wrap(Some)]
                set_content = &gtk::Label {
                    set_label: "Settings",
                    set_vexpand: true,
                }
            }
        }
    }

    fn init(
        _: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SettingsWindow;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>) {}
}
