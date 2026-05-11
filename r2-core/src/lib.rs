//! r2-core — Core library for the r2 S3 browser
//!
//! This crate provides the core business logic for r2:
//! - S3 client operations (list, get, put, delete, copy, multipart)
//! - Credential storage (libsecret with encrypted file fallback)
//! - Metadata caching (SQLite)
//! - Transfer engine (stub for Sprint 3)
//! - Event types for UI communication

pub mod error;
pub mod events;
pub mod s3_client;
pub mod credentials;
pub mod cache;
pub mod transfer;

// Re-export commonly used types
pub use error::{Error, Result};
pub use events::{PaneEvent, PaneId, ObjectInfo as EventObjectInfo, ProgressEvent};
pub use s3_client::types::{BucketInfo, ObjectInfo, S3ClientConfig, Part, ObjectListResponse};
pub use s3_client::client::{S3Client, AwsSdkS3Client};
pub use credentials::storage::{CredentialStorage, LibsecretCredentialStorage, EncryptedFileBackend};
pub use credentials::profile::Profile;
pub use cache::manager::{CacheManager, SqliteCacheManager};
pub use transfer::{TransferEngine, StubTransferEngine, TokioTransferEngine, TransferJob, TransferDirection, TransferStatus, TransferSource, TransferDestination, TransferProgress, TransferPart};
