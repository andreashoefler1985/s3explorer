//! r2-core — SQLite cache database setup and management

use std::path::PathBuf;
use std::sync::Mutex;
use rusqlite::Connection;
use tracing::{debug, info};

use crate::error::{CacheError, Result};

/// SQLite cache database wrapper
pub struct CacheDatabase {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl CacheDatabase {
    /// Open or create the cache database
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| CacheError::Database(format!(
                "Cannot create cache dir: {}", e
            )))?;

        let db_path = config_dir.join("cache.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| CacheError::Database(format!(
                "Cannot open database: {}", e
            )))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| CacheError::Database(format!(
                "Cannot set WAL mode: {}", e
            )))?;

        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| CacheError::Database(format!(
                "Cannot enable foreign keys: {}", e
            )))?;

        let db = Self {
            conn: Mutex::new(conn),
            db_path: db_path.clone(),
        };

        db.initialize_schema()?;

        info!(path = %db_path.display(), "Cache database initialized");
        Ok(db)
    }

    /// Create the database schema if it doesn't exist
    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS cached_buckets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id TEXT NOT NULL,
                bucket_name TEXT NOT NULL,
                creation_date TEXT,
                cached_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(profile_id, bucket_name)
            );

            CREATE TABLE IF NOT EXISTS cached_objects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id TEXT NOT NULL,
                bucket_name TEXT NOT NULL,
                object_key TEXT NOT NULL,
                size INTEGER DEFAULT 0,
                last_modified TEXT,
                e_tag TEXT,
                storage_class TEXT,
                is_prefix BOOLEAN NOT NULL DEFAULT 0,
                cached_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(profile_id, bucket_name, object_key)
            );

            CREATE TABLE IF NOT EXISTS cache_metadata (
                profile_id TEXT NOT NULL,
                bucket_name TEXT NOT NULL,
                prefix TEXT NOT NULL DEFAULT '',
                last_synced_at TEXT,
                object_count INTEGER DEFAULT 0,
                PRIMARY KEY (profile_id, bucket_name, prefix)
            );

            CREATE INDEX IF NOT EXISTS idx_objects_lookup
                ON cached_objects(profile_id, bucket_name, object_key);

            CREATE INDEX IF NOT EXISTS idx_buckets_profile
                ON cached_buckets(profile_id);

            CREATE INDEX IF NOT EXISTS idx_metadata_lookup
                ON cache_metadata(profile_id, bucket_name, prefix);
            "
        ).map_err(|e| CacheError::Database(format!(
            "Cannot create schema: {}", e
        )))?;

        debug!("Cache database schema initialized");
        Ok(())
    }

    /// Get the database path
    pub fn path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Execute a function with a reference to the connection
    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> std::result::Result<T, rusqlite::Error>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn).map_err(|e| CacheError::Database(e.to_string()).into())
    }
}
