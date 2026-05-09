# PRD: Handy Linux GTK4 Port

## Problem Statement

Handy is a cross-platform speech-to-text desktop application built on Tauri, which bundles a WebKit webview to render a React/TypeScript frontend. On Linux, this means shipping a webview runtime, a JavaScript engine, and a full web stack for what is functionally a small tray utility. The result is a non-native appearance, webview-specific rendering quirks, and a dependency footprint that is disproportionate to the application's purpose. A Linux user who wants a native, lightweight transcription tool cannot get one from the current codebase without accepting the full Tauri/webview stack.

## Solution

Fork Handy into a Linux-only application that replaces the Tauri framework and React frontend with a native GTK4 UI built using Relm4 and libadwaita, while reusing the existing Rust backend managers (audio recording, VAD, transcription, model management, history, LLM post-processing) without modification to their core logic. The result is a single native binary, installable via `cargo install` or distributed as an AppImage, that behaves identically to Handy from a user perspective while integrating naturally with the Linux desktop.

## User Stories

1. As a Linux user, I want to install Handy via `cargo install` so that I can get a working transcription tool without managing system packages beyond what cargo requires.
2. As a Linux user, I want the application to look and feel native to my GNOME or COSMIC desktop so that it does not visually stand out from other system applications.
3. As a user, I want to trigger recording by pressing a global keyboard shortcut so that I can dictate text into any application without switching focus.
4. As a user, I want to toggle recording on and off with a single shortcut press so that I can dictate long passages without holding a key.
5. As a user, I want push-to-talk mode where recording only continues while I hold the shortcut key so that I can record short phrases precisely.
6. As a user, I want a floating overlay indicator to appear at the top or bottom of my screen while recording, transcribing, or processing so that I always know the state of the application.
7. As a user, I want the overlay to display a real-time audio level visualizer while recording so that I can confirm my microphone is picking up sound.
8. As a user, I want the transcribed text to be automatically pasted into the focused application after I finish speaking so that dictation requires no additional steps.
9. As a user, I want to choose my paste method (Ctrl+V, Shift+Insert, direct typing, or external script) so that I can work around application-specific clipboard handling.
10. As a user, I want transcribed text to be copied to the clipboard regardless of paste method so that I can paste it again if needed.
11. As a user, I want an optional trailing space appended to each transcription so that consecutive dictations do not run words together.
12. As a user, I want to download Whisper models of different sizes (small, medium, turbo, large) so that I can trade off between speed and accuracy based on my hardware.
13. As a user, I want to see download progress with speed and ETA while a model is downloading so that I know how long to wait.
14. As a user, I want to delete downloaded models to reclaim disk space so that I can manage my storage.
15. As a user, I want to switch between downloaded models without restarting the application so that I can change accuracy/speed tradeoffs on the fly.
16. As a user, I want the active model to be unloaded from memory after a configurable idle period so that GPU and RAM are freed when I am not dictating.
17. As a user, I want to manually unload the model from memory so that I can free resources immediately without waiting for the timeout.
18. As a user, I want to see which model is currently loaded in the settings UI so that I know the current state without guessing.
19. As a user, I want a clear prompt in the Models settings page when no model is downloaded so that I know exactly what to do to get started.
20. As a user, I want to select my input microphone from a dropdown so that I can use a specific device rather than the system default.
21. As a user, I want to select my output audio device so that feedback sounds play through the correct speakers or headphones.
22. As a user, I want audio feedback sounds to play when recording starts and stops so that I have confirmation without looking at the screen.
23. As a user, I want to adjust the feedback sound volume independently of system volume so that it does not interfere with other audio.
24. As a user, I want to choose from multiple sound themes or supply custom sound files so that feedback sounds match my preference.
25. As a user, I want to mute system audio while I am recording so that audio from other applications does not contaminate my recording.
26. As a user, I want to keep the microphone stream open briefly between recordings so that re-triggering is nearly instant for rapid successive dictations.
27. As a user, I want a configurable delay before the transcription is pasted so that focus can settle on the target application before the paste occurs.
28. As a user, I want a configurable pre/post recording buffer so that the beginning and end of my speech are not clipped.
29. As a user, I want transcribed text to be optionally sent to an LLM for post-processing so that filler words, punctuation, or formatting are automatically improved.
30. As a user, I want to configure which LLM provider handles post-processing (OpenAI, Anthropic, or a compatible endpoint) so that I can use my existing API access.
31. As a user, I want to enter a custom base URL and API key for post-processing so that I can use self-hosted or third-party LLM endpoints.
32. As a user, I want to fetch the list of available models from my configured LLM provider so that I do not have to type model names manually.
33. As a user, I want to create, edit, and delete custom post-processing prompt templates so that I can tailor the LLM's behavior to different use cases.
34. As a user, I want to select which prompt template is active so that I can switch between tasks (cleanup, formatting, translation) without reconfiguring.
35. As a user, I want to toggle post-processing on or off without losing my configuration so that I can transcribe raw text when I do not need LLM cleanup.
36. As a user, I want a separate shortcut to toggle recording with post-processing so that I can choose per-dictation whether to run LLM cleanup.
37. As a user, I want transcribed text to be optionally translated to English so that I can dictate in my native language and output English text.
38. As a user, I want to select the transcription language explicitly so that Whisper does not have to auto-detect and potentially misidentify my language.
39. As a user, I want to define custom word corrections so that domain-specific terms Whisper consistently mishears are automatically fixed.
40. As a user, I want to configure the fuzzy matching threshold for custom word corrections so that I can tune sensitivity to avoid unwanted substitutions.
41. As a user, I want a scrollable history of past transcriptions so that I can review, copy, or re-use previous dictations.
42. As a user, I want to play back the audio recording associated with a history entry so that I can verify what was said versus what was transcribed.
43. As a user, I want playback to show a progress indicator so that I can see how much of the recording remains.
44. As a user, I want to mark history entries as saved so that I can distinguish important transcriptions from throwaway ones.
45. As a user, I want to delete individual history entries so that I can remove sensitive or unwanted transcriptions.
46. As a user, I want to configure the maximum number of history entries retained so that the database does not grow unboundedly.
47. As a user, I want to configure how long audio recordings are kept (never, 3 days, 2 weeks, 3 months, or matched to history limit) so that I can manage disk usage.
48. As a user, I want to re-transcribe an existing history entry with the current model so that I can improve old transcriptions after switching to a better model.
49. As a user, I want the application to live in the system tray so that it runs in the background without occupying taskbar space.
50. As a user, I want to open the settings window by clicking the tray icon so that configuration is one click away.
51. As a user, I want the settings window to close without quitting the application so that the app continues running in the tray.
52. As a user, I want the application to start automatically on login so that I do not have to manually launch it each session.
53. As a user, I want to configure the application to start hidden (tray only, no settings window) so that it does not interrupt my workflow at login.
54. As a user, I want to cancel an in-progress recording or transcription via a dedicated shortcut so that I can abort without waiting for completion.
55. As a user, I want to control the application from the command line (`--toggle-transcription`, `--toggle-post-process`, `--cancel`) so that I can bind these actions to my window manager or script them.
56. As a user, I want launching a second instance to bring the settings window to the front rather than opening a duplicate so that the application behaves predictably.
57. As a user, I want to select GPU acceleration for Whisper (auto, CPU, Vulkan GPU) so that I can use my graphics card for faster transcription.
58. As a user, I want to select GPU acceleration for ONNX-based models (auto, CPU, CUDA, ROCm) so that I can optimize for my hardware.
59. As a user, I want to select which GPU device is used when multiple are present so that transcription runs on the correct card.
60. As a user, I want to configure an auto-submit key (Enter, Space, Tab) that is pressed after pasting so that dictation can submit forms or chat messages automatically.
61. As a user, I want to receive a toast notification when a paste error occurs so that I know the transcription was not delivered and can take action.
62. As a user, I want to open the log directory and application data directory from the settings UI so that I can inspect or report issues without navigating the filesystem manually.
63. As a user, I want to set the log level from the settings UI so that I can enable verbose logging for debugging without restarting with CLI flags.
64. As a user, I want debug mode to expose additional diagnostic information in the settings so that I can investigate unexpected behavior.

## Implementation Decisions

### Module Breakdown

The implementation is organized into three layers: new infrastructure modules that replace Tauri primitives, modified backend modules with Tauri coupling removed, and new UI modules replacing the React frontend.

#### New Infrastructure Modules

**`app_context`** — Deep module. Replaces `AppHandle` throughout the codebase. Encapsulates `Arc<RwLock<AppSettings>>` for shared settings access and a `relm4::Sender<BackendEvent>` for backend-to-UI event delivery. All backend managers and action handlers accept `AppContext` by value or reference instead of `AppHandle`. Interface: `settings() -> AppSettings` (takes a read lock and clones), `update_settings(f: impl FnOnce(&mut AppSettings))` (takes a write lock and schedules a debounced persistence write), `emit(event: BackendEvent)` (non-blocking send to the UI channel).

**`backend_event`** — Deep module. A single `BackendEvent` enum that replaces all Tauri event emissions. Variants mirror the current Tauri event set: overlay state changes, mic level readings, model state transitions, model download progress, history updates, paste errors, recording errors, and update check triggers. Pure data — no logic.

**`config`** — Deep module. TOML-based settings persistence replacing `tauri-plugin-store`. Responsibilities: resolve the settings file path via `dirs::config_dir()` (to `~/.config/handy/config.toml`), deserialize on load with `serde`/`toml`, serialize on save, enforce a 500ms debounce on writes using a background Tokio task so that rapid setting changes (e.g. dragging a volume slider) do not hammer disk. Interface: `load() -> Result<AppSettings>`, `save(settings: AppSettings)` (debounced, non-blocking).

**`ipc`** — Deep module. D-Bus service and client via `zbus`. The service, registered under a well-known name (e.g. `computer.handy.Handy`), exposes four typed methods on a single interface: `Ping() -> bool` (presence detection), `ToggleTranscription()`, `TogglePostProcess()`, `Cancel()`. At startup the application first calls `Ping()` via the client stub; if a response arrives the second instance forwards its CLI flags via the appropriate method and exits. If no response arrives the process registers the service and starts normally. Method implementations send the corresponding `BackendEvent` through the `AppContext` sender.

**`autostart`** — Deep module. Manages the XDG autostart `.desktop` file at `~/.config/autostart/handy.desktop`. Interface: `enable(exec_path: &Path)` writes a conforming `.desktop` file with `Exec=<path> --start-hidden`, `disable()` removes it, `is_enabled() -> bool` checks for the file's existence. No D-Bus or system service involvement.

**`runtime`** — Shallow module. Owns the background Tokio `Runtime`. Initializes the runtime on a dedicated `std::thread`, runs manager startup (audio, model, transcription, history) within it, and holds join handles for clean shutdown. Exposes a `shutdown()` method that signals managers to stop and joins the thread.

**`tray`** — Shallow module. `ksni`-based StatusNotifierItem tray icon. Menu items map to `ipc` method calls (or direct `AppContext` emits within the same process): show settings window, toggle recording, quit. No system C library dependencies.

#### Modified Backend Modules

**`managers/audio`, `managers/transcription`, `managers/history`, `managers/model`** — The four manager structs are modified to accept `AppContext` in place of `AppHandle` at construction. Internal calls to `get_settings(&app_handle)` become `ctx.settings()`. Internal calls to `app_handle.emit(event, payload)` become `ctx.emit(BackendEvent::...)`. All macOS, Windows, and DirectML conditional compilation blocks are removed. The mock `TranscriptionManager` in `transcription_mock.rs` is updated in parallel to keep CI passing.

**`settings`** — The `AppSettings` struct, its enums, and default functions are preserved. All function signatures that accepted `&AppHandle` are replaced with direct `AppSettings` values or `Arc<RwLock<AppSettings>>`. The `LogLevel` conversion to `tauri_plugin_log::LogLevel` is replaced with a conversion to `tracing::Level`. The `KeyboardImplementation` enum is removed; `handy_keys` is the only backend. The `DirectMl` variant of `OrtAcceleratorSetting` is removed.

**`audio_feedback`** — The bundled default sounds (start, stop, cancel) are embedded at compile time via `include_bytes!`. At runtime, `rodio` decodes them from memory using `Cursor<&[u8]>`. Custom user sounds are loaded from `dirs::data_dir()/handy/sounds/`. The Tauri path resolution (`BaseDirectory::AppData`, `BaseDirectory::Resource`) is replaced entirely.

**`clipboard`** — The `tauri_plugin_clipboard_manager::ClipboardExt` dependency is replaced with direct use of the `arboard` crate. An `arboard::Clipboard` instance is held behind `Arc<Mutex<arboard::Clipboard>>` in `AppContext` (alongside `Arc<Mutex<enigo::Enigo>>`). The `enigo` initialization no longer requires a macOS permission gate; it is initialized unconditionally at startup.

**`actions`** — The `AppHandle` parameter on action trait methods (`start`, `stop`) is replaced with `AppContext`. `tauri::async_runtime::spawn` calls are replaced with `tokio::spawn` (executed on the background Tokio runtime). The `FinishGuard` RAII type is updated to hold `AppContext`.

**`shortcut`** — The `tauri_impl` sub-module and the `KeyboardImplementation` setting are removed. The module retains only the `handy_keys` backend. The `change_keyboard_implementation_setting` command and its UI toggle are dropped.

#### New UI Modules

**`ui::app`** — Root Relm4 `Component`. Holds the `AppContext`, the overlay child component controller, and a reference to the settings window. In `init()`, spawns the `BackendEvent` bridge: `sender.command(|cmd_tx, shutdown| async move { loop { tokio::select! { _ = shutdown.recv() => break, Some(event) = backend_rx.recv() => cmd_tx.emit(event) } } })`. `update_cmd()` routes `BackendEvent` variants to the overlay sender, settings window sender, or handles directly (e.g. showing toasts, toggling window visibility).

**`ui::settings_window`** — An `AdwPreferencesWindow` wrapped in an `AdwToastOverlay`. Contains one `AdwPreferencesPage` per settings section, each backed by a Relm4 child component. Receives toast trigger messages from the root component for paste errors and recording errors.

**`ui::pages::general`** — Autostart toggle, start-hidden toggle, show-tray-icon toggle, audio feedback toggle, sound theme picker, volume slider, output device selector, paste delay, auto-submit key selector, clipboard handling, append trailing space, mute-while-recording toggle, lazy stream close toggle.

**`ui::pages::microphone`** — Input device dropdown, PTT/toggle mode switch, always-on microphone toggle, extra recording buffer slider.

**`ui::pages::models`** — `FactoryVecDeque<ModelEntry>` listing all known models with per-row download, delete, and select actions. An `AdwBanner` is shown when no model is present. Model state and download progress arrive via `BackendEvent` variants routed from the root component.

**`ui::pages::history`** — `FactoryVecDeque<HistoryEntry>` showing transcription history with per-row save toggle, delete, replay, and re-transcribe actions. Pagination is triggered by scrolling to the bottom. History mutation events (`BackendEvent::HistoryUpdate`) are routed here by the root component.

**`ui::pages::post_processing`** — LLM provider selector, base URL input, API key input, model fetch button and dropdown, custom prompt list with add/edit/delete, active prompt selector, post-processing enable/disable toggle.

**`ui::pages::advanced`** — Whisper accelerator selector, ORT accelerator selector, GPU device selector, paste method selector, typing tool selector, external script path input, word correction threshold slider, custom word list editor, model unload timeout selector, log level selector, debug mode toggle, open-log-dir and open-app-data-dir buttons.

**`ui::overlay`** — A Relm4 child component whose root widget is a `gtk::Window` with `gtk4-layer-shell` applied. Anchored to the top or bottom screen edge (per `OverlayPosition` setting), zero-width margins on the sides, keyboard-passthrough enabled. Internal state: current status (`Recording`, `Transcribing`, `Processing`), mic level buffer (`[f32; 16]`). A `gtk::DrawingArea` renders a Cairo bar chart from the level buffer; `queue_draw()` is called on each `BackendEvent::MicLevel`. A status label below the bars shows the current state string. The window is shown/hidden on `BackendEvent::ShowOverlay` / `BackendEvent::HideOverlay`.

**`ui::widgets::shortcut_recorder`** — A `gtk::Button` subclass (or a plain button with attached controller). On click, enters capture mode: label changes to a placeholder, an `EventControllerKey` is attached, and the next key-press event (with modifiers) is translated into the `handy-keys` binding string format and emitted as a widget output signal. Pressing Escape cancels capture and restores the previous binding label.

**`ui::factory::history_entry`** — Relm4 factory component for history list rows. State: `HistoryEntry` data, `Option<rodio::Sink>` for active playback, playback position. A `glib::timeout_add_local` at 100ms interval updates a `gtk::Scale` progress indicator while a sink is playing. Row actions: play/pause, delete, save toggle, re-transcribe.

**`ui::factory::model_entry`** — Relm4 factory component for model list rows. Displays model name, size, and status. Shows a `gtk::ProgressBar` during download (driven by `BackendEvent::ModelDownloadProgress`). Row actions: download, cancel download, delete, select.

### Architectural Decisions

**Single process, two runtimes.** GTK4 (via Relm4) runs on the glib main loop on the main thread. All backend managers run on a dedicated `tokio::runtime::Runtime` on a background thread. The two runtimes communicate exclusively via a `relm4::Sender<BackendEvent>` (backend → UI) and `tokio::sync::mpsc` or direct function calls through manager `Arc` references (UI → backend). No Tokio future is awaited on the glib executor; no glib future is awaited on Tokio.

**`AppContext` as the single seam.** The boundary between Tauri and backend logic is entirely expressed by `AppContext`. Replacing `AppHandle` with `AppContext` is the primary mechanical change in all backend modules. No other Tauri types cross the boundary.

**Settings persistence: debounced TOML.** Settings are kept in-memory as `Arc<RwLock<AppSettings>>`. Writes schedule a 500ms debounced Tokio task that serializes and writes `~/.config/handy/config.toml`. Reads take a short-lived read lock and clone. No GSettings/dconf schema is required.

**D-Bus as the single-instance and IPC mechanism.** The well-known D-Bus name also serves as the presence check. The client stub attempts `Ping()` before registering; success means a primary instance is running and the second instance dispatches its CLI command and exits. Failure means no primary instance; the process registers the name and starts normally.

**Wayland-only overlay.** The recording overlay is positioned exclusively via `gtk4-layer-shell`. No X11 override-redirect fallback is implemented. The `HANDY_NO_GTK_LAYER_SHELL` environment variable override from the original codebase is dropped.

**Embedded default sounds.** Default audio feedback files are embedded via `include_bytes!` and decoded at runtime from a `std::io::Cursor`. Custom user sounds are read from `dirs::data_dir()/handy/sounds/`. This enables `cargo install` with no post-install asset step.

**Autostart via XDG `.desktop`.** The `~/.config/autostart/handy.desktop` file is written with `Exec=<current_exe_path> --start-hidden`. Enable/disable is a file create/delete operation.

**Logging via `tracing` + `tracing-appender`.** `tracing-subscriber` replaces `tauri-plugin-log`. Log files are rotated daily to `dirs::data_dir()/handy/logs/`. The log level setting maps to `tracing::Level`.

**Platform code removal.** All `#[cfg(target_os = "macos")]`, `#[cfg(target_os = "windows")]`, Apple Intelligence, clamshell microphone, accessibility permissions, Windows registry microphone status, DirectML acceleration, and `tauri-nspanel` code is removed. The `OrtAcceleratorSetting::DirectMl` variant is deleted. The `KeyboardImplementation` enum and its associated settings command and UI toggle are deleted.

**UI i18n removed.** All i18next infrastructure is deleted. User-visible strings are hardcoded in English in the Rust source. Transcription language selection (a Whisper parameter, not a UI concern) is retained.

## Testing Decisions

### What makes a good test

Tests should verify observable behavior through the module's public interface, not implementation details. A test should remain valid if the internal implementation is refactored. Tests should not mock internal collaborators — only external I/O boundaries (filesystem, D-Bus, network). A test that breaks when a private function is renamed is a bad test.

### Modules to test

**`config`** — Test that `load()` on a missing file returns default settings. Test that `save()` followed by `load()` round-trips `AppSettings` exactly. Test that rapid successive `save()` calls result in exactly one file write (debounce behavior) using a temporary directory. Test that a corrupt TOML file returns an error rather than panicking. Prior art: the `transcription_mock.rs` pattern in the repo demonstrates that pure-Rust manager behavior can be tested by substituting implementations; apply the same discipline here with `tempfile::tempdir()`.

**`app_context`** — Test that `emit()` delivers events to the receiver in order. Test that `settings()` returns a consistent snapshot under concurrent `update_settings()` calls. Test that `update_settings()` schedules exactly one debounced write per burst of rapid calls. No GTK dependency required.

**`ipc`** — Test that a second instance calling `Ping()` on a registered service receives a response. Test that `ToggleTranscription()`, `TogglePostProcess()`, and `Cancel()` each deliver the expected `BackendEvent` variant to the `AppContext` sender. Use `zbus`'s in-process test transport to avoid requiring a live D-Bus session.

**`autostart`** — Test that `enable(path)` creates a valid `.desktop` file at the expected path with the correct `Exec` line. Test that `disable()` removes it. Test that `is_enabled()` returns the correct state before and after each operation. Use `tempfile::tempdir()` to redirect the XDG config path.

**`managers/history`** — Test that `add_entry()` followed by `get_history_entries()` returns the entry. Test that `delete_history_entry()` removes it. Test that `toggle_history_entry_saved()` flips the saved flag. Test that `update_history_limit()` prunes older entries beyond the limit. Test the retention period cleanup logic. Prior art: `transcription_mock.rs` shows that the CI build already substitutes manager implementations to avoid hardware dependencies — history tests can run against an in-memory SQLite database (`rusqlite` supports `:memory:` connections) with no special CI configuration.

### Modules not tested

UI modules (`ui::*`) are GTK-dependent and require a running display server. They are not unit-tested. The audio, model, and transcription managers depend on hardware (microphone, GPU) or large binary model files; they use the existing mock substitution strategy for CI and are not unit-tested in this effort. The `tray` and `runtime` modules are thin wrappers and are not unit-tested.

## Out of Scope

- **macOS support.** All macOS-specific code is removed. The fork is Linux-only.
- **Windows support.** All Windows-specific code is removed.
- **Cross-platform parity.** Making the GTK port work on macOS or Windows is an explicit non-goal.
- **UI internationalization.** The React i18next infrastructure and all locale files are removed. The UI is English-only.
- **In-app auto-update.** The `tauri-plugin-updater` is removed. Users update via `cargo install` or by replacing their AppImage.
- **Apple Intelligence integration.** Removed entirely.
- **Clamshell microphone mode.** Removed (macOS laptop-specific feature).
- **Windows microphone privacy settings.** Removed.
- **DirectML GPU acceleration.** Removed (Windows-only).
- **Metal GPU acceleration.** Removed (macOS-only).
- **X11 overlay support.** The overlay uses Wayland layer shell exclusively; X11 is not supported.
- **GTK theme customization.** The application uses libadwaita defaults; custom theming is not implemented.
- **COSMIC-native UI.** The application uses GTK4/libadwaita, not `libcosmic`/iced, despite running on COSMIC desktop.
- **Flatpak packaging.** The target distribution format is `cargo install` or AppImage; Flatpak sandboxing is not addressed.
- **New features.** This port targets feature parity with upstream Handy's Linux behavior only.

## Further Notes

**Upstream divergence.** This is a personal fork. Cherry-picking bug fixes from upstream Handy is straightforward as long as the fix touches backend managers (which are structurally preserved) rather than Tauri commands or the React frontend. Upstream UI features will not be cherry-picked and must be re-implemented in Relm4 if desired.

**`gtk4-layer-shell` system dependency.** The overlay requires `libgtk4-layer-shell` to be installed (`pacman -S gtk4-layer-shell` on Arch). This is the only system library dependency beyond GTK4 itself that is not satisfied by `cargo install` alone. An AppImage distribution should bundle the library.

**`ksni` and StatusNotifierItem.** COSMIC's panel implements the StatusNotifierItem spec natively. `ksni` is a pure-Rust implementation of the SNI D-Bus protocol; it shares the same D-Bus session as the `ipc` module's `zbus` service. Both can coexist on the same connection.

**Relm4 version.** The implementation targets Relm4 0.9.x (current stable). The `relm4-components` crate provides tested implementations of common patterns (open dialogs, alert dialogs) that should be preferred over custom implementations where applicable.

**No Tauri dependency remains.** After the port, the `tauri`, `tauri-runtime`, `tauri-runtime-wry`, `tauri-utils`, and all `tauri-plugin-*` crates are removed from `Cargo.toml`. The `specta` and `specta-typescript` crates (used for TypeScript binding generation) are also removed. The `src/` directory (React frontend) and all JavaScript tooling (`package.json`, `bun.lockb`, `vite.config.ts`, `tsconfig.json`, `eslint.config.js`) are deleted.
