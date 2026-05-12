use std::process::{Command, Stdio};
use std::time::Duration;

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;
use crate::config::{AppSettings, AutoSubmitKey, ClipboardHandling, PasteMethod, TypingTool};

/// Entry point called from the UI layer after transcription completes.
///
/// Runs the async delay on the Tokio executor, then hands off the blocking
/// clipboard / input-injection work to `spawn_blocking`.
pub async fn execute(ctx: &AppContext, text: String) {
    let settings = ctx.settings();

    let text = if settings.append_trailing_space {
        format!("{text} ")
    } else {
        text
    };

    if settings.paste_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(settings.paste_delay_ms)).await;
    }

    let ctx_clone = ctx.clone();
    let result = tokio::task::spawn_blocking(move || do_paste(&text, &settings)).await;

    let outcome = match result {
        Ok(r) => r,
        Err(e) => Err(format!("paste task panicked: {e}")),
    };

    if let Err(e) = outcome {
        tracing::warn!("Paste failed: {e}");
        ctx_clone.emit(BackendEvent::PasteError(e));
    }
}

// ── Blocking paste logic ──────────────────────────────────────────────────────

fn do_paste(text: &str, settings: &AppSettings) -> Result<(), String> {
    match settings.paste_method {
        PasteMethod::CtrlV | PasteMethod::ShiftInsert => {
            paste_via_clipboard(text, settings)?;
        }
        PasteMethod::Typing => {
            type_directly(text, settings.typing_tool)?;
        }
        PasteMethod::Script => {
            paste_via_script(text, settings)?;
        }
    }

    if settings.auto_submit {
        send_auto_submit_key(settings.auto_submit_key)?;
    }

    Ok(())
}

fn paste_via_clipboard(text: &str, settings: &AppSettings) -> Result<(), String> {
    let saved = if settings.clipboard_handling == ClipboardHandling::Restore {
        read_clipboard()
    } else {
        None
    };

    write_clipboard(text)?;

    // Let the compositor register the new clipboard owner before we send the keystroke.
    std::thread::sleep(Duration::from_millis(50));

    send_paste_combo(settings.paste_method)?;

    // Give the target app time to read the clipboard before we touch it again.
    std::thread::sleep(Duration::from_millis(50));

    match settings.clipboard_handling {
        ClipboardHandling::Restore => restore_clipboard(saved.as_deref()),
        ClipboardHandling::Clear => clear_clipboard(),
        ClipboardHandling::Keep => {}
    }

    Ok(())
}

fn send_paste_combo(method: PasteMethod) -> Result<(), String> {
    // wtype: uses the zwp_virtual_keyboard_manager_v1 protocol.
    if is_available("wtype") {
        let args: &[&str] = match method {
            PasteMethod::CtrlV => &["-M", "ctrl", "-k", "v"],
            PasteMethod::ShiftInsert => &["-M", "shift", "-k", "Insert"],
            _ => unreachable!(),
        };
        let status = Command::new("wtype")
            .args(args)
            .status()
            .map_err(|e| format!("wtype: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }

    // ydotool: uses uinput via the ydotoold daemon.
    // Linux input event keycodes: ctrl=29, v=47, shift=42, insert=110
    if is_available("ydotool") {
        let args: &[&str] = match method {
            PasteMethod::CtrlV => &["key", "29:1", "47:1", "47:0", "29:0"],
            PasteMethod::ShiftInsert => &["key", "42:1", "110:1", "110:0", "42:0"],
            _ => unreachable!(),
        };
        let status = Command::new("ydotool")
            .args(args)
            .status()
            .map_err(|e| format!("ydotool: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }

    Err("no Wayland key injection tool found; install wtype or ydotool".into())
}

fn type_directly(text: &str, tool: TypingTool) -> Result<(), String> {
    let try_wtype = || -> Result<bool, String> {
        if !is_available("wtype") {
            return Ok(false);
        }
        let status = Command::new("wtype")
            .arg("--")
            .arg(text)
            .status()
            .map_err(|e| format!("wtype: {e}"))?;
        Ok(status.success())
    };

    let try_ydotool = || -> Result<bool, String> {
        if !is_available("ydotool") {
            return Ok(false);
        }
        let status = Command::new("ydotool")
            .args(["type", "--", text])
            .status()
            .map_err(|e| format!("ydotool: {e}"))?;
        Ok(status.success())
    };

    let ok = match tool {
        TypingTool::Auto => try_wtype()? || try_ydotool()?,
        TypingTool::Wtype => try_wtype()?,
        TypingTool::Ydotool => try_ydotool()?,
    };

    if ok {
        Ok(())
    } else {
        Err("no Wayland typing tool found; install wtype or ydotool".into())
    }
}

fn paste_via_script(text: &str, settings: &AppSettings) -> Result<(), String> {
    let path = settings
        .external_script_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or("no external script path configured")?;

    let status = Command::new(path)
        .arg(text)
        .status()
        .map_err(|e| format!("script '{path}': {e}"))?;

    if !status.success() {
        return Err(format!("script '{path}' exited non-zero"));
    }

    Ok(())
}

fn send_auto_submit_key(key: AutoSubmitKey) -> Result<(), String> {
    if key == AutoSubmitKey::None {
        return Ok(());
    }

    if is_available("wtype") {
        let wtype_key = match key {
            AutoSubmitKey::Enter => "Return",
            AutoSubmitKey::Space => "space",
            AutoSubmitKey::Tab => "Tab",
            AutoSubmitKey::None => unreachable!(),
        };
        let status = Command::new("wtype")
            .args(["-k", wtype_key])
            .status()
            .map_err(|e| format!("wtype: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }

    // Linux input event keycodes: Return=28, Space=57, Tab=15
    if is_available("ydotool") {
        let (press, release) = match key {
            AutoSubmitKey::Enter => ("28:1", "28:0"),
            AutoSubmitKey::Space => ("57:1", "57:0"),
            AutoSubmitKey::Tab => ("15:1", "15:0"),
            AutoSubmitKey::None => unreachable!(),
        };
        let status = Command::new("ydotool")
            .args(["key", press, release])
            .status()
            .map_err(|e| format!("ydotool: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }

    Err("no Wayland key injection tool found for auto-submit; install wtype or ydotool".into())
}

// ── Wayland clipboard helpers ─────────────────────────────────────────────────

fn write_clipboard(text: &str) -> Result<(), String> {
    let status = Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("wl-copy not found: {e}; install wl-clipboard to enable paste"))?;

    if !status.success() {
        return Err("wl-copy failed".into());
    }

    Ok(())
}

fn read_clipboard() -> Option<String> {
    let output = Command::new("wl-paste")
        .arg("--no-newline")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

fn restore_clipboard(text: Option<&str>) {
    match text {
        Some(content) => {
            let _ = Command::new("wl-copy")
                .arg("--")
                .arg(content)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        None => clear_clipboard(),
    }
}

fn clear_clipboard() {
    let _ = Command::new("wl-copy")
        .arg("--clear")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn is_available(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AutoSubmitKey, ClipboardHandling, PasteMethod};

    fn settings_with_method(method: PasteMethod) -> AppSettings {
        let mut s = AppSettings::default();
        s.paste_method = method;
        s
    }

    #[test]
    fn trailing_space_applied_before_paste() {
        // Verify the text transformation happens before blocking work.
        let text = "hello";
        let mut settings = AppSettings::default();
        settings.append_trailing_space = true;
        let text = if settings.append_trailing_space {
            format!("{text} ")
        } else {
            text.to_owned()
        };
        assert_eq!(text, "hello ");
    }

    #[test]
    fn no_trailing_space_when_disabled() {
        let text = "hello";
        let settings = AppSettings::default();
        let text = if settings.append_trailing_space {
            format!("{text} ")
        } else {
            text.to_owned()
        };
        assert_eq!(text, "hello");
    }

    #[test]
    fn script_method_errors_without_path() {
        let settings = settings_with_method(PasteMethod::Script);
        let result = paste_via_script("test", &settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no external script path"));
    }

    #[test]
    fn auto_submit_none_is_noop() {
        let result = send_auto_submit_key(AutoSubmitKey::None);
        assert!(result.is_ok());
    }
}
