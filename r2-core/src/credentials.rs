//! r2-core — Credential storage module
//!
//! Provides the CredentialStorage trait with libsecret and encrypted file backends.

pub mod storage;
pub mod profile;

pub use storage::{CredentialStorage, LibsecretCredentialStorage, EncryptedFileBackend};
pub use profile::Profile;