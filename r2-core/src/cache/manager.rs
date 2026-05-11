//! r2-core — CacheManager trait and SQLite implementation

use chrono::{DateTime, Utc};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use super::database::CacheDatabase;
use crate::error::Result;
use crate::s3_client::types::{BucketInfo, ObjectInfo};

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub bucket_count: usize,
    pub object_count: usize,
    pub cache_age_secs: Option<i64>,
}

/// CacheManager trait — handles local metadata caching
pub trait CacheManager: Send + Sync {
    /// Cache bucket list for a profile
    fn cache_buckets(&self, profile_id: &Uuid, buckets: &[BucketInfo]) -> Result<()>;

    /// Get cached buckets for a profile
    fn get_cached_buckets(&self, profile_id: &Uuid) -> Result<Vec<BucketInfo>>;

    /// Cache objects for a profile/bucket/prefix
    fn cache_objects(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
        objects: &[ObjectInfo],
    ) -> Result<()>;

    /// Get cached objects for a profile/bucket/prefix
    fn get_cached_objects(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectInfo>>;

    /// Check if cached data is stale
    fn is_cache_stale(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
    ) -> Result<bool>;

    /// Clear all cached data for a profile
    fn clear_cache(&self, profile_id: &Uuid) -> Result<()>;

    /// Get the age of the cache in seconds (oldest entry)
    fn get_cache_age(&self, profile_id: &Uuid) -> Result<Option<i64>>;

    /// Get cache statistics
    fn get_cache_stats(&self, profile_id: &Uuid) -> Result<CacheStats>;
}

/// SQLite-based cache manager implementation
pub struct SqliteCacheManager {
    db: Arc<CacheDatabase>,
    bucket_ttl_secs: i64,
    object_ttl_secs: i64,
}

impl SqliteCacheManager {
    /// Create a new SqliteCacheManager
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        let db = Arc::new(CacheDatabase::new(config_dir)?);
        Ok(Self {
            db,
            bucket_ttl_secs: 300,
            object_ttl_secs: 30,
        })
    }

    /// Create with custom TTL values
    #[allow(dead_code)]
    pub fn with_ttl(config_dir: PathBuf, bucket_ttl_secs: i64, object_ttl_secs: i64) -> Result<Self> {
        let db = Arc::new(CacheDatabase::new(config_dir)?);
        Ok(Self {
            db,
            bucket_ttl_secs,
            object_ttl_secs,
        })
    }
}

impl CacheManager for SqliteCacheManager {
    fn cache_buckets(&self, profile_id: &Uuid, buckets: &[BucketInfo]) -> Result<()> {
        let profile_id_str = profile_id.to_string();
        let now = Utc::now().to_rfc3339();

        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cached_buckets WHERE profile_id = ?1",
                params![profile_id_str],
            )?;

            for bucket in buckets {
                let creation_date = bucket.creation_date
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default();
                conn.execute(
                    "INSERT OR REPLACE INTO cached_buckets (profile_id, bucket_name, creation_date, cached_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![profile_id_str, bucket.name, creation_date, now],
                )?;
            }

            Ok(())
        })?;

        info!(
            profile_id = %profile_id_str,
            count = buckets.len(),
            "Buckets cached"
        );
        Ok(())
    }

    fn get_cached_buckets(&self, profile_id: &Uuid) -> Result<Vec<BucketInfo>> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT bucket_name, creation_date FROM cached_buckets
                 WHERE profile_id = ?1
                 ORDER BY bucket_name"
            )?;

            let buckets = stmt.query_map(params![profile_id_str], |row| {
                let name: String = row.get(0)?;
                let creation_date_str: String = row.get(1)?;
                let creation_date = if creation_date_str.is_empty() {
                    None
                } else {
                    DateTime::parse_from_rfc3339(&creation_date_str)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                };

                Ok(BucketInfo { name, creation_date })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            Ok(buckets)
        })
    }

    fn cache_objects(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
        objects: &[ObjectInfo],
    ) -> Result<()> {
        let profile_id_str = profile_id.to_string();
        let now = Utc::now().to_rfc3339();

        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cached_objects WHERE profile_id = ?1 AND bucket_name = ?2 AND object_key LIKE ?3",
                params![profile_id_str, bucket, format!("{}%", prefix)],
            )?;

            for obj in objects {
                let last_modified = obj.last_modified
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default();
                conn.execute(
                    "INSERT OR REPLACE INTO cached_objects
                     (profile_id, bucket_name, object_key, size, last_modified, e_tag, storage_class, is_prefix, cached_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        profile_id_str,
                        bucket,
                        obj.key,
                        obj.size,
                        last_modified,
                        obj.e_tag,
                        obj.storage_class,
                        obj.is_prefix as i32,
                        now,
                    ],
                )?;
            }

            conn.execute(
                "INSERT OR REPLACE INTO cache_metadata (profile_id, bucket_name, prefix, last_synced_at, object_count)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![profile_id_str, bucket, prefix, now, objects.len() as i64],
            )?;

            Ok(())
        })?;

        debug!(
            profile_id = %profile_id_str,
            bucket = %bucket,
            prefix = %prefix,
            count = objects.len(),
            "Objects cached"
        );
        Ok(())
    }

    fn get_cached_objects(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectInfo>> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT object_key, size, last_modified, e_tag, storage_class, is_prefix
                 FROM cached_objects
                 WHERE profile_id = ?1 AND bucket_name = ?2 AND object_key LIKE ?3
                 ORDER BY object_key"
            )?;

            let objects = stmt.query_map(
                params![profile_id_str, bucket, format!("{}%", prefix)],
                |row| {
                    let key: String = row.get(0)?;
                    let size: i64 = row.get(1)?;
                    let last_modified_str: String = row.get(2)?;
                    let e_tag: Option<String> = row.get(3)?;
                    let storage_class: Option<String> = row.get(4)?;
                    let is_prefix: i32 = row.get(5)?;

                    let last_modified = if last_modified_str.is_empty() {
                        None
                    } else {
                        DateTime::parse_from_rfc3339(&last_modified_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    };

                    Ok(ObjectInfo {
                        key,
                        size,
                        last_modified,
                        e_tag,
                        storage_class,
                        is_prefix: is_prefix != 0,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            Ok(objects)
        })
    }

    fn is_cache_stale(
        &self,
        profile_id: &Uuid,
        bucket: &str,
        prefix: &str,
    ) -> Result<bool> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT last_synced_at FROM cache_metadata
                 WHERE profile_id = ?1 AND bucket_name = ?2 AND prefix = ?3"
            )?;

            let result: Option<String> = stmt.query_row(
                params![profile_id_str, bucket, prefix],
                |row| row.get(0),
            ).ok();

            match result {
                None => Ok(true),
                Some(last_synced) => {
                    let last_synced_dt = DateTime::parse_from_rfc3339(&last_synced)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let elapsed = Utc::now() - last_synced_dt;
                    Ok(elapsed.num_seconds() > self.object_ttl_secs)
                }
            }
        })
    }

    fn get_cache_age(&self, profile_id: &Uuid) -> Result<Option<i64>> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT MIN(cached_at) FROM cached_objects WHERE profile_id = ?1"
            )?;

            let result: Option<String> = stmt.query_row(
                params![profile_id_str],
                |row| row.get(0),
            ).ok();

            match result {
                None => Ok(None),
                Some(date_str) => {
                    let dt = DateTime::parse_from_rfc3339(&date_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let age = (Utc::now() - dt).num_seconds();
                    Ok(Some(age))
                }
            }
        })
    }

    fn get_cache_stats(&self, profile_id: &Uuid) -> Result<CacheStats> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            let bucket_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cached_buckets WHERE profile_id = ?1",
                params![profile_id_str],
                |row| row.get(0),
            ).unwrap_or(0);

            let object_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cached_objects WHERE profile_id = ?1",
                params![profile_id_str],
                |row| row.get(0),
            ).unwrap_or(0);

            let cache_age = {
                let mut stmt = conn.prepare(
                    "SELECT MIN(cached_at) FROM cached_objects WHERE profile_id = ?1"
                )?;
                let result: Option<String> = stmt.query_row(
                    params![profile_id_str],
                    |row| row.get(0),
                ).ok();
                match result {
                    None => None,
                    Some(date_str) => {
                        let dt = DateTime::parse_from_rfc3339(&date_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        Some((Utc::now() - dt).num_seconds())
                    }
                }
            };

            Ok(CacheStats {
                bucket_count: bucket_count as usize,
                object_count: object_count as usize,
                cache_age_secs: cache_age,
            })
        })
    }

    fn clear_cache(&self, profile_id: &Uuid) -> Result<()> {
        let profile_id_str = profile_id.to_string();

        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cached_buckets WHERE profile_id = ?1",
                params![profile_id_str],
            )?;
            conn.execute(
                "DELETE FROM cached_objects WHERE profile_id = ?1",
                params![profile_id_str],
            )?;
            conn.execute(
                "DELETE FROM cache_metadata WHERE profile_id = ?1",
                params![profile_id_str],
            )?;
            Ok(())
        })?;

        info!(profile_id = %profile_id_str, "Cache cleared");
        Ok(())
    }
}
