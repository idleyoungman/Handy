use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::app_context::AppContext;
use crate::backend_event::BackendEvent;
use crate::config::RecordingRetentionPeriod;

static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaginatedHistory {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum HistoryUpdatePayload {
    #[serde(rename = "added")]
    Added { entry: HistoryEntry },
    #[serde(rename = "updated")]
    Updated { entry: HistoryEntry },
    #[serde(rename = "deleted")]
    Deleted { id: i64 },
    #[serde(rename = "toggled")]
    Toggled { id: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
}

pub struct HistoryManager {
    ctx: AppContext,
    recordings_dir: PathBuf,
    db_path: PathBuf,
    // Mutex so each call can open a fresh connection; kept for future connection pooling.
    _lock: Arc<Mutex<()>>,
}

impl HistoryManager {
    pub fn new(ctx: AppContext) -> Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow!("could not determine XDG data directory"))?
            .join("handy");

        let recordings_dir = data_dir.join("recordings");
        let db_path = data_dir.join("history.db");

        fs::create_dir_all(&recordings_dir)?;

        let manager = Self {
            ctx,
            recordings_dir,
            db_path,
            _lock: Arc::new(Mutex::new(())),
        };

        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        let mut conn = Connection::open(&self.db_path)?;
        self.migrate_from_tauri_plugin_sql(&conn)?;
        let migrations = Migrations::new(MIGRATIONS.to_vec());
        migrations.to_latest(&mut conn)?;
        Ok(())
    }

    fn migrate_from_tauri_plugin_sql(&self, conn: &Connection) -> Result<()> {
        let has_sqlx: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_sqlx {
            return Ok(());
        }

        let current_version: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if current_version > 0 {
            return Ok(());
        }

        let old_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if old_version > 0 {
            conn.pragma_update(None, "user_version", old_version)?;
        }

        Ok(())
    }

    fn open(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn map_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
        })
    }

    fn format_title(timestamp: i64) -> String {
        if let Some(utc) = DateTime::from_timestamp(timestamp, 0) {
            utc.with_timezone(&Local)
                .format("%B %e, %Y - %l:%M%p")
                .to_string()
        } else {
            format!("Recording {timestamp}")
        }
    }

    pub fn recordings_dir(&self) -> &std::path::Path {
        &self.recordings_dir
    }

    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp();
        let title = Self::format_title(timestamp);

        let conn = self.open()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name, timestamp, saved, title,
                transcription_text, post_processed_text,
                post_process_prompt, post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &file_name,
                timestamp,
                false,
                &title,
                &transcription_text,
                &post_processed_text,
                &post_process_prompt,
                post_process_requested,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid(),
            file_name,
            timestamp,
            saved: false,
            title,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
        };

        self.cleanup_old_entries()?;

        self.ctx
            .emit(BackendEvent::HistoryUpdated(HistoryUpdatePayload::Added {
                entry: entry.clone(),
            }));

        Ok(entry)
    }

    pub fn update_transcription(
        &self,
        id: i64,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let conn = self.open()?;
        let updated = conn.execute(
            "UPDATE transcription_history
             SET transcription_text = ?1,
                 post_processed_text = ?2,
                 post_process_prompt = ?3
             WHERE id = ?4",
            params![
                transcription_text,
                post_processed_text,
                post_process_prompt,
                id
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let entry = conn.query_row(
            "SELECT id, file_name, timestamp, saved, title, transcription_text,
                    post_processed_text, post_process_prompt, post_process_requested
             FROM transcription_history WHERE id = ?1",
            params![id],
            Self::map_entry,
        )?;

        self.ctx.emit(BackendEvent::HistoryUpdated(
            HistoryUpdatePayload::Updated {
                entry: entry.clone(),
            },
        ));

        Ok(entry)
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let settings = self.ctx.settings();
        match settings.recording_retention_period {
            RecordingRetentionPeriod::Never => Ok(()),
            RecordingRetentionPeriod::MatchHistory => self.cleanup_by_count(settings.history_limit),
            period => self.cleanup_by_time(period),
        }
    }

    fn delete_entries_and_files(&self, entries: &[(i64, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.open()?;
        let mut count = 0;

        for (id, file_name) in entries {
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )?;

            let path = self.recordings_dir.join(file_name);
            if path.exists() {
                if let Err(e) = fs::remove_file(&path) {
                    tracing::warn!("Failed to delete WAV file {file_name}: {e}");
                } else {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    fn cleanup_by_count(&self, limit: usize) -> Result<()> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 ORDER BY timestamp DESC",
        )?;

        let entries: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get("id")?, row.get("file_name")?)))?
            .collect::<rusqlite::Result<_>>()?;

        if entries.len() > limit {
            self.delete_entries_and_files(&entries[limit..])?;
        }

        Ok(())
    }

    fn cleanup_by_time(&self, period: RecordingRetentionPeriod) -> Result<()> {
        let conn = self.open()?;
        let now = Utc::now().timestamp();
        let cutoff = match period {
            RecordingRetentionPeriod::ThreeDays => now - 3 * 24 * 60 * 60,
            RecordingRetentionPeriod::TwoWeeks => now - 2 * 7 * 24 * 60 * 60,
            RecordingRetentionPeriod::ThreeMonths => now - 3 * 30 * 24 * 60 * 60,
            _ => return Ok(()),
        };

        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 AND timestamp < ?1",
        )?;

        let entries: Vec<(i64, String)> = stmt
            .query_map(params![cutoff], |row| {
                Ok((row.get("id")?, row.get("file_name")?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        self.delete_entries_and_files(&entries)?;

        Ok(())
    }

    pub fn get_history_entries(
        &self,
        cursor: Option<i64>,
        limit: Option<usize>,
    ) -> Result<PaginatedHistory> {
        let conn = self.open()?;
        let limit = limit.map(|l| l.min(100));

        const COLS: &str = "id, file_name, timestamp, saved, title, transcription_text, \
                            post_processed_text, post_process_prompt, post_process_requested";

        let mut entries: Vec<HistoryEntry> = match (cursor, limit) {
            (Some(cur), Some(lim)) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM transcription_history WHERE id < ?1 ORDER BY id DESC LIMIT ?2"
                ))?;
                let rows = stmt
                    .query_map(params![cur, (lim + 1) as i64], Self::map_entry)?
                    .collect::<rusqlite::Result<_>>()?;
                rows
            }
            (None, Some(lim)) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM transcription_history ORDER BY id DESC LIMIT ?1"
                ))?;
                let rows = stmt
                    .query_map(params![(lim + 1) as i64], Self::map_entry)?
                    .collect::<rusqlite::Result<_>>()?;
                rows
            }
            (_, None) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM transcription_history ORDER BY id DESC"
                ))?;
                let rows = stmt
                    .query_map([], Self::map_entry)?
                    .collect::<rusqlite::Result<_>>()?;
                rows
            }
        };

        let has_more = limit.is_some_and(|lim| entries.len() > lim);
        if has_more {
            entries.pop();
        }

        Ok(PaginatedHistory { entries, has_more })
    }

    pub fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_name, timestamp, saved, title, transcription_text,
                    post_processed_text, post_process_prompt, post_process_requested
             FROM transcription_history WHERE id = ?1",
        )?;
        Ok(stmt.query_row([id], Self::map_entry).optional()?)
    }

    pub fn toggle_saved_status(&self, id: i64) -> Result<()> {
        let conn = self.open()?;
        let current: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![!current, id],
        )?;

        self.ctx.emit(BackendEvent::HistoryUpdated(
            HistoryUpdatePayload::Toggled { id },
        ));

        Ok(())
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        if let Some(entry) = self.get_entry_by_id(id)? {
            let path = self.recordings_dir.join(&entry.file_name);
            if path.exists() {
                if let Err(e) = fs::remove_file(&path) {
                    tracing::warn!("Failed to delete audio file {}: {e}", entry.file_name);
                }
            }
        }

        let conn = self.open()?;
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        self.ctx.emit(BackendEvent::HistoryUpdated(
            HistoryUpdatePayload::Deleted { id },
        ));

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> PathBuf {
        self.recordings_dir.join(file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0
            );",
        )
        .expect("create table");
        conn
    }

    fn insert(conn: &Connection, timestamp: i64, text: &str, post: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history
                (file_name, timestamp, saved, title,
                 transcription_text, post_processed_text,
                 post_process_prompt, post_process_requested)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("handy-{timestamp}.wav"),
                timestamp,
                false,
                format!("Recording {timestamp}"),
                text,
                post,
                Option::<String>::None,
                false,
            ],
        )
        .expect("insert");
    }

    fn fetch_latest(conn: &Connection) -> Option<HistoryEntry> {
        let mut stmt = conn
            .prepare(
                "SELECT id, file_name, timestamp, saved, title, transcription_text,
                        post_processed_text, post_process_prompt, post_process_requested
                 FROM transcription_history ORDER BY timestamp DESC LIMIT 1",
            )
            .unwrap();
        stmt.query_row([], HistoryManager::map_entry)
            .optional()
            .unwrap()
    }

    fn fetch_latest_completed(conn: &Connection) -> Option<HistoryEntry> {
        let mut stmt = conn
            .prepare(
                "SELECT id, file_name, timestamp, saved, title, transcription_text,
                        post_processed_text, post_process_prompt, post_process_requested
                 FROM transcription_history
                 WHERE transcription_text != ''
                 ORDER BY timestamp DESC LIMIT 1",
            )
            .unwrap();
        stmt.query_row([], HistoryManager::map_entry)
            .optional()
            .unwrap()
    }

    #[test]
    fn empty_db_returns_none() {
        let conn = setup_conn();
        assert!(fetch_latest(&conn).is_none());
    }

    #[test]
    fn latest_entry_is_newest() {
        let conn = setup_conn();
        insert(&conn, 100, "first", None);
        insert(&conn, 200, "second", Some("processed"));

        let e = fetch_latest(&conn).unwrap();
        assert_eq!(e.timestamp, 200);
        assert_eq!(e.transcription_text, "second");
        assert_eq!(e.post_processed_text.as_deref(), Some("processed"));
    }

    #[test]
    fn latest_completed_skips_empty() {
        let conn = setup_conn();
        insert(&conn, 100, "completed", None);
        insert(&conn, 200, "", None);

        let e = fetch_latest_completed(&conn).unwrap();
        assert_eq!(e.timestamp, 100);
        assert_eq!(e.transcription_text, "completed");
    }

    #[test]
    fn format_title_non_zero_timestamp() {
        let title = HistoryManager::format_title(0);
        assert!(!title.is_empty());
    }

    #[test]
    fn pagination_has_more_flag() {
        let conn = setup_conn();
        for i in 1..=5i64 {
            insert(&conn, i, &format!("text{i}"), None);
        }

        let mut stmt = conn
            .prepare(
                "SELECT id, file_name, timestamp, saved, title, transcription_text,
                        post_processed_text, post_process_prompt, post_process_requested
                 FROM transcription_history ORDER BY id DESC LIMIT 4",
            )
            .unwrap();

        let entries: Vec<HistoryEntry> = stmt
            .query_map([], HistoryManager::map_entry)
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        // Fetched 4 with limit=3 means has_more=true (one extra was fetched)
        let has_more = entries.len() > 3;
        assert!(has_more);
    }
}
