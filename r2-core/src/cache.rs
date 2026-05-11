//! r2-core — Metadata cache module
//!
//! Provides the CacheManager trait with SQLite implementation.

pub mod manager;
pub mod database;

pub use manager::{CacheManager, SqliteCacheManager};
pub use database::CacheDatabase;