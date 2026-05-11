//! r2-core — Profile data structures

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A stored S3 endpoint profile (without secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Unique identifier
    pub id: Uuid,
    /// Display name
    pub name: String,
    /// S3 endpoint URL (e.g., https://s3.eu-central-1.amazonaws.com)
    pub endpoint_url: String,
    /// AWS region
    pub region: String,
    /// Optional default bucket
    pub default_bucket: Option<String>,
    /// Whether to use path-style addressing
    #[serde(default)]
    pub path_style: bool,
}

impl Profile {
    /// Create a new profile with a generated UUID
    pub fn new(
        name: String,
        endpoint_url: String,
        region: String,
        default_bucket: Option<String>,
        path_style: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            endpoint_url,
            region,
            default_bucket,
            path_style,
        }
    }
}

/// Profiles configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    pub profiles: Vec<Profile>,
}

impl ProfilesConfig {
    pub fn new() -> Self {
        Self {
            profiles: Vec::new(),
        }
    }
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        Self::new()
    }
}
