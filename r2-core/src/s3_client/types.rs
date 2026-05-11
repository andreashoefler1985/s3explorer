//! r2-core — S3 type definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Information about an S3 bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: Option<DateTime<Utc>>,
}

/// Information about an S3 object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<DateTime<Utc>>,
    pub e_tag: Option<String>,
    pub storage_class: Option<String>,
    pub is_prefix: bool,
}

/// Paginated object list response
#[derive(Debug, Clone)]
pub struct ObjectListResponse {
    pub objects: Vec<ObjectInfo>,
    pub is_truncated: bool,
    pub continuation_token: Option<String>,
}

/// S3 client configuration
#[derive(Debug, Clone)]
pub struct S3ClientConfig {
    pub endpoint_url: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_style: bool,
    pub connect_timeout_secs: u64,
    pub operation_timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for S3ClientConfig {
    fn default() -> Self {
        Self {
            endpoint_url: String::new(),
            region: "us-east-1".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            path_style: false,
            connect_timeout_secs: 30,
            operation_timeout_secs: 120,
            max_retries: 3,
        }
    }
}

/// Part information for multipart uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub part_number: i32,
    pub e_tag: String,
}