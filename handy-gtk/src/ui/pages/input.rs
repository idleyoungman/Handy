use cpal::traits::{DeviceTrait, HostTrait};
use relm4::adw::prelude::*;
use relm4::prelude::*;

use crate::app_context::AppContext;

/// Extracts a human-readable card name from an ALSA device string.
/// `sysdefault:CARD=C930e` → `Some("C930e")`
/// `front:CARD=PCH,DEV=0` → `Some("PCH")`
/// `pulse` → `None`
fn alsa_card_name(device_name: &str) -> Option<&str> {
    let card_start = device_name.find("CARD=")? + 5;
    let rest = &device_name[card_start..];
    let end = rest.find([',', ':']).unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Enumerates available audio input device names using cpal.
/// Returns a list starting with "System default" followed by deduplicated physical devices.
fn enumerate_input_devices() -> Vec<String> {
    let mut names = vec!["System default".to_string()];
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => {
            for device in devices {
                match device.name() {
                    Ok(raw) => {
                        if let Some(card) = alsa_card_name(&raw) {
                            let label = card.to_string();
                            if !names.contains(&label) {
                                names.push(label);
                            }
                        }
                    }
                    Err(e) => tracing::warn!("Could not get input device name: {e}"),
                }
            }
        }
        Err(e) => tracing::warn!("Could not enumerate input devices: {e}"),
    }
    names
}

/// Maps a persisted device name to an index in `device_names`.
/// Returns 0 (System default) if not found.
fn device_index(device_names: &[String], selected: &Option<String>) -> u32 {
    match selected {
        None => 0,
        Some(name) => device_names.iter().position(|n| n == name).unwrap_or(0) as u32,
    }
}

pub struct InputPage {
    ctx: AppContext,
    device_names: Vec<String>,
    selected_index: u32,
}

#[derive(Debug)]
pub enum InputInput {
    MicrophoneChanged(u32),
}

#[relm4::component(pub)]
impl SimpleComponent for InputPage {
    type Init = AppContext;
    type Input = InputInput;
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
                        set_title: "Microphone",

                        adw::ComboRow {
                            set_title: "Input device",
                            set_subtitle: "Microphone used for recording",
                            set_model: Some(&{
                                let list = gtk::StringList::new(&[]);
                                for name in &model.device_names {
                                    list.append(name);
                                }
                                list
                            }),
                            #[watch]
                            set_selected: model.selected_index,
                            connect_selected_notify[sender] => move |row| {
                                sender.input(InputInput::MicrophoneChanged(row.selected()));
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
        let device_names = enumerate_input_devices();
        let selected_index = device_index(&device_names, &settings.selected_microphone);
        let model = InputPage {
            ctx,
            device_names,
            selected_index,
        };
        let widgets = view_output!();
        let _ = &sender;
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: InputInput, _sender: ComponentSender<Self>) {
        match msg {
            InputInput::MicrophoneChanged(idx) => {
                self.selected_index = idx;
                let selection = if idx == 0 {
                    None
                } else {
                    self.device_names.get(idx as usize).cloned()
                };
                self.ctx
                    .update_settings(|s| s.selected_microphone = selection);
            }
        }
    }
}
