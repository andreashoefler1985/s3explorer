//! r2-core — S3 Client module
//!
//! Provides S3Client trait and aws-sdk-s3 implementation.

pub mod client;
pub mod types;

pub use client::{AwsSdkS3Client, S3Client};
pub use types::{BucketInfo, ObjectInfo, AclGrant, Grantee, ObjectVersion};

/// Get the parent prefix (one level up)
pub fn parent_prefix(prefix: &str) -> String {
    if prefix.is_empty() {
        return String::new();
    }

    let trimmed = prefix.trim_end_matches('/');
    if let Some(last_slash) = trimmed.rfind('/') {
        trimmed[..=last_slash].to_string()
    } else {
        String::new()
    }
}

/// Format bytes in human-readable form (KB, MB, GB, TB)
pub fn format_bytes(bytes: i64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", bytes, units[unit_idx])
    } else {
        format!("{:.1} {}", size, units[unit_idx])
    }
}