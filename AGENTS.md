# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

This is a Linux-only GTK4 port of [Handy](https://github.com/cjpais/Handy). The GTK port lives in `handy-gtk/`. The original Tauri/React app remains in `src/` and `src-tauri/` but is not the target of active development here.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- `libgtk-4-dev`, `libadwaita-1-dev`, `libgtk4-layer-shell-dev` (or distro equivalents)
- On Arch: `pacman -S gtk4 libadwaita gtk4-layer-shell`

**Core Development (GTK port):**

```bash
cd handy-gtk

# Build
cargo build

# Run
cargo run

# Run with verbose logging
cargo run -- --debug
```

**Linting and Formatting (run before committing):**

```bash
cd handy-gtk
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Architecture Overview

The GTK port is a single native binary that replaces Tauri and the React frontend with a GTK4/libadwaita UI built on Relm4, while sharing the same backend logic (audio, VAD, transcription, history).

### Source Structure (`handy-gtk/src/`)

**Infrastructure modules:**

- `main.rs` — Entry point: Tokio runtime, single-instance check, manager startup, GTK main loop
- `app_context.rs` — `AppContext` replaces `AppHandle`; holds shared settings and a `BackendEvent` sender
- `backend_event.rs` — `BackendEvent` enum: all backend-to-UI event variants
- `config.rs` — TOML settings persistence (`~/.config/handy/config.toml`), debounced writes
- `ipc.rs` — D-Bus service and client via `zbus`; single-instance detection and remote control
- `autostart.rs` — XDG autostart `.desktop` file management (`~/.config/autostart/handy.desktop`)
- `shortcut.rs` — Global keyboard shortcut listener via `handy-keys`
- `tray.rs` — StatusNotifierItem system tray icon via `ksni`
- `cli.rs` — CLI argument definitions via `clap`

**UI modules (`ui/`):**

- `ui/app.rs` — Root Relm4 component; bridges Tokio `BackendEvent` channel to GTK; routes events to child components
- `ui/overlay.rs` — Floating recording indicator via `gtk4-layer-shell`; mic level visualizer
- `ui/settings_window.rs` — `adw::Window` settings shell; hides on close rather than quitting

### Key Architecture Patterns

**Single process, two runtimes.** GTK4 (Relm4) owns the main thread. All backend work runs on a dedicated `tokio::runtime::Runtime` on a background thread. Communication: `relm4::Sender<BackendEvent>` (backend → UI); direct `Arc` manager calls (UI → backend).

**`AppContext` as the boundary seam.** All backend modules accept `AppContext` instead of `AppHandle`. Replacing Tauri is entirely expressed by this substitution.

**Settings persistence: debounced TOML.** In-memory `Arc<RwLock<AppSettings>>`, written to `~/.config/handy/config.toml` with a 500 ms debounce.

**D-Bus single-instance and IPC.** On startup, the app attempts `Ping()` on `computer.handy.Handy`. If a primary instance responds, the second instance forwards its CLI flags and exits. Otherwise it registers the name and starts normally.

**Wayland-only overlay.** The recording overlay uses `gtk4-layer-shell` exclusively. No X11 fallback.

### Technology Stack

- `relm4` + `libadwaita` — Native GTK4/libadwaita UI
- `gtk4-layer-shell` — Wayland overlay positioning
- `ksni` — StatusNotifierItem tray icon (pure Rust)
- `zbus` — D-Bus IPC for single-instance and remote control
- `handy-keys` — Global keyboard shortcuts
- `tokio` — Async runtime for backend managers
- `tracing` — Structured logging

### Application Flow

1. **Startup:** Parse CLI args, check D-Bus for existing instance, load settings, build `AppContext`
2. **Services:** Register D-Bus IPC service, start shortcut listener, spawn tray icon
3. **GTK loop:** `RelmApp::new().run::<App>()` — blocks until quit
4. **Recording:** Global shortcut → `BackendEvent` → managers → transcription → paste

### Settings System

Settings are stored in `~/.config/handy/config.toml` as TOML. Writes are debounced 500 ms to avoid hammering disk on rapid slider changes. The `AppSettings` struct is the single source of truth; all modules read a cloned snapshot via `ctx.settings()`.

### Single Instance Architecture

The well-known D-Bus name `computer.handy.Handy` doubles as the presence check. A second instance calls `Ping()` — success means forward CLI flags via the appropriate method and exit; failure means become the primary instance.

## Code Style

- Run `cargo fmt` and `cargo clippy -- -D warnings` before committing
- No `unwrap()` in production paths — propagate errors explicitly
- No comments unless the WHY is non-obvious (hidden constraint, subtle invariant, workaround)
- No Tauri types (`AppHandle`, `tauri::*`) in the GTK crate
- User-facing strings are hardcoded English — i18n infrastructure is removed

## Commit Guidelines

Use conventional commits with a `gtk-port` scope: `feat(gtk-port):`, `fix(gtk-port):`, `refactor(gtk-port):`, `chore(gtk-port):`

## CLI Parameters

| Flag                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on a running instance (via D-Bus)         |
| `--toggle-post-process`  | Toggle recording with post-processing (via D-Bus)          |
| `--cancel`               | Cancel in-progress operation on a running instance (D-Bus) |
| `--start-hidden`         | Launch with tray only — no settings window shown           |
| `--debug`                | Enable verbose (Trace) logging                             |

Remote control flags work by launching a second instance that sends the command via D-Bus and exits immediately.

## Platform Notes

This port is **Linux-only**. macOS and Windows code has been removed.

- Overlay requires a Wayland compositor with the `wlr-layer-shell` protocol
- System tray requires a StatusNotifierItem-compatible panel (KDE Plasma, GNOME with AppIndicator extension, etc.)
- Global shortcuts require `/dev/input` access — add your user to the `input` group if shortcuts are not working

## Testing

Tests live in each module alongside the source. The PRD (`docs/prd-gtk-port.md`) identifies which modules are unit-tested:

- `config`, `app_context`, `ipc`, `autostart` — pure logic, tested with `tempfile` and in-process zbus transport
- `managers/history` — tested against in-memory SQLite
- UI modules and hardware-dependent managers — not unit-tested

```bash
cd handy-gtk && cargo test
```
