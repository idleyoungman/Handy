use relm4::adw::prelude::*;
use relm4::prelude::*;

use crate::app_context::AppContext;

pub struct GeneralPage {
    ctx: AppContext,
    autostart_enabled: bool,
    start_hidden: bool,
}

#[derive(Debug)]
pub enum GeneralInput {
    SetAutostart(bool),
    SetStartHidden(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for GeneralPage {
    type Init = AppContext;
    type Input = GeneralInput;
    type Output = ();

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hscrollbar_policy: gtk::PolicyType::Never,

            adw::Clamp {
                set_maximum_size: 600,
                set_margin_top: 24,
                set_margin_bottom: 24,
                set_margin_start: 12,
                set_margin_end: 12,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,

                    adw::PreferencesGroup {
                        set_title: "Startup",

                        adw::SwitchRow {
                            set_title: "Start automatically on login",
                            set_subtitle: "Launch Handy when you log in",
                            #[watch]
                            set_active: model.autostart_enabled,
                            connect_active_notify[sender] => move |row| {
                                sender.input(GeneralInput::SetAutostart(row.is_active()));
                            },
                        },

                        adw::SwitchRow {
                            set_title: "Start hidden",
                            set_subtitle: "Open to the tray only — no settings window at launch",
                            #[watch]
                            set_active: model.start_hidden,
                            connect_active_notify[sender] => move |row| {
                                sender.input(GeneralInput::SetStartHidden(row.is_active()));
                            },
                        },
                    },
                }
            }
        }
    }

    fn init(
        ctx: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let settings = ctx.settings();
        let model = GeneralPage {
            autostart_enabled: settings.autostart_enabled,
            start_hidden: settings.start_hidden,
            ctx,
        };
        let widgets = view_output!();
        // Suppress unused-sender warning from the borrow in connect_active_notify closures.
        let _ = &sender;
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: GeneralInput, _sender: ComponentSender<Self>) {
        match msg {
            GeneralInput::SetAutostart(enabled) => {
                self.autostart_enabled = enabled;
                self.ctx.update_settings(|s| s.autostart_enabled = enabled);
                let result = if enabled {
                    std::env::current_exe()
                        .map_err(|e| e.to_string())
                        .and_then(|exe| crate::autostart::enable(&exe))
                } else {
                    crate::autostart::disable()
                };
                if let Err(e) = result {
                    tracing::warn!("Autostart toggle failed: {e}");
                }
            }
            GeneralInput::SetStartHidden(hidden) => {
                self.start_hidden = hidden;
                self.ctx.update_settings(|s| s.start_hidden = hidden);
            }
        }
    }
}
