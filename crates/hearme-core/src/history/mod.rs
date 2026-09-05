use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub id: i64,
    pub created_at: String,
    pub raw: String,
    pub cleaned: String,
    pub polished: Option<String>,
    pub lang: Option<String>,
    pub duration_ms: i64,
}

pub struct History {
    conn: Connection,
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS transcripts (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    raw TEXT NOT NULL,
    cleaned TEXT NOT NULL,
    polished TEXT,
    lang TEXT,
    duration_ms INTEGER NOT NULL
)";

impl History {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute(SCHEMA, [])?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute(SCHEMA, [])?;
        Ok(Self { conn })
    }

    pub fn insert(
        &self,
        raw: &str,
        cleaned: &str,
        polished: Option<&str>,
        lang: Option<&str>,
        duration_ms: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO transcripts (raw, cleaned, polished, lang, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![raw, cleaned, polished, lang, duration_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn recent(&self, limit: u32) -> Result<Vec<Record>> {
        self.query("SELECT id, created_at, raw, cleaned, polished, lang, duration_ms
                    FROM transcripts ORDER BY id DESC LIMIT ?1", rusqlite::params![limit])
    }

    pub fn search(&self, q: &str, limit: u32) -> Result<Vec<Record>> {
        let like = format!("%{q}%");
        self.query(
            "SELECT id, created_at, raw, cleaned, polished, lang, duration_ms
             FROM transcripts
             WHERE raw LIKE ?1 OR cleaned LIKE ?1 OR polished LIKE ?1
             ORDER BY id DESC LIMIT ?2",
            rusqlite::params![like, limit],
        )
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, |r| {
            Ok(Record {
                id: r.get(0)?,
                created_at: r.get(1)?,
                raw: r.get(2)?,
                cleaned: r.get(3)?,
                polished: r.get(4)?,
                lang: r.get(5)?,
                duration_ms: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_recent() {
        let h = History::open_in_memory().unwrap();
        h.insert("raw one", "clean one", None, Some("en"), 1200).unwrap();
        h.insert("raw two", "clean two", Some("polished two"), Some("es"), 800).unwrap();
        let rows = h.recent(10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cleaned, "clean two"); // newest first
        assert_eq!(rows[0].polished.as_deref(), Some("polished two"));
        assert_eq!(rows[1].lang.as_deref(), Some("en"));
        assert!(!rows[0].created_at.is_empty());
    }

    #[test]
    fn search_matches_any_text_column() {
        let h = History::open_in_memory().unwrap();
        h.insert("the quick fox", "the quick fox", None, None, 100).unwrap();
        h.insert("hola mundo", "hola mundo", Some("saludos cordiales"), None, 100).unwrap();
        assert_eq!(h.search("quick", 10).unwrap().len(), 1);
        assert_eq!(h.search("cordiales", 10).unwrap().len(), 1);
        assert_eq!(h.search("zzz", 10).unwrap().len(), 0);
    }

    #[test]
    fn limit_respected() {
        let h = History::open_in_memory().unwrap();
        for i in 0..5 {
            h.insert(&format!("r{i}"), &format!("c{i}"), None, None, 10).unwrap();
        }
        assert_eq!(h.recent(3).unwrap().len(), 3);
    }

    #[test]
    fn open_creates_file_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/history.sqlite3");
        let h = History::open(&path).unwrap();
        h.insert("a", "a", None, None, 1).unwrap();
        assert!(path.exists());
    }
}
