use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::config::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Image,
}

impl ClipKind {
    fn as_str(self) -> &'static str {
        match self {
            ClipKind::Text => "text",
            ClipKind::Image => "image",
        }
    }

    fn parse(raw: &str) -> Self {
        if raw == "image" {
            ClipKind::Image
        } else {
            ClipKind::Text
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListRow {
    pub id: i64,
    pub kind: ClipKind,
    pub text: String,
    pub thumb_png: Option<Vec<u8>>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub enum ClipBody {
    Text(String),
    ImagePng(Vec<u8>),
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        crate::config::ensure_parent(path)?;
        let conn = Connection::open(path).map_err(db_err)?;
        conn.busy_timeout(std::time::Duration::from_millis(1000))
            .map_err(db_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_err)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(db_err)?;
        conn.pragma_update(None, "temp_store", "MEMORY")
            .map_err(db_err)?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn reopen(&mut self, path: &Path) -> Result<(), String> {
        *self = Store::open(path)?;
        Ok(())
    }

    pub fn upsert(
        &self,
        kind: ClipKind,
        text: Option<&str>,
        thumb_png: Option<&[u8]>,
        image_png: Option<&[u8]>,
        hash: &str,
        settings: &Settings,
    ) -> Result<(), String> {
        let now = now_us();
        let byte_len = text.map(|t| t.len()).unwrap_or(0)
            + thumb_png.map(|b| b.len()).unwrap_or(0)
            + image_png.map(|b| b.len()).unwrap_or(0);
        self.conn
            .execute(
                "INSERT INTO clips (created_at, kind, text, thumb_png, image_png, byte_len, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(hash) DO UPDATE SET created_at = excluded.created_at",
                params![
                    now,
                    kind.as_str(),
                    text,
                    thumb_png,
                    image_png,
                    byte_len as i64,
                    hash
                ],
            )
            .map_err(db_err)?;
        self.evict(settings)
    }

    pub fn evict(&self, settings: &Settings) -> Result<(), String> {
        let max_entries = settings.max_entries as i64;
        let max_bytes = (settings.max_db_mb as i64) * 1024 * 1024;
        loop {
            let count: i64 = self
                .conn
                .query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))
                .map_err(db_err)?;
            if count > max_entries {
                self.delete_oldest()?;
                continue;
            }
            let bytes: i64 = self
                .conn
                .query_row("SELECT COALESCE(SUM(byte_len), 0) FROM clips", [], |r| {
                    r.get(0)
                })
                .map_err(db_err)?;
            if bytes > max_bytes && count > 0 {
                self.delete_oldest()?;
                continue;
            }
            break;
        }
        Ok(())
    }

    fn delete_oldest(&self) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM clips WHERE id = (
                    SELECT id FROM clips ORDER BY created_at ASC, id ASC LIMIT 1
                )",
                [],
            )
            .map_err(db_err)?;
        Ok(())
    }

    pub fn list_recent(&self, limit: u32) -> Result<Vec<ListRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, text, thumb_png, created_at
                 FROM clips
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([limit as i64], row_to_list)
            .map_err(db_err)?;
        collect_rows(rows)
    }

    pub fn list_images(&self, limit: u32) -> Result<Vec<ListRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, text, thumb_png, created_at
                 FROM clips
                 WHERE kind = 'image'
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([limit as i64], row_to_list)
            .map_err(db_err)?;
        collect_rows(rows)
    }

    pub fn search_fts(&self, match_query: &str, limit: u32) -> Result<Vec<ListRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT clips.id, clips.kind, clips.text, clips.thumb_png, clips.created_at
                 FROM clips
                 JOIN clips_fts ON clips_fts.rowid = clips.id
                 WHERE clips_fts MATCH ?1
                 LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![match_query, limit as i64], row_to_list)
            .map_err(db_err)?;
        collect_rows(rows)
    }

    pub fn get_body(&self, id: i64) -> Result<Option<ClipBody>, String> {
        let row = self
            .conn
            .query_row(
                "SELECT kind, text, image_png FROM clips WHERE id = ?1",
                [id],
                |r| {
                    let kind: String = r.get(0)?;
                    let text: Option<String> = r.get(1)?;
                    let image: Option<Vec<u8>> = r.get(2)?;
                    Ok((kind, text, image))
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.map(|(kind, text, image)| {
            if ClipKind::parse(&kind) == ClipKind::Image {
                ClipBody::ImagePng(image.unwrap_or_default())
            } else {
                ClipBody::Text(text.unwrap_or_default())
            }
        }))
    }

    pub fn delete(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM clips WHERE id = ?1", [id])
            .map_err(db_err)?;
        Ok(())
    }
}

fn row_to_list(r: &rusqlite::Row<'_>) -> rusqlite::Result<ListRow> {
    let kind: String = r.get(1)?;
    let text: Option<String> = r.get(2)?;
    Ok(ListRow {
        id: r.get(0)?,
        kind: ClipKind::parse(&kind),
        text: text.unwrap_or_default(),
        thumb_png: r.get(3)?,
        created_at: r.get(4)?,
    })
}

fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<ListRow>>,
) -> Result<Vec<ListRow>, String> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(db_err)?);
    }
    Ok(out)
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS clips (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at INTEGER NOT NULL,
            kind       TEXT    NOT NULL CHECK (kind IN ('text','image')),
            text       TEXT,
            thumb_png  BLOB,
            image_png  BLOB,
            byte_len   INTEGER NOT NULL DEFAULT 0,
            hash       TEXT    NOT NULL UNIQUE
        );
        CREATE INDEX IF NOT EXISTS idx_clips_created ON clips(created_at DESC);
        CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
            text,
            content='clips',
            content_rowid='id',
            tokenize='unicode61 remove_diacritics 2'
        );
        CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN
            INSERT INTO clips_fts(rowid, text) VALUES (new.id, new.text);
        END;
        CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN
            INSERT INTO clips_fts(clips_fts, rowid, text) VALUES('delete', old.id, old.text);
        END;
        CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE ON clips BEGIN
            INSERT INTO clips_fts(clips_fts, rowid, text) VALUES('delete', old.id, old.text);
            INSERT INTO clips_fts(rowid, text) VALUES (new.id, new.text);
        END;
        ",
    )
    .map_err(db_err)
}

fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn db_err(err: impl ToString) -> String {
    err.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("clipi-test-{nanos}.db"))
    }

    #[test]
    fn insert_search_evict() {
        let path = temp_db();
        let store = Store::open(&path).expect("open");
        let mut settings = Settings::default();
        settings.max_entries = 10;
        settings.max_db_mb = 8;

        store
            .upsert(
                ClipKind::Text,
                Some("hello clipboard"),
                None,
                None,
                "h1",
                &settings,
            )
            .unwrap();
        store
            .upsert(
                ClipKind::Text,
                Some("fuzzy search rust"),
                None,
                None,
                "h2",
                &settings,
            )
            .unwrap();

        let recent = store.list_recent(80).unwrap();
        assert_eq!(recent.len(), 2);

        let hits = store.search_fts("fuzzy*", 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("fuzzy"));

        store
            .upsert(ClipKind::Text, Some("hello clipboard"), None, None, "h1", &settings)
            .unwrap();
        let recent = store.list_recent(80).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].text, "hello clipboard");

        settings.max_entries = 10;
        for i in 0..12 {
            store
                .upsert(
                    ClipKind::Text,
                    Some(&format!("item {i}")),
                    None,
                    None,
                    &format!("n{i}"),
                    &settings,
                )
                .unwrap();
        }
        let recent = store.list_recent(80).unwrap();
        assert!(recent.len() <= 10);

        let _ = std::fs::remove_file(path);
    }
}
