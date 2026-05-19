use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime};

use transcribe_rs::{
    onnx::{
        canary::CanaryModel,
        cohere::CohereModel,
        gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity},
        sense_voice::{SenseVoiceModel, SenseVoiceParams},
        Quantization,
    },
    SpeechModel, TranscribeOptions,
};

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;
use crate::config::ModelUnloadTimeout;
use crate::managers::model::{EngineType, ModelManager};

enum LoadedEngine {
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAM(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<Option<LoadedEngine>>>,
    model_manager: Arc<ModelManager>,
    ctx: AppContext,
    current_model_id: Arc<Mutex<Option<String>>>,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl TranscriptionManager {
    pub fn new(ctx: AppContext, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(Mutex::new(None)),
            model_manager,
            ctx: ctx.clone(),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(AtomicU64::new(Self::now_ms())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
        };

        {
            let mgr = manager.clone();
            let shutdown = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                tracing::debug!("TranscriptionManager idle watcher started");
                while !shutdown.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10));
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }

                    let timeout = mgr.ctx.settings().model_unload_timeout;
                    if timeout == ModelUnloadTimeout::Immediately {
                        continue;
                    }

                    if let Some(limit_secs) = timeout.to_seconds() {
                        let last = mgr.last_activity.load(Ordering::Relaxed);
                        let idle_ms = Self::now_ms().saturating_sub(last);
                        if idle_ms > limit_secs * 1000 && mgr.is_model_loaded() {
                            tracing::info!(
                                "Model idle for {}s (limit: {}s), unloading",
                                idle_ms / 1000,
                                limit_secs
                            );
                            if let Err(e) = mgr.unload_model() {
                                tracing::warn!("Failed to unload idle model: {e}");
                            }
                        }
                    }
                }
                tracing::debug!("TranscriptionManager idle watcher stopped");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    fn lock_engine(&self) -> MutexGuard<'_, Option<LoadedEngine>> {
        self.engine.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Engine mutex was poisoned, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        self.lock_engine().is_some()
    }

    pub fn unload_model(&self) -> Result<()> {
        {
            let mut engine = self.lock_engine();
            *engine = None;
        }
        {
            let mut current = self.current_model_id.lock().unwrap();
            if let Some(id) = current.take() {
                tracing::info!("Model '{}' unloaded", id);
                if let Err(e) = self.model_manager.unload_model() {
                    tracing::warn!("ModelManager.unload_model failed: {e}");
                }
            }
        }
        Ok(())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn touch_activity(&self) {
        self.last_activity.store(Self::now_ms(), Ordering::Relaxed);
    }

    fn maybe_unload_immediately(&self) {
        let settings = self.ctx.settings();
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            if let Err(e) = self.unload_model() {
                tracing::warn!("Failed to immediately unload model: {e}");
            }
        }
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            anyhow::bail!("Model '{}' is not downloaded", model_id);
        }

        let model_path = self.model_manager.get_model_path(model_id)?;

        let loaded = match model_info.engine_type {
            EngineType::Parakeet => {
                let engine = ParakeetModel::load(&model_path, &Quantization::Int8)
                    .map_err(|e| anyhow::anyhow!("Failed to load Parakeet: {e}"))?;
                LoadedEngine::Parakeet(engine)
            }
            EngineType::Moonshine => {
                let engine = MoonshineModel::load(
                    &model_path,
                    MoonshineVariant::Base,
                    &Quantization::default(),
                )
                .map_err(|e| anyhow::anyhow!("Failed to load Moonshine: {e}"))?;
                LoadedEngine::Moonshine(engine)
            }
            EngineType::MoonshineStreaming => {
                let engine = StreamingModel::load(&model_path, 0, &Quantization::default())
                    .map_err(|e| anyhow::anyhow!("Failed to load MoonshineStreaming: {e}"))?;
                LoadedEngine::MoonshineStreaming(engine)
            }
            EngineType::SenseVoice => {
                let engine = SenseVoiceModel::load(&model_path, &Quantization::Int8)
                    .map_err(|e| anyhow::anyhow!("Failed to load SenseVoice: {e}"))?;
                LoadedEngine::SenseVoice(engine)
            }
            EngineType::GigaAM => {
                let engine = GigaAMModel::load(&model_path, &Quantization::Int8)
                    .map_err(|e| anyhow::anyhow!("Failed to load GigaAM: {e}"))?;
                LoadedEngine::GigaAM(engine)
            }
            EngineType::Canary => {
                let engine = CanaryModel::load(&model_path, &Quantization::Int8)
                    .map_err(|e| anyhow::anyhow!("Failed to load Canary: {e}"))?;
                LoadedEngine::Canary(engine)
            }
            EngineType::Cohere => {
                let engine = CohereModel::load(&model_path, &Quantization::Int8)
                    .map_err(|e| anyhow::anyhow!("Failed to load Cohere: {e}"))?;
                LoadedEngine::Cohere(engine)
            }
            EngineType::Whisper => {
                anyhow::bail!(
                    "Whisper engine is not available in this build. \
                     Please select a Parakeet, Moonshine, SenseVoice, GigaAM, Canary, or Cohere model."
                );
            }
        };

        {
            let mut engine = self.lock_engine();
            *engine = Some(loaded);
        }
        {
            let mut current = self.current_model_id.lock().unwrap();
            *current = Some(model_id.to_string());
        }

        self.touch_activity();

        if let Err(e) = self.model_manager.load_model(model_id) {
            tracing::warn!("ModelManager.load_model failed: {e}");
        }

        tracing::info!("Loaded transcription model: {}", model_id);
        Ok(())
    }

    /// Initiates a background model load if no model is currently loaded or loading.
    pub fn initiate_model_load(&self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading || self.is_model_loaded() {
            return;
        }
        *is_loading = true;

        let mgr = self.clone();
        thread::spawn(move || {
            let settings = mgr.ctx.settings();
            if settings.selected_model.is_empty() {
                tracing::warn!("No model selected, cannot load");
            } else if let Err(e) = mgr.load_model(&settings.selected_model) {
                tracing::error!("Failed to load model '{}': {e}", settings.selected_model);
                mgr.ctx.emit(BackendEvent::ModelDownloadFailed {
                    model_id: settings.selected_model.clone(),
                    error: e.to_string(),
                });
            }
            let mut is_loading = mgr.is_loading.lock().unwrap();
            *is_loading = false;
            mgr.loading_condvar.notify_all();
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        self.current_model_id.lock().unwrap().clone()
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        self.touch_activity();

        if audio.is_empty() {
            self.maybe_unload_immediately();
            return Ok(String::new());
        }

        {
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }

            if self.lock_engine().is_none() {
                anyhow::bail!("No model is loaded. Please download and select a model.");
            }
        }

        let settings = self.ctx.settings();

        let validated_language = if settings.selected_language == "auto" {
            "auto".to_string()
        } else {
            let model_id = settings.selected_model.clone();
            let is_supported = self
                .model_manager
                .get_model_info(&model_id)
                .map(|info| {
                    info.supported_languages.is_empty()
                        || info
                            .supported_languages
                            .contains(&settings.selected_language)
                })
                .unwrap_or(true);

            if is_supported {
                settings.selected_language.clone()
            } else {
                tracing::warn!(
                    "Language '{}' not supported by current model, falling back to auto",
                    settings.selected_language
                );
                "auto".to_string()
            }
        };

        let st = std::time::Instant::now();

        let result = {
            let mut engine_guard = self.lock_engine();
            let mut engine = match engine_guard.take() {
                Some(e) => e,
                None => anyhow::bail!("No model loaded"),
            };
            drop(engine_guard);

            let transcribe_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String> {
                    match &mut engine {
                        LoadedEngine::Parakeet(e) => {
                            let params = ParakeetParams {
                                timestamp_granularity: Some(TimestampGranularity::Segment),
                                ..Default::default()
                            };
                            e.transcribe_with(&audio, &params)
                                .map(|r| r.text)
                                .map_err(|e| anyhow::anyhow!("Parakeet failed: {e}"))
                        }
                        LoadedEngine::Moonshine(e) => e
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map(|r| r.text)
                            .map_err(|e| anyhow::anyhow!("Moonshine failed: {e}")),
                        LoadedEngine::MoonshineStreaming(e) => e
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map(|r| r.text)
                            .map_err(|e| anyhow::anyhow!("MoonshineStreaming failed: {e}")),
                        LoadedEngine::SenseVoice(e) => {
                            let lang = match validated_language.as_str() {
                                "zh" | "zh-Hans" | "zh-Hant" => Some("zh".to_string()),
                                "en" => Some("en".to_string()),
                                "ja" => Some("ja".to_string()),
                                "ko" => Some("ko".to_string()),
                                "yue" => Some("yue".to_string()),
                                _ => None,
                            };
                            let params = SenseVoiceParams {
                                language: lang,
                                use_itn: Some(true),
                            };
                            e.transcribe_with(&audio, &params)
                                .map(|r| r.text)
                                .map_err(|e| anyhow::anyhow!("SenseVoice failed: {e}"))
                        }
                        LoadedEngine::GigaAM(e) => e
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map(|r| r.text)
                            .map_err(|e| anyhow::anyhow!("GigaAM failed: {e}")),
                        LoadedEngine::Canary(e) => {
                            let lang =
                                (validated_language != "auto").then(|| validated_language.clone());
                            let opts = TranscribeOptions {
                                language: lang,
                                translate: settings.translate_to_english,
                                ..Default::default()
                            };
                            e.transcribe(&audio, &opts)
                                .map(|r| r.text)
                                .map_err(|e| anyhow::anyhow!("Canary failed: {e}"))
                        }
                        LoadedEngine::Cohere(e) => {
                            let lang = if validated_language == "auto" {
                                None
                            } else if validated_language == "zh-Hans"
                                || validated_language == "zh-Hant"
                            {
                                Some("zh".to_string())
                            } else {
                                Some(validated_language.clone())
                            };
                            let opts = TranscribeOptions {
                                language: lang,
                                ..Default::default()
                            };
                            e.transcribe(&audio, &opts)
                                .map(|r| r.text)
                                .map_err(|e| anyhow::anyhow!("Cohere failed: {e}"))
                        }
                    }
                }));

            match transcribe_result {
                Ok(inner) => {
                    let mut guard = self.lock_engine();
                    *guard = Some(engine);
                    inner?
                }
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown panic".to_string());

                    tracing::error!("Transcription engine panicked: {}. Model unloaded.", msg);

                    let mut current = self
                        .current_model_id
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    *current = None;

                    if let Err(e) = self.model_manager.unload_model() {
                        tracing::warn!("ModelManager.unload_model after panic: {e}");
                    }

                    anyhow::bail!(
                        "Transcription engine panicked: {msg}. The model has been unloaded."
                    );
                }
            }
        };

        tracing::info!("Transcription completed in {}ms", st.elapsed().as_millis());

        if result.is_empty() {
            tracing::info!("Transcription result is empty");
        } else {
            tracing::info!("Transcription result: {result}");
        }

        self.maybe_unload_immediately();
        Ok(result)
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.engine) > 1 {
            return;
        }
        self.shutdown_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.watcher_handle.lock().unwrap().take() {
            if let Err(e) = handle.join() {
                tracing::warn!("Failed to join idle watcher thread: {e:?}");
            }
        }
    }
}
