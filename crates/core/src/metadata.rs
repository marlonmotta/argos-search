//! SQLite metadata store for tracking indexed files.
//!
//! Stores path, mtime, size, and optional hash for incremental indexing.
//! Uses WAL mode for concurrent read/write performance.

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// A record of a file's metadata in the SQLite database.
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub mtime: i64,
    pub size: i64,
    pub hash: Option<String>,
    pub indexed_at: i64,
}

/// SQLite-backed metadata store for tracking file state.
pub struct MetadataStore {
    conn: Connection,
}

impl MetadataStore {
    /// Open (or create) the metadata database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // Enable WAL mode for concurrent read/write
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;"
        )?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path       TEXT PRIMARY KEY,
                mtime      INTEGER NOT NULL,
                size       INTEGER NOT NULL,
                hash       TEXT,
                indexed_at INTEGER NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    /// Get a file record by path.
    pub fn get(&self, path: &str) -> Result<Option<FileRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, mtime, size, hash, indexed_at FROM files WHERE path = ?1"
        )?;

        let record = stmt.query_row(params![path], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                mtime: row.get(1)?,
                size: row.get(2)?,
                hash: row.get(3)?,
                indexed_at: row.get(4)?,
            })
        });

        match record {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Insert or update a file record.
    pub fn upsert(&self, record: &FileRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO files (path, mtime, size, hash, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime,
                size = excluded.size,
                hash = excluded.hash,
                indexed_at = excluded.indexed_at",
            params![
                record.path,
                record.mtime,
                record.size,
                record.hash,
                record.indexed_at,
            ],
        )?;
        Ok(())
    }

    /// Remove a file record by path.
    pub fn remove(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files WHERE path = ?1",
            params![path],
        )?;
        Ok(())
    }

    /// Get all known file paths in the database.
    pub fn all_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;
        let paths = stmt.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// Get total count of indexed files.
    pub fn count(&self) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Remove records for paths that no longer exist on disk.
    pub fn prune_missing(&self) -> Result<u64> {
        let paths = self.all_paths()?;
        let mut removed = 0u64;
        for path_str in &paths {
            let path = PathBuf::from(path_str);
            if !path.exists() {
                self.remove(path_str)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_unix() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn test_metadata_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = MetadataStore::open(&db_path).unwrap();

        let record = FileRecord {
            path: "C:/test/file.txt".to_string(),
            mtime: 1700000000,
            size: 1024,
            hash: Some("abc123".to_string()),
            indexed_at: now_unix(),
        };

        // Insert
        store.upsert(&record).unwrap();

        // Read
        let found = store.get("C:/test/file.txt").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.size, 1024);
        assert_eq!(found.hash.as_deref(), Some("abc123"));

        // Count
        assert_eq!(store.count().unwrap(), 1);

        // Remove
        store.remove("C:/test/file.txt").unwrap();
        assert!(store.get("C:/test/file.txt").unwrap().is_none());
        assert_eq!(store.count().unwrap(), 0);
    }
}
