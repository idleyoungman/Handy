use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::{mpsc, watch};

use crate::backend_event::BackendEvent;
use crate::config::{self, AppSettings};

/// The single seam between backend logic and the UI layer.
///
/// `AppContext` replaces `AppHandle` throughout the backend.  It is cheap to
/// clone (everything is behind `Arc`) and safe to send across threads.
///
/// - **Settings** — shared in-memory state; reads take a brief read lock and
///   clone; writes take a write lock then schedule a debounced disk flush.
/// - **Events** — `emit()` is non-blocking; it sends a `BackendEvent` on an
///   mpsc channel whose receiver lives in the UI root component.
#[derive(Clone)]
pub struct AppContext {
    settings: Arc<RwLock<AppSettings>>,
    event_tx: mpsc::Sender<BackendEvent>,
    save_tx: Arc<watch::Sender<AppSettings>>,
}

impl AppContext {
    /// Creates a new `AppContext`.
    ///
    /// `event_tx` is the sender half of the backend→UI event channel.
    /// `config_path` is the path to write settings to (used for debounced saves).
    pub fn new(
        settings: AppSettings,
        event_tx: mpsc::Sender<BackendEvent>,
        config_path: PathBuf,
    ) -> Self {
        let save_tx = config::spawn_debounced_saver(config_path, settings.clone());
        Self {
            settings: Arc::new(RwLock::new(settings)),
            event_tx,
            save_tx,
        }
    }

    /// Returns a snapshot of the current settings.
    ///
    /// Takes a short-lived read lock and clones, so the returned value is
    /// independent of subsequent `update_settings` calls.
    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .expect("settings RwLock poisoned")
            .clone()
    }

    /// Applies `f` to the in-memory settings and schedules a debounced
    /// write to disk.  Does not block on the disk write.
    pub fn update_settings(&self, f: impl FnOnce(&mut AppSettings)) {
        let updated = {
            let mut guard = self.settings.write().expect("settings RwLock poisoned");
            f(&mut guard);
            guard.clone()
        };
        // watch::Sender::send only fails if all receivers are dropped (task exited early).
        self.save_tx.send(updated).ok();
    }

    /// Sends a `BackendEvent` to the UI layer.  Non-blocking; events are
    /// dropped if the UI has not yet set up its receiver.
    pub fn emit(&self, event: BackendEvent) {
        // try_send so we never block a backend thread on a full channel.
        if let Err(e) = self.event_tx.try_send(event) {
            tracing::warn!("BackendEvent dropped: {e}");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_event::BackendEvent;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    fn make_ctx(dir: &TempDir) -> (AppContext, mpsc::Receiver<BackendEvent>) {
        let (event_tx, event_rx) = mpsc::channel(32);
        let config_path = dir.path().join("handy").join("config.toml");
        let ctx = AppContext::new(AppSettings::default(), event_tx, config_path);
        (ctx, event_rx)
    }

    #[tokio::test]
    async fn settings_returns_consistent_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _rx) = make_ctx(&dir);

        ctx.update_settings(|s| s.push_to_talk = true);
        assert!(ctx.settings().push_to_talk);

        ctx.update_settings(|s| s.history_limit = 77);
        assert_eq!(ctx.settings().history_limit, 77);
    }

    #[tokio::test]
    async fn emit_delivers_events_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, mut rx) = make_ctx(&dir);

        ctx.emit(BackendEvent::RecordingStarted);
        ctx.emit(BackendEvent::RecordingStopped);
        ctx.emit(BackendEvent::FocusWindow);

        let recv = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(recv, BackendEvent::RecordingStarted));

        let recv = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(recv, BackendEvent::RecordingStopped));

        let recv = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(recv, BackendEvent::FocusWindow));
    }

    #[tokio::test]
    async fn update_settings_under_concurrent_reads() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _rx) = make_ctx(&dir);
        let ctx = Arc::new(ctx);

        let readers: Vec<_> = (0..8)
            .map(|_| {
                let c = Arc::clone(&ctx);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        let _ = c.settings();
                    }
                })
            })
            .collect();

        // Writer runs concurrently with readers.
        for i in 0..100usize {
            ctx.update_settings(|s| s.history_limit = i);
        }

        for h in readers {
            h.join().unwrap();
        }

        assert_eq!(ctx.settings().history_limit, 99);
    }

    #[tokio::test]
    async fn debounced_save_collapses_rapid_updates() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("handy").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        let (event_tx, _rx) = mpsc::channel(32);
        let ctx = AppContext::new(AppSettings::default(), event_tx, config_path.clone());

        for i in 0..20usize {
            ctx.update_settings(|s| s.history_limit = i);
        }

        // Allow debounce window to expire.
        tokio::time::sleep(Duration::from_millis(800)).await;

        let loaded = config::load_from(&config_path).expect("load after debounce");
        assert_eq!(loaded.history_limit, 19);
    }
}
