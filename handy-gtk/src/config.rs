use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

const CONFIG_FILENAME: &str = "config.toml";
const CONFIG_DIR: &str = "handy";
const DEBOUNCE_MS: u64 = 500;

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    #[default]
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    #[default]
    Never,
    FiveMinutes,
    TenMinutes,
    ThirtyMinutes,
    OneHour,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    #[default]
    Never,
    ThreeDays,
    TwoWeeks,
    ThreeMonths,
    MatchHistory,
}

/// Paste method: how transcribed text is delivered to the focused application.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    #[default]
    CtrlV,
    ShiftInsert,
    Typing,
    Script,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    Restore,
    Keep,
    Clear,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    None,
    Enter,
    Space,
    Tab,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    #[default]
    Default,
    Custom,
}

/// Whisper GPU acceleration backend (Linux: CPU or Vulkan).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhisperAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Vulkan,
}

/// ONNX Runtime acceleration backend (Linux: CPU, CUDA, ROCm).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    Rocm,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Enigo,
    Xdotool,
}

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

// ── AppSettings ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    #[serde(default = "default_bindings")]
    pub bindings: HashMap<String, ShortcutBinding>,
    #[serde(default)]
    pub push_to_talk: bool,
    #[serde(default = "default_audio_feedback")]
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default)]
    pub sound_theme: SoundTheme,
    #[serde(default)]
    pub start_hidden: bool,
    #[serde(default)]
    pub autostart_enabled: bool,
    #[serde(default)]
    pub selected_model: String,
    #[serde(default)]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default)]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default)]
    pub overlay_position: OverlayPosition,
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default)]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default)]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default)]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default)]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default)]
    pub post_process_api_keys: HashMap<String, String>,
    #[serde(default)]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default)]
    pub typing_tool: TypingTool,
    #[serde(default)]
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub whisper_accelerator: WhisperAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_whisper_gpu_device")]
    pub whisper_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            bindings: default_bindings(),
            push_to_talk: false,
            audio_feedback: default_audio_feedback(),
            audio_feedback_volume: default_audio_feedback_volume(),
            sound_theme: SoundTheme::Default,
            start_hidden: false,
            autostart_enabled: false,
            selected_model: String::new(),
            always_on_microphone: false,
            selected_microphone: None,
            selected_output_device: None,
            translate_to_english: false,
            selected_language: default_selected_language(),
            overlay_position: OverlayPosition::Top,
            debug_mode: false,
            log_level: LogLevel::Info,
            custom_words: Vec::new(),
            model_unload_timeout: ModelUnloadTimeout::Never,
            word_correction_threshold: default_word_correction_threshold(),
            history_limit: default_history_limit(),
            recording_retention_period: RecordingRetentionPeriod::Never,
            paste_method: PasteMethod::CtrlV,
            clipboard_handling: ClipboardHandling::Restore,
            auto_submit: false,
            auto_submit_key: AutoSubmitKey::None,
            post_process_enabled: false,
            post_process_provider_id: default_post_process_provider_id(),
            post_process_providers: default_post_process_providers(),
            post_process_api_keys: HashMap::new(),
            post_process_models: HashMap::new(),
            post_process_prompts: default_post_process_prompts(),
            post_process_selected_prompt_id: None,
            mute_while_recording: false,
            append_trailing_space: false,
            lazy_stream_close: false,
            show_tray_icon: default_show_tray_icon(),
            paste_delay_ms: default_paste_delay_ms(),
            typing_tool: TypingTool::Enigo,
            external_script_path: None,
            whisper_accelerator: WhisperAcceleratorSetting::Auto,
            ort_accelerator: OrtAcceleratorSetting::Auto,
            whisper_gpu_device: default_whisper_gpu_device(),
            extra_recording_buffer_ms: 0,
        }
    }
}

// ── Defaults ──────────────────────────────────────────────────────────────────

fn default_bindings() -> HashMap<String, ShortcutBinding> {
    let mut map = HashMap::new();
    let transcribe = ShortcutBinding {
        id: "transcribe".into(),
        name: "Transcribe".into(),
        description: "Converts your speech into text.".into(),
        default_binding: "ctrl+space".into(),
        current_binding: "ctrl+space".into(),
    };
    let transcribe_pp = ShortcutBinding {
        id: "transcribe_with_post_process".into(),
        name: "Transcribe with Post-Processing".into(),
        description: "Converts your speech into text and applies AI post-processing.".into(),
        default_binding: "ctrl+shift+space".into(),
        current_binding: "ctrl+shift+space".into(),
    };
    let cancel = ShortcutBinding {
        id: "cancel".into(),
        name: "Cancel".into(),
        description: "Cancels the current recording.".into(),
        default_binding: "escape".into(),
        current_binding: "escape".into(),
    };
    map.insert(transcribe.id.clone(), transcribe);
    map.insert(transcribe_pp.id.clone(), transcribe_pp);
    map.insert(cancel.id.clone(), cancel);
    map
}

fn default_audio_feedback() -> bool {
    true
}

fn default_audio_feedback_volume() -> f32 {
    0.5
}

fn default_selected_language() -> String {
    "auto".into()
}

fn default_word_correction_threshold() -> f64 {
    0.8
}

fn default_history_limit() -> usize {
    100
}

fn default_post_process_provider_id() -> String {
    "openai".into()
}

fn default_post_process_providers() -> Vec<PostProcessProvider> {
    vec![
        PostProcessProvider {
            id: "openai".into(),
            label: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".into()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".into(),
            label: "Anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            allow_base_url_edit: false,
            models_endpoint: None,
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "custom".into(),
            label: "Custom".into(),
            base_url: String::new(),
            allow_base_url_edit: true,
            models_endpoint: Some("/models".into()),
            supports_structured_output: false,
        },
    ]
}

fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![LLMPrompt {
        id: "default".into(),
        name: "Default Cleanup".into(),
        prompt: "Clean up the following transcription by fixing grammar, removing filler words, and ensuring proper punctuation. Return only the cleaned text.".into(),
    }]
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_paste_delay_ms() -> u64 {
    200
}

fn default_whisper_gpu_device() -> i32 {
    0
}

// ── Path resolution ───────────────────────────────────────────────────────────

pub fn config_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(CONFIG_DIR).join(CONFIG_FILENAME)
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| config_path_in(&d))
}

// ── Load / Save ───────────────────────────────────────────────────────────────

/// Loads settings from `~/.config/handy/config.toml`.
/// Returns default settings if the file does not exist.
/// Returns an error if the file exists but cannot be parsed.
pub fn load() -> Result<AppSettings, String> {
    load_from(&config_path().ok_or("Could not determine XDG config directory")?)
}

pub fn load_from(path: &Path) -> Result<AppSettings, String> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read config file: {e}"))?;
    toml::from_str(&contents).map_err(|e| format!("Failed to parse config file: {e}"))
}

/// Persists `settings` to disk immediately (no debounce).
pub fn save_to(path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    let contents =
        toml::to_string_pretty(settings).map_err(|e| format!("Failed to serialize config: {e}"))?;
    std::fs::write(path, contents).map_err(|e| format!("Failed to write config file: {e}"))
}

// ── Debounced save task ───────────────────────────────────────────────────────

/// Spawns a Tokio task that writes the latest `AppSettings` to disk after a
/// 500 ms quiet period.  Returns an `Arc<watch::Sender>`; drop all clones to
/// stop the task.
///
/// Multiple rapid calls to `send()` collapse: the watch channel retains only
/// the most-recent value, so only one write occurs per debounce window.
pub fn spawn_debounced_saver(
    path: PathBuf,
    initial: AppSettings,
) -> Arc<watch::Sender<AppSettings>> {
    let (tx, mut rx) = watch::channel(initial);
    let tx = Arc::new(tx);

    tokio::spawn(async move {
        loop {
            // Block until a new value has been sent since we last looked.
            if rx.changed().await.is_err() {
                break; // all senders dropped
            }
            // Mark the current value as seen so that changed() truly blocks
            // on the next iteration (not just returns immediately again).
            rx.borrow_and_update();

            // Debounce: drain rapid bursts within the quiet window.
            loop {
                tokio::select! {
                    biased;
                    result = rx.changed() => {
                        if result.is_err() { return; }
                        rx.borrow_and_update();
                    }
                    _ = sleep(Duration::from_millis(DEBOUNCE_MS)) => break,
                }
            }

            let settings = rx.borrow().clone();
            if let Err(e) = save_to(&path, &settings) {
                tracing::warn!("Config save failed: {e}");
            }
        }
    });

    tx
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn tmp_config(dir: &TempDir) -> PathBuf {
        dir.path().join("handy").join(CONFIG_FILENAME)
    }

    #[test]
    fn missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = tmp_config(&dir);
        let settings = load_from(&path).expect("should return defaults");
        let defaults = AppSettings::default();
        assert_eq!(settings.push_to_talk, defaults.push_to_talk);
        assert_eq!(settings.history_limit, defaults.history_limit);
    }

    #[test]
    fn round_trip_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = tmp_config(&dir);

        let mut original = AppSettings::default();
        original.push_to_talk = true;
        original.history_limit = 42;
        original.selected_language = "es".into();

        save_to(&path, &original).expect("save");
        let loaded = load_from(&path).expect("load");

        assert!(loaded.push_to_talk);
        assert_eq!(loaded.history_limit, 42);
        assert_eq!(loaded.selected_language, "es");
    }

    #[test]
    fn save_creates_intermediate_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join(CONFIG_FILENAME);
        save_to(&nested, &AppSettings::default()).expect("save in nested dirs");
        assert!(nested.exists());
    }

    #[test]
    fn corrupt_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = tmp_config(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"this is not valid toml }{{{").unwrap();
        assert!(load_from(&path).is_err());
    }

    #[test]
    fn default_bindings_have_three_keys() {
        let settings = AppSettings::default();
        assert!(settings.bindings.contains_key("transcribe"));
        assert!(settings
            .bindings
            .contains_key("transcribe_with_post_process"));
        assert!(settings.bindings.contains_key("cancel"));
    }

    #[tokio::test]
    async fn debounced_saver_writes_once_per_burst() {
        let dir = tempfile::tempdir().unwrap();
        let path = tmp_config(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let tx = spawn_debounced_saver(path.clone(), AppSettings::default());

        let mut settings = AppSettings::default();
        for i in 0..20u64 {
            settings.paste_delay_ms = i;
            tx.send(settings.clone()).unwrap();
        }

        // Allow the debounce window to expire and the write to happen.
        tokio::time::sleep(Duration::from_millis(800)).await;

        let loaded = load_from(&path).expect("load after debounce");
        // Only the last value (19) should be persisted.
        assert_eq!(loaded.paste_delay_ms, 19);
    }
}
