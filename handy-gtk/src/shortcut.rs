use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;
use crate::config::AppSettings;

// ── Manager commands ──────────────────────────────────────────────────────────

enum ManagerCommand {
    Register {
        binding_id: String,
        hotkey_string: String,
        response: Sender<Result<(), String>>,
    },
    Unregister {
        binding_id: String,
        response: Sender<Result<(), String>>,
    },
    Shutdown,
}

// ── ShortcutManager ───────────────────────────────────────────────────────────

/// Owns the `HotkeyManager` on a dedicated thread and dispatches shortcut
/// events as `BackendEvent`s through the given `AppContext`.
///
/// The manager thread polls for hotkey events at 10 ms intervals, which is
/// sufficient for interactive use while keeping CPU usage negligible.
pub struct ShortcutManager {
    command_tx: Sender<ManagerCommand>,
    thread: Option<JoinHandle<()>>,
}

impl ShortcutManager {
    /// Spawns the manager thread and registers all configured shortcuts from
    /// `settings`.  `ctx` is cloned into the thread for event dispatch.
    pub fn start(ctx: AppContext, settings: &AppSettings) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ManagerCommand>();

        let bindings_to_register: Vec<(String, String)> = settings
            .bindings
            .values()
            .filter(|b| b.id != "cancel") // cancel registered dynamically on recording start
            .filter(|b| b.id != "transcribe_with_post_process" || settings.post_process_enabled)
            .map(|b| (b.id.clone(), b.current_binding.clone()))
            .collect();

        let push_to_talk = settings.push_to_talk;

        let thread = thread::Builder::new()
            .name("handy-shortcut".into())
            .spawn(move || {
                manager_thread(cmd_rx, ctx, bindings_to_register, push_to_talk);
            })
            .map_err(|e| format!("Failed to spawn shortcut thread: {e}"))?;

        Ok(Self {
            command_tx: cmd_tx,
            thread: Some(thread),
        })
    }

    /// Registers or re-registers a single binding.  Blocks until the manager
    /// thread acknowledges the command.
    pub fn register(&self, binding_id: &str, hotkey_string: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(ManagerCommand::Register {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                response: tx,
            })
            .map_err(|_| "Shortcut manager thread is gone")?;
        rx.recv().map_err(|_| "No response from shortcut manager")?
    }

    /// Unregisters a single binding.
    pub fn unregister(&self, binding_id: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(ManagerCommand::Unregister {
                binding_id: binding_id.to_string(),
                response: tx,
            })
            .map_err(|_| "Shortcut manager thread is gone")?;
        rx.recv().map_err(|_| "No response from shortcut manager")?
    }
}

impl Drop for ShortcutManager {
    fn drop(&mut self) {
        let _ = self.command_tx.send(ManagerCommand::Shutdown);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

// ── Manager thread ────────────────────────────────────────────────────────────

fn manager_thread(
    cmd_rx: Receiver<ManagerCommand>,
    ctx: AppContext,
    initial_bindings: Vec<(String, String)>,
    push_to_talk: bool,
) {
    tracing::info!("Shortcut manager thread started");

    let manager = match HotkeyManager::new_with_blocking() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create HotkeyManager: {e}");
            return;
        }
    };

    let mut binding_to_id: HashMap<String, HotkeyId> = HashMap::new();
    let mut id_to_binding: HashMap<HotkeyId, String> = HashMap::new();

    // Register initial shortcuts.
    for (binding_id, hotkey_string) in initial_bindings {
        if let Err(e) = do_register(
            &manager,
            &mut binding_to_id,
            &mut id_to_binding,
            &binding_id,
            &hotkey_string,
        ) {
            tracing::warn!("Failed to register shortcut '{binding_id}': {e}");
        }
    }

    loop {
        // Dispatch any pending hotkey events.
        while let Some(event) = manager.try_recv() {
            if let Some(binding_id) = id_to_binding.get(&event.id) {
                let is_pressed = event.state == HotkeyState::Pressed;
                dispatch_shortcut(&ctx, binding_id, is_pressed, push_to_talk);
            }
        }

        // Process manager commands (10 ms timeout keeps the event loop responsive).
        match cmd_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(ManagerCommand::Register {
                binding_id,
                hotkey_string,
                response,
            }) => {
                let result = do_register(
                    &manager,
                    &mut binding_to_id,
                    &mut id_to_binding,
                    &binding_id,
                    &hotkey_string,
                );
                let _ = response.send(result);
            }
            Ok(ManagerCommand::Unregister {
                binding_id,
                response,
            }) => {
                let result =
                    do_unregister(&manager, &mut binding_to_id, &mut id_to_binding, &binding_id);
                let _ = response.send(result);
            }
            Ok(ManagerCommand::Shutdown) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    tracing::info!("Shortcut manager thread stopped");
}

// ── Shortcut dispatch ─────────────────────────────────────────────────────────

fn dispatch_shortcut(ctx: &AppContext, binding_id: &str, is_pressed: bool, push_to_talk: bool) {
    tracing::debug!("Shortcut: binding={binding_id} pressed={is_pressed}");

    match binding_id {
        "transcribe" => {
            if push_to_talk {
                if is_pressed {
                    ctx.emit(BackendEvent::RecordingStarted);
                } else {
                    ctx.emit(BackendEvent::RecordingStopped);
                }
            } else if is_pressed {
                // Toggle mode: the coordinator decides whether to start or stop.
                ctx.emit(BackendEvent::RecordingStarted);
            }
        }
        "transcribe_with_post_process" => {
            if push_to_talk {
                if is_pressed {
                    ctx.emit(BackendEvent::PostProcessingStarted);
                } else {
                    ctx.emit(BackendEvent::RecordingStopped);
                }
            } else if is_pressed {
                ctx.emit(BackendEvent::PostProcessingStarted);
            }
        }
        "cancel" => {
            if is_pressed {
                ctx.emit(BackendEvent::RecordingStopped);
            }
        }
        other => {
            tracing::warn!("Unknown shortcut binding: {other}");
        }
    }
}

// ── Register / unregister helpers ─────────────────────────────────────────────

fn do_register(
    manager: &HotkeyManager,
    binding_to_id: &mut HashMap<String, HotkeyId>,
    id_to_binding: &mut HashMap<HotkeyId, String>,
    binding_id: &str,
    hotkey_string: &str,
) -> Result<(), String> {
    let hotkey: Hotkey = hotkey_string
        .parse()
        .map_err(|e| format!("Could not parse hotkey '{hotkey_string}': {e}"))?;

    let id = manager
        .register(hotkey)
        .map_err(|e| format!("Could not register hotkey: {e}"))?;

    binding_to_id.insert(binding_id.to_string(), id);
    id_to_binding.insert(id, binding_id.to_string());

    tracing::debug!("Registered shortcut: {binding_id} = {hotkey_string}");
    Ok(())
}

fn do_unregister(
    manager: &HotkeyManager,
    binding_to_id: &mut HashMap<String, HotkeyId>,
    id_to_binding: &mut HashMap<HotkeyId, String>,
    binding_id: &str,
) -> Result<(), String> {
    if let Some(id) = binding_to_id.remove(binding_id) {
        manager
            .unregister(id)
            .map_err(|e| format!("Could not unregister hotkey: {e}"))?;
        id_to_binding.remove(&id);
        tracing::debug!("Unregistered shortcut: {binding_id}");
    }
    Ok(())
}
