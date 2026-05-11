//! r2-core — Event types for signal-based UI communication

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Pane identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaneId {
    A,
    B,
}

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneId::A => write!(f, "A"),
            PaneId::B => write!(f, "B"),
        }
    }
}

/// Object information for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
    pub is_prefix: bool,
}

/// Pane events — communicate between UI panes and core
#[derive(Debug, Clone)]
pub enum PaneEvent {
    /// Profile was changed in a pane
    ProfileChanged {
        pane_id: PaneId,
        profile_id: Uuid,
    },

    /// Bucket was changed in a pane
    BucketChanged {
        pane_id: PaneId,
        bucket_name: String,
    },

    /// Navigation to a different prefix
    PrefixChanged {
        pane_id: PaneId,
        prefix: String,
    },

    /// Object(s) were selected
    ObjectsSelected {
        pane_id: PaneId,
        objects: Vec<ObjectInfo>,
    },

    /// Files were dropped into a pane
    DropFiles {
        target_pane: PaneId,
        file_paths: Vec<String>,
        target_prefix: String,
    },

    /// S3 objects were dragged from one pane to another
    DropObjects {
        source_pane: PaneId,
        target_pane: PaneId,
        objects: Vec<ObjectInfo>,
    },

    /// Transfer was requested (e.g., from context menu)
    TransferRequested {
        job_id: Uuid,
    },
}

/// Transfer direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferDirection {
    S3ToS3,
    LocalToS3,
    S3ToLocal,
}

/// Transfer status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,
    Active,
    Paused,
    Completed,
    Failed,
}

impl std::fmt::Display for TransferStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferStatus::Pending => write!(f, "pending"),
            TransferStatus::Active => write!(f, "active"),
            TransferStatus::Paused => write!(f, "paused"),
            TransferStatus::Completed => write!(f, "completed"),
            TransferStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Transfer job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJob {
    pub id: Uuid,
    pub source_profile_id: Uuid,
    pub source_bucket: String,
    pub source_key: String,
    pub dest_profile_id: Uuid,
    pub dest_bucket: String,
    pub dest_key: String,
    pub direction: TransferDirection,
    pub total_bytes: i64,
    pub transferred_bytes: i64,
    pub status: TransferStatus,
    pub priority: i32,
    pub error_message: Option<String>,
}

/// Progress event for transfers
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub job_id: Uuid,
    pub transferred_bytes: i64,
    pub total_bytes: i64,
    pub speed_bps: i64,
    pub eta_seconds: Option<i64>,
}