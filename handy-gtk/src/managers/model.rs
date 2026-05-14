use anyhow::Result;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tar::Archive;

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineType {
    Whisper,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAM,
    Canary,
    Cohere,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_directory: bool,
    pub engine_type: EngineType,
    pub accuracy_score: f32,
    pub speed_score: f32,
    pub supports_translation: bool,
    pub is_recommended: bool,
    pub supported_languages: Vec<String>,
    pub supports_language_selection: bool,
    pub is_custom: bool,
}

/// RAII guard that cleans up `is_downloading` and cancel flag on every error path.
struct DownloadCleanup<'a> {
    available_models: &'a Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: &'a Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    model_id: String,
    disarmed: bool,
}

impl Drop for DownloadCleanup<'_> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(self.model_id.as_str()) {
                model.is_downloading = false;
            }
        }
        self.cancel_flags.lock().unwrap().remove(&self.model_id);
    }
}

pub struct ModelManager {
    ctx: AppContext,
    models_dir: PathBuf,
    available_models: Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    extracting_models: Arc<Mutex<HashSet<String>>>,
}

impl ModelManager {
    pub fn new(ctx: AppContext) -> Result<Arc<Self>> {
        let models_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine XDG data directory"))?
            .join("handy")
            .join("models");

        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)?;
        }

        let whisper_languages: Vec<String> = vec![
            "en", "zh", "zh-Hans", "zh-Hant", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl",
            "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs",
            "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy",
            "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is",
            "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo",
            "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht",
            "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
            "ha", "ba", "jw", "su", "yue",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let mut available_models: HashMap<String, ModelInfo> = HashMap::new();

        available_models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Fast and fairly accurate.".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-small.bin".to_string()),
                sha256: Some(
                    "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b".to_string(),
                ),
                size_mb: 465,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.60,
                speed_score: 0.85,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "medium".to_string(),
            ModelInfo {
                id: "medium".to_string(),
                name: "Whisper Medium".to_string(),
                description: "Good accuracy, medium speed.".to_string(),
                filename: "whisper-medium-q4_1.bin".to_string(),
                url: Some("https://blob.handy.computer/whisper-medium-q4_1.bin".to_string()),
                sha256: Some(
                    "79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57".to_string(),
                ),
                size_mb: 469,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.75,
                speed_score: 0.60,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "turbo".to_string(),
            ModelInfo {
                id: "turbo".to_string(),
                name: "Whisper Turbo".to_string(),
                description: "Balanced accuracy and speed.".to_string(),
                filename: "ggml-large-v3-turbo.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-turbo.bin".to_string()),
                sha256: Some(
                    "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69".to_string(),
                ),
                size_mb: 1549,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.80,
                speed_score: 0.40,
                supports_translation: false,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "large".to_string(),
            ModelInfo {
                id: "large".to_string(),
                name: "Whisper Large".to_string(),
                description: "Best accuracy, but slow.".to_string(),
                filename: "ggml-large-v3-q5_0.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-q5_0.bin".to_string()),
                sha256: Some(
                    "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1".to_string(),
                ),
                size_mb: 1031,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.30,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "parakeet-tdt-0.6b-v2".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v2".to_string(),
                name: "Parakeet V2".to_string(),
                description: "English only. The best model for English speakers.".to_string(),
                filename: "parakeet-tdt-0.6b-v2-int8".to_string(),
                url: Some("https://blob.handy.computer/parakeet-v2-int8.tar.gz".to_string()),
                sha256: Some(
                    "ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991".to_string(),
                ),
                size_mb: 451,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.85,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        let parakeet_v3_languages: Vec<String> = vec![
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "parakeet-tdt-0.6b-v3".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v3".to_string(),
                name: "Parakeet V3".to_string(),
                description: "Fast and accurate. Supports 25 European languages.".to_string(),
                filename: "parakeet-tdt-0.6b-v3-int8".to_string(),
                url: Some("https://blob.handy.computer/parakeet-v3-int8.tar.gz".to_string()),
                sha256: Some(
                    "43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77".to_string(),
                ),
                size_mb: 456,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.80,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: true,
                supported_languages: parakeet_v3_languages,
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-base".to_string(),
            ModelInfo {
                id: "moonshine-base".to_string(),
                name: "Moonshine Base".to_string(),
                description: "Very fast, English only. Handles accents well.".to_string(),
                filename: "moonshine-base".to_string(),
                url: Some("https://blob.handy.computer/moonshine-base.tar.gz".to_string()),
                sha256: Some(
                    "04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8".to_string(),
                ),
                size_mb: 55,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Moonshine,
                accuracy_score: 0.70,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-tiny-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-tiny-streaming-en".to_string(),
                name: "Moonshine V2 Tiny".to_string(),
                description: "Ultra-fast, English only.".to_string(),
                filename: "moonshine-tiny-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-tiny-streaming-en.tar.gz".to_string(),
                ),
                sha256: Some(
                    "465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf".to_string(),
                ),
                size_mb: 31,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.55,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-small-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-small-streaming-en".to_string(),
                name: "Moonshine V2 Small".to_string(),
                description: "Fast, English only. Good balance of speed and accuracy.".to_string(),
                filename: "moonshine-small-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-small-streaming-en.tar.gz".to_string(),
                ),
                sha256: Some(
                    "dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5".to_string(),
                ),
                size_mb: 99,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.65,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-medium-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-medium-streaming-en".to_string(),
                name: "Moonshine V2 Medium".to_string(),
                description: "English only. High quality.".to_string(),
                filename: "moonshine-medium-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-medium-streaming-en.tar.gz".to_string(),
                ),
                sha256: Some(
                    "07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13".to_string(),
                ),
                size_mb: 192,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.75,
                speed_score: 0.80,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        let sense_voice_languages: Vec<String> =
            vec!["zh", "zh-Hans", "zh-Hant", "en", "yue", "ja", "ko"]
                .into_iter()
                .map(String::from)
                .collect();

        available_models.insert(
            "sense-voice-int8".to_string(),
            ModelInfo {
                id: "sense-voice-int8".to_string(),
                name: "SenseVoice".to_string(),
                description: "Very fast. Chinese, English, Japanese, Korean, Cantonese."
                    .to_string(),
                filename: "sense-voice-int8".to_string(),
                url: Some("https://blob.handy.computer/sense-voice-int8.tar.gz".to_string()),
                sha256: Some(
                    "171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4".to_string(),
                ),
                size_mb: 152,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::SenseVoice,
                accuracy_score: 0.65,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: sense_voice_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        let gigaam_languages: Vec<String> = vec!["ru"].into_iter().map(String::from).collect();

        available_models.insert(
            "gigaam-v3-e2e-ctc".to_string(),
            ModelInfo {
                id: "gigaam-v3-e2e-ctc".to_string(),
                name: "GigaAM v3".to_string(),
                description: "Russian speech recognition. Fast and accurate.".to_string(),
                filename: "giga-am-v3-int8".to_string(),
                url: Some("https://blob.handy.computer/giga-am-v3-int8.tar.gz".to_string()),
                sha256: Some(
                    "d872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b".to_string(),
                ),
                size_mb: 151,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::GigaAM,
                accuracy_score: 0.85,
                speed_score: 0.75,
                supports_translation: false,
                is_recommended: false,
                supported_languages: gigaam_languages,
                supports_language_selection: false,
                is_custom: false,
            },
        );

        let canary_flash_languages: Vec<String> = vec!["en", "de", "es", "fr"]
            .into_iter()
            .map(String::from)
            .collect();

        available_models.insert(
            "canary-180m-flash".to_string(),
            ModelInfo {
                id: "canary-180m-flash".to_string(),
                name: "Canary 180M Flash".to_string(),
                description: "Very fast. English, German, Spanish, French. Supports translation."
                    .to_string(),
                filename: "canary-180m-flash".to_string(),
                url: Some("https://blob.handy.computer/canary-180m-flash.tar.gz".to_string()),
                sha256: Some(
                    "6d9cfca6118b296e196eaedc1c8fa9788305a7b0f1feafdb6dc91932ab6e53f7".to_string(),
                ),
                size_mb: 146,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Canary,
                accuracy_score: 0.75,
                speed_score: 0.85,
                supports_translation: true,
                is_recommended: false,
                supported_languages: canary_flash_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        let canary_1b_languages: Vec<String> = vec![
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "canary-1b-v2".to_string(),
            ModelInfo {
                id: "canary-1b-v2".to_string(),
                name: "Canary 1B v2".to_string(),
                description: "Accurate multilingual. 25 European languages. Supports translation."
                    .to_string(),
                filename: "canary-1b-v2".to_string(),
                url: Some("https://blob.handy.computer/canary-1b-v2.tar.gz".to_string()),
                sha256: Some(
                    "02305b2a25f9cf3e7deaffa7f94df00efa44f442cd55c101c2cb9c000f904666".to_string(),
                ),
                size_mb: 691,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Canary,
                accuracy_score: 0.85,
                speed_score: 0.70,
                supports_translation: true,
                is_recommended: false,
                supported_languages: canary_1b_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        let cohere_languages: Vec<String> = vec![
            "en", "fr", "de", "it", "es", "pt", "el", "nl", "pl", "zh", "zh-Hans", "zh-Hant", "ja",
            "ko", "vi", "ar",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "cohere-int8".to_string(),
            ModelInfo {
                id: "cohere-int8".to_string(),
                name: "Cohere".to_string(),
                description: "A large, slower, but very accurate multilingual model.".to_string(),
                filename: "cohere-int8".to_string(),
                url: Some("https://blob.handy.computer/cohere-int8.tar.gz".to_string()),
                sha256: Some(
                    "ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0".to_string(),
                ),
                size_mb: 1708,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Cohere,
                accuracy_score: 0.90,
                speed_score: 0.60,
                supports_translation: false,
                is_recommended: false,
                supported_languages: cohere_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        if let Err(e) = Self::discover_custom_whisper_models(&models_dir, &mut available_models) {
            tracing::warn!("Failed to discover custom models: {}", e);
        }

        let manager = Arc::new(Self {
            ctx,
            models_dir,
            available_models: Mutex::new(available_models),
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            extracting_models: Arc::new(Mutex::new(HashSet::new())),
        });

        manager.update_download_status()?;
        manager.auto_select_model_if_needed()?;

        Ok(manager)
    }

    pub fn get_available_models(&self) -> Vec<ModelInfo> {
        self.available_models
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        self.available_models.lock().unwrap().get(model_id).cloned()
    }

    pub fn get_model_path(&self, model_id: &str) -> Result<PathBuf> {
        let info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !info.is_downloaded {
            return Err(anyhow::anyhow!("Model not downloaded: {}", model_id));
        }
        if info.is_downloading {
            return Err(anyhow::anyhow!(
                "Model is currently downloading: {}",
                model_id
            ));
        }

        let model_path = self.models_dir.join(&info.filename);
        let partial_path = self.models_dir.join(format!("{}.partial", &info.filename));

        if info.is_directory {
            if model_path.exists() && model_path.is_dir() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model directory not found: {}",
                    model_id
                ))
            }
        } else if model_path.exists() && !partial_path.exists() {
            Ok(model_path)
        } else {
            Err(anyhow::anyhow!(
                "Complete model file not found: {}",
                model_id
            ))
        }
    }

    fn update_download_status(&self) -> Result<()> {
        let mut models = self.available_models.lock().unwrap();

        for model in models.values_mut() {
            let model_path = self.models_dir.join(&model.filename);
            let partial_path = self.models_dir.join(format!("{}.partial", &model.filename));

            if model.is_directory {
                let extracting_path = self
                    .models_dir
                    .join(format!("{}.extracting", &model.filename));
                let is_extracting = {
                    let extracting = self.extracting_models.lock().unwrap();
                    extracting.contains(&model.id)
                };
                if extracting_path.exists() && !is_extracting {
                    tracing::warn!("Cleaning up interrupted extraction for model: {}", model.id);
                    let _ = fs::remove_dir_all(&extracting_path);
                }
                model.is_downloaded = model_path.exists() && model_path.is_dir();
            } else {
                model.is_downloaded = model_path.exists();
            }

            model.is_downloading = false;
            model.partial_size = if partial_path.exists() {
                partial_path.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
        }

        Ok(())
    }

    fn auto_select_model_if_needed(&self) -> Result<()> {
        let settings = self.ctx.settings();

        if !settings.selected_model.is_empty() {
            let exists = self
                .available_models
                .lock()
                .unwrap()
                .contains_key(&settings.selected_model);
            if !exists {
                tracing::info!(
                    "Selected model '{}' not in available models, clearing",
                    settings.selected_model
                );
                self.ctx
                    .update_settings(|s| s.selected_model = String::new());
            } else {
                return Ok(());
            }
        }

        let first_downloaded = self
            .available_models
            .lock()
            .unwrap()
            .values()
            .find(|m| m.is_downloaded)
            .map(|m| m.id.clone());

        if let Some(id) = first_downloaded {
            tracing::info!("Auto-selecting model: {}", id);
            self.ctx.update_settings(|s| s.selected_model = id);
        }

        Ok(())
    }

    fn discover_custom_whisper_models(
        models_dir: &Path,
        available_models: &mut HashMap<String, ModelInfo>,
    ) -> Result<()> {
        if !models_dir.exists() {
            return Ok(());
        }

        let predefined_filenames: HashSet<String> = available_models
            .values()
            .filter(|m| matches!(m.engine_type, EngineType::Whisper) && !m.is_directory)
            .map(|m| m.filename.clone())
            .collect();

        for entry in fs::read_dir(models_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if filename.starts_with('.') || !filename.ends_with(".bin") {
                continue;
            }
            if predefined_filenames.contains(&filename) {
                continue;
            }

            let model_id = filename.trim_end_matches(".bin").to_string();
            if available_models.contains_key(&model_id) {
                continue;
            }

            let display_name = model_id
                .replace(['-', '_'], " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            let size_mb = path
                .metadata()
                .map(|m| m.len() / (1024 * 1024))
                .unwrap_or(0);

            available_models.insert(
                model_id.clone(),
                ModelInfo {
                    id: model_id,
                    name: display_name,
                    description: "Custom model (not officially supported).".to_string(),
                    filename,
                    url: None,
                    sha256: None,
                    size_mb,
                    is_downloaded: true,
                    is_downloading: false,
                    partial_size: 0,
                    is_directory: false,
                    engine_type: EngineType::Whisper,
                    accuracy_score: 0.0,
                    speed_score: 0.0,
                    supports_translation: false,
                    is_recommended: false,
                    supported_languages: vec![],
                    supports_language_selection: true,
                    is_custom: true,
                },
            );
        }

        Ok(())
    }

    fn verify_sha256(path: &Path, expected_sha256: Option<&str>, model_id: &str) -> Result<()> {
        let Some(expected) = expected_sha256 else {
            return Ok(());
        };
        match Self::compute_sha256(path) {
            Ok(actual) if actual == expected => {
                tracing::info!("SHA256 verified for model {}", model_id);
                Ok(())
            }
            Ok(actual) => {
                tracing::warn!(
                    "SHA256 mismatch for model {}: expected {}, got {}",
                    model_id,
                    expected,
                    actual
                );
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Download verification failed for model {}: file is corrupt. Please retry.",
                    model_id
                ))
            }
            Err(e) => {
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Failed to verify download for model {}: {}. Please retry.",
                    model_id,
                    e
                ))
            }
        }
    }

    fn compute_sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub async fn download_model(self: &Arc<Self>, model_id: &str) -> Result<()> {
        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };
        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        let url = model_info
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No download URL for model: {}", model_id))?;

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        if model_path.exists() {
            if partial_path.exists() {
                let _ = fs::remove_file(&partial_path);
            }
            self.update_download_status()?;
            return Ok(());
        }

        let mut resume_from = if partial_path.exists() {
            let size = partial_path.metadata()?.len();
            tracing::info!("Resuming download of {} from byte {}", model_id, size);
            size
        } else {
            tracing::info!("Starting download of {} from {}", model_id, url);
            0
        };

        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(m) = models.get_mut(model_id) {
                m.is_downloading = true;
            }
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            self.cancel_flags
                .lock()
                .unwrap()
                .insert(model_id.to_string(), cancel_flag.clone());
        }

        let mut cleanup = DownloadCleanup {
            available_models: &self.available_models,
            cancel_flags: &self.cancel_flags,
            model_id: model_id.to_string(),
            disarmed: false,
        };

        let client = reqwest::Client::new();
        let mut request = client.get(&url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }
        let mut response = request.send().await?;

        if resume_from > 0
            && (response.status() == reqwest::StatusCode::OK
                || response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE)
        {
            tracing::warn!(
                "Restarting download of {} (server status: {})",
                model_id,
                response.status()
            );
            drop(response);
            let _ = fs::remove_file(&partial_path);
            resume_from = 0;
            response = client.get(&url).send().await?;
        }

        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };

        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        let mut file = if resume_from > 0 {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            std::fs::File::create(&partial_path)?
        };

        let mut last_emit = Instant::now();
        let throttle = Duration::from_millis(100);
        let mut speed_window_start = Instant::now();
        let mut speed_window_bytes: u64 = 0;
        let mut current_speed_bps: u64 = 0;

        // Emit initial progress
        self.ctx.emit(BackendEvent::ModelDownloadProgress {
            model_id: model_id.to_string(),
            progress: 0.0,
            speed_bps: 0,
            eta_secs: 0,
        });

        while let Some(chunk) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                drop(file);
                tracing::info!("Download cancelled for: {}", model_id);
                return Ok(());
            }

            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            speed_window_bytes += chunk.len() as u64;

            // Recompute speed once per second
            let window_elapsed = speed_window_start.elapsed();
            if window_elapsed >= Duration::from_secs(1) {
                current_speed_bps =
                    (speed_window_bytes as f64 / window_elapsed.as_secs_f64()) as u64;
                speed_window_bytes = 0;
                speed_window_start = Instant::now();
            }

            if last_emit.elapsed() >= throttle {
                let progress = if total_size > 0 {
                    downloaded as f32 / total_size as f32
                } else {
                    0.0
                };
                let remaining = total_size.saturating_sub(downloaded);
                let eta_secs = remaining.checked_div(current_speed_bps).unwrap_or(0);
                self.ctx.emit(BackendEvent::ModelDownloadProgress {
                    model_id: model_id.to_string(),
                    progress,
                    speed_bps: current_speed_bps,
                    eta_secs,
                });
                last_emit = Instant::now();
            }
        }

        file.flush()?;
        drop(file);

        if total_size > 0 {
            let actual = partial_path.metadata()?.len();
            if actual != total_size {
                let _ = fs::remove_file(&partial_path);
                return Err(anyhow::anyhow!(
                    "Download incomplete: expected {} bytes, got {}",
                    total_size,
                    actual
                ));
            }
        }

        tracing::info!("Verifying SHA256 for model {}...", model_id);
        let verify_path = partial_path.clone();
        let verify_expected = model_info.sha256.clone();
        let verify_id = model_id.to_string();
        let verify_result = tokio::task::spawn_blocking(move || {
            Self::verify_sha256(&verify_path, verify_expected.as_deref(), &verify_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("SHA256 task panicked: {}", e))?;
        verify_result?;

        if model_info.is_directory {
            {
                self.extracting_models
                    .lock()
                    .unwrap()
                    .insert(model_id.to_string());
            }

            let temp_extract_dir = self
                .models_dir
                .join(format!("{}.extracting", &model_info.filename));
            let final_model_dir = self.models_dir.join(&model_info.filename);

            if temp_extract_dir.exists() {
                let _ = fs::remove_dir_all(&temp_extract_dir);
            }
            fs::create_dir_all(&temp_extract_dir)?;

            let tar_gz = File::open(&partial_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

            if let Err(e) = archive.unpack(&temp_extract_dir) {
                let _ = fs::remove_dir_all(&temp_extract_dir);
                let _ = fs::remove_file(&partial_path);
                self.extracting_models.lock().unwrap().remove(model_id);
                return Err(anyhow::anyhow!("Failed to extract archive: {}", e));
            }

            let extracted_dirs: Vec<_> = fs::read_dir(&temp_extract_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect();

            if extracted_dirs.len() == 1 {
                let source = extracted_dirs[0].path();
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&source, &final_model_dir)?;
                let _ = fs::remove_dir_all(&temp_extract_dir);
            } else {
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&temp_extract_dir, &final_model_dir)?;
            }

            self.extracting_models.lock().unwrap().remove(model_id);
            let _ = fs::remove_file(&partial_path);
        } else {
            fs::rename(&partial_path, &model_path)?;
        }

        cleanup.disarmed = true;
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(m) = models.get_mut(model_id) {
                m.is_downloading = false;
                m.is_downloaded = true;
                m.partial_size = 0;
            }
        }
        self.cancel_flags.lock().unwrap().remove(model_id);

        self.ctx.emit(BackendEvent::ModelDownloadCompleted {
            model_id: model_id.to_string(),
        });

        tracing::info!("Successfully downloaded model: {}", model_id);
        Ok(())
    }

    pub fn cancel_download(&self, model_id: &str) {
        let flags = self.cancel_flags.lock().unwrap();
        if let Some(flag) = flags.get(model_id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        let mut deleted_something = false;

        if model_info.is_directory {
            if model_path.exists() && model_path.is_dir() {
                fs::remove_dir_all(&model_path)?;
                deleted_something = true;
            }
        } else if model_path.exists() {
            fs::remove_file(&model_path)?;
            deleted_something = true;
        }

        if partial_path.exists() {
            fs::remove_file(&partial_path)?;
            deleted_something = true;
        }

        if !deleted_something {
            return Err(anyhow::anyhow!("No model files found to delete"));
        }

        if model_info.is_custom {
            self.available_models.lock().unwrap().remove(model_id);
        } else {
            self.update_download_status()?;
        }

        self.ctx.emit(BackendEvent::ModelDeleted {
            model_id: model_id.to_string(),
        });

        Ok(())
    }

    /// Emits the full model list to the UI as individual `ModelStateChanged` events.
    pub fn emit_model_list(&self) {
        let models = self.available_models.lock().unwrap();
        for m in models.values() {
            self.ctx.emit(BackendEvent::ModelStateChanged {
                model_id: m.id.clone(),
                loaded: m.is_downloaded,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_models_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn discover_custom_whisper_models_finds_bin_files() {
        let dir = make_models_dir();
        let models_dir = dir.path().to_path_buf();

        // Custom .bin file
        let mut f = File::create(models_dir.join("my-model.bin")).unwrap();
        f.write_all(b"fake").unwrap();

        // Should be ignored: predefined filename, hidden, non-bin
        File::create(models_dir.join("ggml-small.bin")).unwrap();
        File::create(models_dir.join(".hidden.bin")).unwrap();
        File::create(models_dir.join("readme.txt")).unwrap();

        let mut available: HashMap<String, ModelInfo> = HashMap::new();
        // Add a predefined entry so it gets excluded
        available.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: String::new(),
                filename: "ggml-small.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 0,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec![],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        ModelManager::discover_custom_whisper_models(&models_dir, &mut available).unwrap();

        assert!(
            available.contains_key("my-model"),
            "custom model should be discovered"
        );
        assert!(
            !available.contains_key(".hidden"),
            "hidden file should be excluded"
        );
        assert_eq!(available["my-model"].is_custom, true);
        assert_eq!(available["my-model"].is_downloaded, true);
    }

    #[test]
    fn discover_custom_whisper_models_skips_directories() {
        let dir = make_models_dir();
        let models_dir = dir.path().to_path_buf();
        fs::create_dir(models_dir.join("dir-model.bin")).unwrap();

        let mut available: HashMap<String, ModelInfo> = HashMap::new();
        ModelManager::discover_custom_whisper_models(&models_dir, &mut available).unwrap();
        assert!(
            !available.contains_key("dir-model"),
            "directory should not be discovered"
        );
    }

    #[test]
    fn verify_sha256_skips_when_none() {
        let dir = make_models_dir();
        let path = dir.path().join("dummy.bin");
        std::fs::write(&path, b"data").unwrap();
        assert!(ModelManager::verify_sha256(&path, None, "test").is_ok());
    }

    #[test]
    fn verify_sha256_detects_mismatch() {
        let dir = make_models_dir();
        let path = dir.path().join("dummy.bin");
        std::fs::write(&path, b"data").unwrap();
        let result = ModelManager::verify_sha256(&path, Some("wronghash"), "test");
        assert!(result.is_err());
        assert!(!path.exists(), "partial should be deleted on mismatch");
    }

    #[tokio::test]
    async fn update_download_status_detects_existing_file() {
        use crate::app_context::AppContext;
        use crate::config::AppSettings;
        use tokio::sync::mpsc;

        let dir = make_models_dir();
        let models_dir = dir.path().to_path_buf();

        // Write a fake model file
        std::fs::write(models_dir.join("ggml-small.bin"), b"model").unwrap();

        let (tx, _rx) = mpsc::channel(32);
        let config_path = dir.path().join("config.toml");
        let ctx = AppContext::new(AppSettings::default(), tx, config_path);

        // We can't call ModelManager::new() because it uses dirs::data_dir().
        // Test update_download_status in isolation by building just what we need.
        let mut available: HashMap<String, ModelInfo> = HashMap::new();
        available.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: String::new(),
                filename: "ggml-small.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 0,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec![],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        let mgr = ModelManager {
            ctx,
            models_dir,
            available_models: Mutex::new(available),
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            extracting_models: Arc::new(Mutex::new(HashSet::new())),
        };

        mgr.update_download_status().unwrap();

        let models = mgr.available_models.lock().unwrap();
        assert!(
            models["small"].is_downloaded,
            "model file exists, should be marked downloaded"
        );
    }
}
