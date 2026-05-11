//! r2-core — Error types

use thiserror::Error;

/// Unified error type for r2 operations.
#[derive(Error, Debug)]
pub enum Error {
    /// S3-related errors
    #[error("S3 error: {0}")]
    S3(#[from] S3Error),

    /// Credential storage errors
    #[error("Credential error: {0}")]
    Credential(#[from] CredentialError),

    /// Cache errors
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),

    /// Transfer errors
    #[error("Transfer error: {0}")]
    Transfer(#[from] TransferError),

    /// Configuration errors
    #[error("Config error: {0}")]
    Config(String),

    /// I/O errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// S3 operation errors
#[derive(Error, Debug)]
pub enum S3Error {
    #[error("AWS SDK error: {0}")]
    Aws(String),

    #[error("Bucket not found: {0}")]
    BucketNotFound(String),

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("Invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("Region mismatch")]
    RegionMismatch,
}

impl S3Error {
    /// Classify AWS SDK errors
    pub fn from_aws_error(err: impl Into<String>) -> Self {
        S3Error::Aws(err.into())
    }
}

/// Credential storage errors
#[derive(Error, Debug)]
pub enum CredentialError {
    #[error("Libsecret error: {0}")]
    Libsecret(String),

    #[error("Secret not found for profile: {0}")]
    SecretNotFound(String),

    #[error("Keyring locked")]
    KeyringLocked,

    #[error("Keyring unavailable: {0}")]
    KeyringUnavailable(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Decryption error: {0}")]
    DecryptionError(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
}

/// Cache errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Cache stale")]
    CacheStale,

    #[error("Invalid cache path")]
    InvalidPath,
}

/// Transfer errors
#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Job already exists: {0}")]
    JobAlreadyExists(String),

    #[error("Transfer failed: {0}")]
    TransferFailed(String),

    #[error("Multipart error: {0}")]
    MultipartError(String),

    #[error("Cancelled")]
    Cancelled,

    #[error("Paused")]
    Paused,
}

/// Result type alias for r2-core
pub type Result<T> = std::result::Result<T, Error>;