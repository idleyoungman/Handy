use relm4::adw::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

use crate::managers::history::{HistoryEntry, HistoryManager, HistoryUpdatePayload};

pub struct HistoryPage {
    manager: Arc<HistoryManager>,
    entries: Vec<HistoryEntry>,
    has_more: bool,
    list_box: gtk::ListBox,
}

#[derive(Debug)]
pub enum HistoryInput {
    Reload,
    LoadMore,
    Update(HistoryUpdatePayload),
}

#[relm4::component(pub)]
impl SimpleComponent for HistoryPage {
    type Init = Arc<HistoryManager>;
    type Input = HistoryInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_margin_top: 12,
                    set_margin_bottom: 12,
                    set_margin_start: 12,
                    set_margin_end: 12,
                    set_spacing: 6,

                    adw::Clamp {
                        set_maximum_size: 800,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 6,

                            #[local_ref]
                            list_box -> gtk::ListBox {
                                set_selection_mode: gtk::SelectionMode::None,
                                add_css_class: "boxed-list",
                            },

                            gtk::Button {
                                set_label: "Load more",
                                set_visible: model.has_more,
                                connect_clicked[sender] => move |_| {
                                    sender.input(HistoryInput::LoadMore);
                                },
                            },
                        }
                    }
                }
            }
        }
    }

    fn init(
        manager: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::ListBox::new();
        let model = HistoryPage {
            manager,
            entries: Vec::new(),
            has_more: false,
            list_box: list_box.clone(),
        };
        let widgets = view_output!();

        sender.input(HistoryInput::Reload);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: HistoryInput, _sender: ComponentSender<Self>) {
        match msg {
            HistoryInput::Reload => {
                self.entries.clear();
                while let Some(row) = self.list_box.first_child() {
                    self.list_box.remove(&row);
                }
                match self.manager.get_history_entries(None, Some(50)) {
                    Ok(page) => {
                        self.has_more = page.has_more;
                        for entry in page.entries {
                            self.append_row(&entry);
                            self.entries.push(entry);
                        }
                        if self.entries.is_empty() {
                            self.append_empty_state();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load history: {e}");
                    }
                }
            }

            HistoryInput::LoadMore => {
                let cursor = self.entries.last().map(|e| e.id);
                match self.manager.get_history_entries(cursor, Some(50)) {
                    Ok(page) => {
                        self.has_more = page.has_more;
                        for entry in page.entries {
                            self.append_row(&entry);
                            self.entries.push(entry);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load more history: {e}");
                    }
                }
            }

            HistoryInput::Update(payload) => {
                match payload {
                    HistoryUpdatePayload::Added { entry } => {
                        // Reload to show new entry at top — factory insert would be complex.
                        // For now prepend: remove empty-state row if present, prepend new row.
                        self.prepend_row(&entry);
                        self.entries.insert(0, entry);
                    }
                    HistoryUpdatePayload::Updated { entry } => {
                        if let Some(pos) = self.entries.iter().position(|e| e.id == entry.id) {
                            self.entries[pos] = entry;
                        }
                    }
                    HistoryUpdatePayload::Deleted { id } => {
                        self.entries.retain(|e| e.id != id);
                    }
                    HistoryUpdatePayload::Toggled { id } => {
                        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                            e.saved = !e.saved;
                        }
                    }
                }
            }
        }
    }
}

impl HistoryPage {
    fn build_row(entry: &HistoryEntry) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
        vbox.set_margin_top(8);
        vbox.set_margin_bottom(8);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);

        let title = gtk::Label::new(Some(&entry.title));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("heading");

        let display_text = entry
            .post_processed_text
            .as_deref()
            .unwrap_or(&entry.transcription_text);
        let preview: String = display_text.chars().take(120).collect();
        let preview = if display_text.len() > 120 {
            format!("{preview}…")
        } else {
            preview
        };

        let body = gtk::Label::new(Some(&preview));
        body.set_halign(gtk::Align::Start);
        body.set_wrap(true);
        body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        body.add_css_class("body");

        vbox.append(&title);
        vbox.append(&body);
        row.set_child(Some(&vbox));
        row
    }

    fn append_row(&self, entry: &HistoryEntry) {
        self.list_box.append(&Self::build_row(entry));
    }

    fn prepend_row(&self, entry: &HistoryEntry) {
        self.list_box.prepend(&Self::build_row(entry));
    }

    fn append_empty_state(&self) {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some("No transcription history yet"));
        label.set_margin_top(24);
        label.set_margin_bottom(24);
        label.add_css_class("dim-label");
        row.set_child(Some(&label));
        self.list_box.append(&row);
    }
}
