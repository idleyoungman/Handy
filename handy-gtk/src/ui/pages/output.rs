use relm4::adw::prelude::*;
use relm4::prelude::*;

use crate::app_context::AppContext;
use crate::config::{AutoSubmitKey, ClipboardHandling, PasteMethod, TypingTool};

const DELAY_PRESETS_MS: [u64; 6] = [0, 50, 100, 200, 500, 1000];
const DELAY_LABELS: [&str; 6] = ["None", "50 ms", "100 ms", "200 ms", "500 ms", "1 s"];

pub struct OutputPage {
    ctx: AppContext,
    paste_method: PasteMethod,
    clipboard_handling: ClipboardHandling,
    typing_tool: TypingTool,
    external_script_path: String,
    append_trailing_space: bool,
    auto_submit: bool,
    auto_submit_key: AutoSubmitKey,
    paste_delay_ms: u64,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum OutputInput {
    PasteMethodChanged(u32),
    ClipboardHandlingChanged(u32),
    TypingToolChanged(u32),
    ExternalScriptPathChanged(String),
    AppendTrailingSpaceChanged(bool),
    AutoSubmitChanged(bool),
    AutoSubmitKeyChanged(u32),
    PasteDelayChanged(u32),
}

fn paste_method_index(m: PasteMethod) -> u32 {
    match m {
        PasteMethod::CtrlV => 0,
        PasteMethod::ShiftInsert => 1,
        PasteMethod::Typing => 2,
        PasteMethod::Script => 3,
    }
}

fn paste_method_from_index(idx: u32) -> PasteMethod {
    match idx {
        1 => PasteMethod::ShiftInsert,
        2 => PasteMethod::Typing,
        3 => PasteMethod::Script,
        _ => PasteMethod::CtrlV,
    }
}

fn clipboard_handling_index(h: ClipboardHandling) -> u32 {
    match h {
        ClipboardHandling::Restore => 0,
        ClipboardHandling::Keep => 1,
        ClipboardHandling::Clear => 2,
    }
}

fn clipboard_handling_from_index(idx: u32) -> ClipboardHandling {
    match idx {
        1 => ClipboardHandling::Keep,
        2 => ClipboardHandling::Clear,
        _ => ClipboardHandling::Restore,
    }
}

fn typing_tool_index(t: TypingTool) -> u32 {
    match t {
        TypingTool::Auto => 0,
        TypingTool::Wtype => 1,
        TypingTool::Ydotool => 2,
    }
}

fn typing_tool_from_index(idx: u32) -> TypingTool {
    match idx {
        1 => TypingTool::Wtype,
        2 => TypingTool::Ydotool,
        _ => TypingTool::Auto,
    }
}

fn auto_submit_key_index(k: AutoSubmitKey) -> u32 {
    match k {
        AutoSubmitKey::None => 0,
        AutoSubmitKey::Enter => 1,
        AutoSubmitKey::Space => 2,
        AutoSubmitKey::Tab => 3,
    }
}

fn auto_submit_key_from_index(idx: u32) -> AutoSubmitKey {
    match idx {
        1 => AutoSubmitKey::Enter,
        2 => AutoSubmitKey::Space,
        3 => AutoSubmitKey::Tab,
        _ => AutoSubmitKey::None,
    }
}

fn delay_preset_index(ms: u64) -> u32 {
    DELAY_PRESETS_MS.iter().position(|&p| p == ms).unwrap_or(3) as u32
}

#[relm4::component(pub)]
impl SimpleComponent for OutputPage {
    type Init = AppContext;
    type Input = OutputInput;
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
                        set_title: "Paste method",

                        adw::ComboRow {
                            set_title: "Method",
                            set_model: Some(&gtk::StringList::new(&[
                                "Ctrl+V",
                                "Shift+Insert",
                                "Direct typing",
                                "External script",
                            ])),
                            #[watch]
                            set_selected: paste_method_index(model.paste_method),
                            connect_selected_notify[sender] => move |row| {
                                sender.input(OutputInput::PasteMethodChanged(row.selected()));
                            },
                        },

                        adw::ComboRow {
                            set_title: "Clipboard handling",
                            set_subtitle: "What to do with the clipboard after pasting",
                            set_model: Some(&gtk::StringList::new(&[
                                "Restore previous contents",
                                "Keep transcription in clipboard",
                                "Clear clipboard",
                            ])),
                            #[watch]
                            set_visible: model.paste_method == PasteMethod::CtrlV
                                || model.paste_method == PasteMethod::ShiftInsert,
                            #[watch]
                            set_selected: clipboard_handling_index(model.clipboard_handling),
                            connect_selected_notify[sender] => move |row| {
                                sender.input(OutputInput::ClipboardHandlingChanged(row.selected()));
                            },
                        },

                        adw::ComboRow {
                            set_title: "Typing tool",
                            set_subtitle: "Wayland tool used to inject key events",
                            set_model: Some(&gtk::StringList::new(&[
                                "Auto-detect",
                                "wtype",
                                "ydotool",
                            ])),
                            #[watch]
                            set_visible: model.paste_method == PasteMethod::Typing,
                            #[watch]
                            set_selected: typing_tool_index(model.typing_tool),
                            connect_selected_notify[sender] => move |row| {
                                sender.input(OutputInput::TypingToolChanged(row.selected()));
                            },
                        },

                        adw::EntryRow {
                            set_title: "Script path",
                            set_text: &model.external_script_path,
                            #[watch]
                            set_visible: model.paste_method == PasteMethod::Script,
                            connect_changed[sender] => move |row| {
                                sender.input(OutputInput::ExternalScriptPathChanged(
                                    row.text().to_string(),
                                ));
                            },
                        },
                    },

                    adw::PreferencesGroup {
                        set_title: "Delivery",

                        adw::ComboRow {
                            set_title: "Delay before paste",
                            set_model: Some(&gtk::StringList::new(&DELAY_LABELS)),
                            #[watch]
                            set_selected: delay_preset_index(model.paste_delay_ms),
                            connect_selected_notify[sender] => move |row| {
                                sender.input(OutputInput::PasteDelayChanged(row.selected()));
                            },
                        },

                        adw::SwitchRow {
                            set_title: "Append trailing space",
                            set_subtitle: "Add a space after pasted text",
                            #[watch]
                            set_active: model.append_trailing_space,
                            connect_active_notify[sender] => move |row| {
                                sender.input(OutputInput::AppendTrailingSpaceChanged(row.is_active()));
                            },
                        },

                        adw::SwitchRow {
                            set_title: "Auto-submit",
                            set_subtitle: "Send a key press after pasting",
                            #[watch]
                            set_active: model.auto_submit,
                            connect_active_notify[sender] => move |row| {
                                sender.input(OutputInput::AutoSubmitChanged(row.is_active()));
                            },
                        },

                        adw::ComboRow {
                            set_title: "Auto-submit key",
                            set_model: Some(&gtk::StringList::new(&[
                                "None",
                                "Enter",
                                "Space",
                                "Tab",
                            ])),
                            #[watch]
                            set_visible: model.auto_submit,
                            #[watch]
                            set_selected: auto_submit_key_index(model.auto_submit_key),
                            connect_selected_notify[sender] => move |row| {
                                sender.input(OutputInput::AutoSubmitKeyChanged(row.selected()));
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
        let model = OutputPage {
            paste_method: settings.paste_method,
            clipboard_handling: settings.clipboard_handling,
            typing_tool: settings.typing_tool,
            external_script_path: settings.external_script_path.clone().unwrap_or_default(),
            append_trailing_space: settings.append_trailing_space,
            auto_submit: settings.auto_submit,
            auto_submit_key: settings.auto_submit_key,
            paste_delay_ms: settings.paste_delay_ms,
            ctx,
        };
        let widgets = view_output!();
        let _ = &sender;
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: OutputInput, _sender: ComponentSender<Self>) {
        match msg {
            OutputInput::PasteMethodChanged(idx) => {
                let method = paste_method_from_index(idx);
                self.paste_method = method;
                self.ctx.update_settings(|s| s.paste_method = method);
            }
            OutputInput::ClipboardHandlingChanged(idx) => {
                let handling = clipboard_handling_from_index(idx);
                self.clipboard_handling = handling;
                self.ctx
                    .update_settings(|s| s.clipboard_handling = handling);
            }
            OutputInput::TypingToolChanged(idx) => {
                let tool = typing_tool_from_index(idx);
                self.typing_tool = tool;
                self.ctx.update_settings(|s| s.typing_tool = tool);
            }
            OutputInput::ExternalScriptPathChanged(path) => {
                let script_path = if path.is_empty() {
                    None
                } else {
                    Some(path.clone())
                };
                self.external_script_path = path;
                self.ctx
                    .update_settings(|s| s.external_script_path = script_path);
            }
            OutputInput::AppendTrailingSpaceChanged(enabled) => {
                self.append_trailing_space = enabled;
                self.ctx
                    .update_settings(|s| s.append_trailing_space = enabled);
            }
            OutputInput::AutoSubmitChanged(enabled) => {
                self.auto_submit = enabled;
                self.ctx.update_settings(|s| s.auto_submit = enabled);
            }
            OutputInput::AutoSubmitKeyChanged(idx) => {
                let key = auto_submit_key_from_index(idx);
                self.auto_submit_key = key;
                self.ctx.update_settings(|s| s.auto_submit_key = key);
            }
            OutputInput::PasteDelayChanged(idx) => {
                let ms = DELAY_PRESETS_MS.get(idx as usize).copied().unwrap_or(200);
                self.paste_delay_ms = ms;
                self.ctx.update_settings(|s| s.paste_delay_ms = ms);
            }
        }
    }
}
