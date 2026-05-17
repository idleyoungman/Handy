use relm4::adw::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

use crate::app_context::AppContext;
use crate::managers::model::{ModelInfo, ModelManager};

pub struct ModelsPage {
    ctx: AppContext,
    model_manager: Arc<ModelManager>,
    models: Vec<ModelInfo>,
    list_box: gtk::ListBox,
    loaded_model_id: String,
}

#[derive(Debug)]
pub enum ModelsInput {
    Download(String),
    CancelDownload(String),
    Delete(String),
    Select(String),
    Unload,
    ModelStateChanged {
        model_id: String,
        loaded: bool,
    },
    DownloadProgress {
        model_id: String,
        progress: f32,
        speed_bps: u64,
        eta_secs: u64,
    },
    DownloadCompleted(String),
    DownloadFailed {
        model_id: String,
        error: String,
    },
    ModelDeleted(String),
}

#[relm4::component(pub)]
impl SimpleComponent for ModelsPage {
    type Init = (AppContext, Arc<ModelManager>);
    type Input = ModelsInput;
    type Output = ();

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hscrollbar_policy: gtk::PolicyType::Never,

            adw::Clamp {
                set_maximum_size: 700,
                set_margin_top: 24,
                set_margin_bottom: 24,
                set_margin_start: 12,
                set_margin_end: 12,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,

                    adw::PreferencesGroup {
                        set_title: "Transcription Models",
                        set_description: Some(
                            "Download a model to enable transcription. \
                             Larger models are more accurate but slower.",
                        ),

                        #[local_ref]
                        list_box -> gtk::ListBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            add_css_class: "boxed-list",
                        },
                    },
                }
            }
        }
    }

    fn init(
        (ctx, model_manager): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut models = model_manager.get_available_models();
        models.sort_by(|a, b| a.name.cmp(&b.name));

        let list_box = gtk::ListBox::new();
        list_box.set_selection_mode(gtk::SelectionMode::None);
        list_box.add_css_class("boxed-list");

        let selected = ctx.settings().selected_model.clone();
        let loaded = model_manager.get_loaded_model_id();
        for info in &models {
            list_box.append(&build_model_row(info, &selected, &loaded, &sender));
        }

        let model = ModelsPage {
            ctx,
            model_manager,
            models,
            list_box: list_box.clone(),
            loaded_model_id: String::new(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: ModelsInput, sender: ComponentSender<Self>) {
        match msg {
            ModelsInput::Download(model_id) => {
                let mgr = Arc::clone(&self.model_manager);
                let id = model_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = mgr.download_model(&id).await {
                        tracing::error!("Download failed for {}: {}", id, e);
                    }
                });
                // Mark as downloading in local state immediately so the UI updates.
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloading = true;
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::CancelDownload(model_id) => {
                self.model_manager.cancel_download(&model_id);
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloading = false;
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::Delete(model_id) => {
                if let Err(e) = self.model_manager.delete_model(&model_id) {
                    tracing::error!("Delete failed for {}: {}", model_id, e);
                }
                // ModelDeleted event will arrive via BackendEvent and trigger rebuild.
            }
            ModelsInput::Select(model_id) => {
                self.ctx.update_settings(|s| s.selected_model = model_id);
                self.rebuild_rows(&sender);
            }
            ModelsInput::Unload => {
                if let Err(e) = self.model_manager.unload_model() {
                    tracing::warn!("Failed to unload model: {}", e);
                }
            }
            ModelsInput::ModelStateChanged { model_id, loaded } => {
                if loaded {
                    self.loaded_model_id = model_id;
                } else if self.loaded_model_id == model_id {
                    self.loaded_model_id.clear();
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::DownloadProgress {
                model_id, progress, ..
            } => {
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloading = true;
                    // Update partial_size as a proxy for progress so rows reflect state.
                    if m.size_mb > 0 {
                        m.partial_size = (progress as u64) * m.size_mb * 1024 * 1024 / 100;
                    }
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::DownloadCompleted(model_id) => {
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloading = false;
                    m.is_downloaded = true;
                    m.partial_size = 0;
                }
                // Auto-select if nothing is selected yet.
                if self.ctx.settings().selected_model.is_empty() {
                    self.ctx.update_settings(|s| s.selected_model = model_id);
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::DownloadFailed { model_id, error } => {
                tracing::warn!("Download failed for {}: {}", model_id, error);
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloading = false;
                }
                self.rebuild_rows(&sender);
            }
            ModelsInput::ModelDeleted(model_id) => {
                if let Some(m) = self.models.iter_mut().find(|m| m.id == model_id) {
                    m.is_downloaded = false;
                    m.partial_size = 0;
                }
                // Clear selection if the deleted model was selected.
                if self.ctx.settings().selected_model == model_id {
                    self.ctx
                        .update_settings(|s| s.selected_model = String::new());
                }
                self.rebuild_rows(&sender);
            }
        }
    }
}

impl ModelsPage {
    fn rebuild_rows(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        let selected = self.ctx.settings().selected_model.clone();
        let loaded = Some(self.loaded_model_id.clone());
        for info in &self.models {
            self.list_box
                .append(&build_model_row(info, &selected, &loaded, sender));
        }
    }
}

fn build_model_row(
    info: &ModelInfo,
    selected_model: &str,
    loaded_model_id: &Option<String>,
    sender: &ComponentSender<ModelsPage>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&info.name)
        .subtitle(format!("{} — {} MB", info.description, info.size_mb))
        .build();

    if info.is_downloading {
        let progress_bar = gtk::ProgressBar::new();
        progress_bar.set_valign(gtk::Align::Center);
        progress_bar.set_pulse_step(0.1);
        progress_bar.pulse();

        let cancel_btn = gtk::Button::builder()
            .icon_name("process-stop-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(vec!["flat".to_string()])
            .tooltip_text("Cancel download")
            .build();
        let id = info.id.clone();
        let s = sender.clone();
        cancel_btn.connect_clicked(move |_| {
            s.input(ModelsInput::CancelDownload(id.clone()));
        });

        row.add_suffix(&progress_bar);
        row.add_suffix(&cancel_btn);
    } else if info.is_downloaded {
        // Show Unload button for the model currently loaded in memory
        if loaded_model_id.as_deref() == Some(&info.id) {
            let unload_btn = gtk::Button::builder()
                .label("Unload")
                .valign(gtk::Align::Center)
                .css_classes(vec!["destructive-action".to_string()])
                .tooltip_text("Unload model from memory")
                .build();
            let s = sender.clone();
            unload_btn.connect_clicked(move |_| {
                s.input(ModelsInput::Unload);
            });
            row.add_suffix(&unload_btn);
        } else if selected_model == info.id {
            let check = gtk::Image::builder()
                .icon_name("emblem-ok-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text("Active model")
                .build();
            row.add_suffix(&check);
        } else {
            let use_btn = gtk::Button::builder()
                .label("Use")
                .valign(gtk::Align::Center)
                .css_classes(vec!["suggested-action".to_string()])
                .tooltip_text("Set as active model")
                .build();
            let id = info.id.clone();
            let s = sender.clone();
            use_btn.connect_clicked(move |_| {
                s.input(ModelsInput::Select(id.clone()));
            });
            row.add_suffix(&use_btn);
        }

        let delete_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(vec!["flat".to_string(), "destructive-action".to_string()])
            .tooltip_text("Delete model")
            .build();
        let id = info.id.clone();
        let s = sender.clone();
        delete_btn.connect_clicked(move |_| {
            s.input(ModelsInput::Delete(id.clone()));
        });
        row.add_suffix(&delete_btn);
    } else if info.url.is_some() {
        let download_btn = gtk::Button::builder()
            .label("Download")
            .valign(gtk::Align::Center)
            .tooltip_text(format!("{} MB", info.size_mb))
            .build();
        let id = info.id.clone();
        let s = sender.clone();
        download_btn.connect_clicked(move |_| {
            s.input(ModelsInput::Download(id.clone()));
        });
        row.add_suffix(&download_btn);
    }

    row
}
