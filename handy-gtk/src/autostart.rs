use std::fs;
use std::path::{Path, PathBuf};

const DESKTOP_FILENAME: &str = "handy.desktop";
const DESKTOP_TEMPLATE: &str = "[Desktop Entry]\nType=Application\nName=Handy\nComment=Speech-to-text transcription\nExec={exec} --start-hidden\nTerminal=false\nCategories=Utility;\n";

fn desktop_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join("autostart").join(DESKTOP_FILENAME)
}

fn enable_in(config_dir: &Path, exec_path: &Path) -> Result<(), String> {
    let autostart_dir = config_dir.join("autostart");
    fs::create_dir_all(&autostart_dir)
        .map_err(|e| format!("Failed to create autostart dir: {e}"))?;

    let exec = exec_path
        .to_str()
        .ok_or("Executable path contains non-UTF-8 characters")?;
    let content = DESKTOP_TEMPLATE.replace("{exec}", exec);

    fs::write(autostart_dir.join(DESKTOP_FILENAME), content)
        .map_err(|e| format!("Failed to write autostart file: {e}"))
}

fn disable_in(config_dir: &Path) -> Result<(), String> {
    let path = desktop_path_in(config_dir);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove autostart file: {e}"))?;
    }
    Ok(())
}

fn is_enabled_in(config_dir: &Path) -> bool {
    desktop_path_in(config_dir).exists()
}

/// Writes the XDG autostart `.desktop` file so the app launches at login.
/// The file uses `--start-hidden` so the settings window is not shown on startup.
pub fn enable(exec_path: &Path) -> Result<(), String> {
    let config = dirs::config_dir().ok_or("Could not determine XDG config dir")?;
    enable_in(&config, exec_path)
}

/// Removes the XDG autostart `.desktop` file if it exists.
pub fn disable() -> Result<(), String> {
    let config = dirs::config_dir().ok_or("Could not determine XDG config dir")?;
    disable_in(&config)
}

/// Returns `true` if the autostart `.desktop` file is present.
pub fn is_enabled() -> bool {
    dirs::config_dir()
        .map(|c| is_enabled_in(&c))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp_config() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn disabled_by_default() {
        let tmp = tmp_config();
        assert!(!is_enabled_in(tmp.path()));
    }

    #[test]
    fn enable_creates_desktop_file() {
        let tmp = tmp_config();
        let exec = Path::new("/usr/bin/handy");

        enable_in(tmp.path(), exec).expect("enable");

        let expected = tmp.path().join("autostart").join(DESKTOP_FILENAME);
        assert!(expected.exists(), "desktop file should exist after enable");
    }

    #[test]
    fn enabled_after_enable() {
        let tmp = tmp_config();
        enable_in(tmp.path(), Path::new("/usr/bin/handy")).expect("enable");
        assert!(is_enabled_in(tmp.path()));
    }

    #[test]
    fn desktop_file_contains_exec_and_start_hidden() {
        let tmp = tmp_config();
        let exec = Path::new("/home/user/.cargo/bin/handy");

        enable_in(tmp.path(), exec).expect("enable");

        let path = tmp.path().join("autostart").join(DESKTOP_FILENAME);
        let content = fs::read_to_string(path).expect("read desktop file");

        assert!(
            content.contains("Exec=/home/user/.cargo/bin/handy --start-hidden"),
            "Exec line missing or wrong: {content}"
        );
        assert!(content.contains("Type=Application"), "missing Type");
        assert!(content.contains("Name=Handy"), "missing Name");
        assert!(content.contains("Terminal=false"), "missing Terminal");
    }

    #[test]
    fn disable_removes_file() {
        let tmp = tmp_config();
        enable_in(tmp.path(), Path::new("/usr/bin/handy")).expect("enable");
        assert!(is_enabled_in(tmp.path()));

        disable_in(tmp.path()).expect("disable");
        assert!(!is_enabled_in(tmp.path()));
    }

    #[test]
    fn disable_is_idempotent_when_not_enabled() {
        let tmp = tmp_config();
        disable_in(tmp.path()).expect("disable on missing file should succeed");
        assert!(!is_enabled_in(tmp.path()));
    }

    #[test]
    fn enable_creates_intermediate_dirs() {
        let tmp = tmp_config();
        let autostart_dir = tmp.path().join("autostart");
        assert!(!autostart_dir.exists(), "should not exist before enable");

        enable_in(tmp.path(), Path::new("/usr/bin/handy")).expect("enable");

        assert!(autostart_dir.exists(), "autostart dir should be created");
    }
}
