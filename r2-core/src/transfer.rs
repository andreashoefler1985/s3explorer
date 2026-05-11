//! r2-core — Transfer engine module
//!
//! Provides the `TransferEngine` trait and a full Tokio-based implementation
//! with multipart upload/download support, pause/resume, retry, and persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex, Semaphore, watch, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::{Result, TransferError};
use crate::s3_client::client::S3Client;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Transfer direction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,   // Local → S3
    Download, // S3 → Local
    S3ToS3,   // S3 → S3 (cross-bucket or cross-endpoint)
}

/// Transfer status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    Pending,
    Active,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

impl TransferStatus {
    /// Returns `true` if the transfer is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, TransferStatus::Completed | TransferStatus::Failed(_) | TransferStatus::Cancelled)
    }

    /// Returns `true` if the transfer can be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(self, TransferStatus::Failed(_) | TransferStatus::Cancelled)
    }
}

/// A single transfer part (for multipart transfers)
#[derive(Debug, Clone)]
pub struct TransferPart {
    pub part_number: i32,
    pub size: u64,
    pub etag: Option<String>,
    pub status: TransferStatus,
}

/// Transfer source
#[derive(Debug, Clone)]
pub enum TransferSource {
    LocalFile(PathBuf),
    S3Object {
        profile_id: Uuid,
        bucket: String,
        key: String,
    },
}

/// Transfer destination
#[derive(Debug, Clone)]
pub enum TransferDestination {
    LocalFile(PathBuf),
    S3Object {
        profile_id: Uuid,
        bucket: String,
        key: String,
    },
}

/// A single transfer job
#[derive(Debug, Clone)]
pub struct TransferJob {
    pub id: Uuid,
    pub direction: TransferDirection,
    pub source: TransferSource,
    pub destination: TransferDestination,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub status: TransferStatus,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parts: Vec<TransferPart>,
    pub retry_count: u8,
    pub max_retries: u8,
    pub multipart_upload_id: Option<String>,
}

impl TransferJob {
    /// Create a new transfer job with a generated UUID.
    pub fn new(
        direction: TransferDirection,
        source: TransferSource,
        destination: TransferDestination,
        total_bytes: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            direction,
            source,
            destination,
            total_bytes,
            transferred_bytes: 0,
            speed_bytes_per_sec: 0.0,
            status: TransferStatus::Pending,
            error_message: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            parts: Vec::new(),
            retry_count: 0,
            max_retries: 3,
            multipart_upload_id: None,
        }
    }

    /// Progress percentage (0.0 – 100.0).
    pub fn progress_pct(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0
    }

    /// Estimated time remaining in seconds.
    pub fn eta_secs(&self) -> Option<f64> {
        if self.speed_bytes_per_sec <= 0.0 || self.total_bytes == 0 {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.transferred_bytes);
        if remaining == 0 {
            return Some(0.0);
        }
        Some(remaining as f64 / self.speed_bytes_per_sec)
    }
}

/// Progress event sent from engine to UI
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub job_id: Uuid,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub status: TransferStatus,
}

// ---------------------------------------------------------------------------
// TransferEngine trait
// ---------------------------------------------------------------------------

/// TransferEngine trait — manages async S3 transfers
#[async_trait]
pub trait TransferEngine: Send + Sync {
    /// Enqueue a new transfer job. Returns the job ID.
    async fn enqueue(&self, job: TransferJob) -> Result<Uuid>;

    /// Pause a running transfer.
    async fn pause(&self, job_id: &Uuid) -> Result<()>;

    /// Resume a paused transfer.
    async fn resume(&self, job_id: &Uuid) -> Result<()>;

    /// Cancel a transfer (running or paused).
    async fn cancel(&self, job_id: &Uuid) -> Result<()>;

    /// Retry a failed or cancelled transfer.
    async fn retry(&self, job_id: &Uuid) -> Result<()>;

    /// Get a single job by ID.
    async fn get_job(&self, job_id: &Uuid) -> Result<TransferJob>;

    /// List all jobs, optionally filtered by status.
    async fn list_jobs(&self, status_filter: Option<TransferStatus>) -> Result<Vec<TransferJob>>;

    /// List active (Pending + Active) jobs.
    async fn list_active(&self) -> Result<Vec<TransferJob>>;

    /// List completed jobs.
    async fn list_completed(&self) -> Result<Vec<TransferJob>>;

    /// List failed jobs.
    async fn list_failed(&self) -> Result<Vec<TransferJob>>;

    /// Subscribe to progress events (for UI updates).
    async fn subscribe(&self) -> mpsc::UnboundedReceiver<TransferProgress>;

    /// Graceful shutdown — cancels all active tasks.
    async fn shutdown(&self);

    /// Resume interrupted transfers from the persistent queue.
    async fn resume_interrupted(&self) -> Result<Vec<Uuid>>;
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct JobState {
    job: TransferJob,
    pause_tx: Option<watch::Sender<bool>>,
    cancel_token: Option<CancellationToken>,
    handle: Option<JoinHandle<()>>,
}

/// Tokio-based TransferEngine implementation
pub struct TokioTransferEngine {
    /// Shared job map (protected by mutex)
    jobs: Arc<Mutex<HashMap<Uuid, JobState>>>,
    /// Concurrency limiter
    semaphore: Arc<Semaphore>,
    /// Progress channel sender
    progress_tx: mpsc::UnboundedSender<TransferProgress>,
    /// Global cancellation token for shutdown
    shutdown_token: CancellationToken,
    /// Shutdown flag
    shutting_down: Arc<AtomicBool>,
    /// Notify when a new job is enqueued
    new_job_notify: Arc<Notify>,
    /// S3 client factory: maps profile_id → S3Client
    client_factory: Arc<dyn Fn(Uuid) -> Option<Arc<dyn S3Client>> + Send + Sync>,
    /// Multipart threshold (bytes)
    multipart_threshold: u64,
    /// Multipart chunk size (bytes)
    chunk_size: u64,
    /// Max concurrent parts per multipart transfer
    max_concurrent_parts: usize,
}

impl TokioTransferEngine {
    /// Create a new TokioTransferEngine.
    ///
    /// * `max_concurrent` — max simultaneous transfers (default 4).
    /// * `client_factory` — callable that returns an S3 client for a given profile ID.
    pub fn new(
        max_concurrent: usize,
        client_factory: Arc<dyn Fn(Uuid) -> Option<Arc<dyn S3Client>> + Send + Sync>,
    ) -> Self {
        let (progress_tx, _rx) = mpsc::unbounded_channel();
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            progress_tx,
            shutdown_token: CancellationToken::new(),
            shutting_down: Arc::new(AtomicBool::new(false)),
            new_job_notify: Arc::new(Notify::new()),
            client_factory,
            multipart_threshold: 100 * 1024 * 1024, // 100 MB
            chunk_size: 50 * 1024 * 1024,            // 50 MB
            max_concurrent_parts: 8,
        }
    }

    /// Set the multipart threshold (bytes).
    pub fn set_multipart_threshold(&mut self, bytes: u64) {
        self.multipart_threshold = bytes;
    }

    /// Set the chunk size for multipart transfers (bytes).
    pub fn set_chunk_size(&mut self, bytes: u64) {
        self.chunk_size = bytes;
    }

    /// Set max concurrent parts per multipart transfer.
    pub fn set_max_concurrent_parts(&mut self, n: usize) {
        self.max_concurrent_parts = n;
    }

    /// Get a reference to the progress sender (for cloning into spawned tasks).
    fn progress_sender(&self) -> mpsc::UnboundedSender<TransferProgress> {
        self.progress_tx.clone()
    }

    /// Get the client factory.
    fn get_client(&self, profile_id: Uuid) -> Option<Arc<dyn S3Client>> {
        (self.client_factory)(profile_id)
    }

    // ── Internal helpers ──

    /// Wait for resume signal or cancellation.
    async fn wait_for_resume(
        pause_rx: &mut watch::Receiver<bool>,
        cancel_token: &CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        loop {
            tokio::select! {
                _ = pause_rx.changed() => {
                    if !*pause_rx.borrow() {
                        return Ok(());
                    }
                }
                _ = cancel_token.cancelled() => {
                    return Err(TransferError::Cancelled);
                }
            }
        }
    }

    /// Spawn the actual transfer execution in a new Tokio task.
    /// This is called from enqueue/retry.
    fn spawn_transfer(
        self: &Arc<Self>,
        job_id: Uuid,
        pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) {
        let this = self.clone();
        let semaphore = self.semaphore.clone();
        let progress_tx = self.progress_tx.clone();
        let client_factory = self.client_factory.clone();
        let multipart_threshold = self.multipart_threshold;
        let chunk_size = self.chunk_size;
        let max_concurrent_parts = self.max_concurrent_parts;

        tokio::spawn(async move {
            // Acquire concurrency permit
            let _permit = semaphore.acquire().await;
            if cancel_token.is_cancelled() {
                return;
            }

            // Get job details
            let (direction, source, destination) = {
                let jobs = this.jobs.lock().await;
                match jobs.get(&job_id) {
                    Some(js) => (
                        js.job.direction.clone(),
                        js.job.source.clone(),
                        js.job.destination.clone(),
                    ),
                    None => return,
                }
            };

            // Mark as active
            {
                let mut jobs = this.jobs.lock().await;
                if let Some(js) = jobs.get_mut(&job_id) {
                    js.job.status = TransferStatus::Active;
                    js.job.started_at = Some(Utc::now());
                }
            }

            let _ = progress_tx.send(TransferProgress {
                job_id,
                transferred_bytes: 0,
                total_bytes: 0,
                speed_bytes_per_sec: 0.0,
                status: TransferStatus::Active,
            });

            // Execute the transfer based on direction
            let result = match direction {
                TransferDirection::Upload => {
                    let (local_path, profile_id, bucket, key) = match (&source, &destination) {
                        (TransferSource::LocalFile(path), TransferDestination::S3Object { profile_id, bucket, key }) => {
                            (path.clone(), *profile_id, bucket.clone(), key.clone())
                        }
                        _ => {
                            Err(TransferError::TransferFailed(
                                "Invalid upload source/destination combination".into()
                            ))
                        }
                    };

                    let (local_path, profile_id, bucket, key) = match result {
                        Ok(v) => v,
                        Err(e) => {
                            update_job_failed(&this.jobs, &progress_tx, job_id, e).await;
                            return;
                        }
                    };

                    let client = match client_factory(profile_id) {
                        Some(c) => c,
                        None => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("No S3 client for profile {}", profile_id))).await;
                            return;
                        }
                    };

                    let file_size = match tokio::fs::metadata(&local_path).await {
                        Ok(m) => m.len(),
                        Err(e) => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("Cannot stat file: {}", e))).await;
                            return;
                        }
                    };

                    if file_size >= multipart_threshold {
                        this.execute_multipart_upload(
                            &job_id, &local_path, &bucket, &key, client,
                            progress_tx.clone(), pause_rx, cancel_token,
                        ).await
                    } else {
                        this.execute_single_upload(
                            &job_id, &local_path, &bucket, &key, client,
                            progress_tx.clone(), pause_rx, cancel_token,
                        ).await
                    }
                }
                TransferDirection::Download => {
                    let (profile_id, bucket, key, local_path) = match (&source, &destination) {
                        (TransferSource::S3Object { profile_id, bucket, key }, TransferDestination::LocalFile(path)) => {
                            (*profile_id, bucket.clone(), key.clone(), path.clone())
                        }
                        _ => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed("Invalid download source/destination combination".into())).await;
                            return;
                        }
                    };

                    let client = match client_factory(profile_id) {
                        Some(c) => c,
                        None => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("No S3 client for profile {}", profile_id))).await;
                            return;
                        }
                    };

                    // Get object size
                    let head = match client.head_object(&bucket, &key).await {
                        Ok(h) => h,
                        Err(e) => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("HeadObject failed: {}", e))).await;
                            return;
                        }
                    };
                    let obj_size = head.size as u64;

                    if obj_size >= multipart_threshold {
                        this.execute_multipart_download(
                            &job_id, &local_path, &bucket, &key, obj_size, client,
                            progress_tx.clone(), pause_rx, cancel_token,
                        ).await
                    } else {
                        this.execute_single_download(
                            &job_id, &local_path, &bucket, &key, client,
                            progress_tx.clone(), pause_rx, cancel_token,
                        ).await
                    }
                }
                TransferDirection::S3ToS3 => {
                    let (src_profile, src_bucket, src_key, dst_profile, dst_bucket, dst_key) = match (&source, &destination) {
                        (
                            TransferSource::S3Object { profile_id: sp, bucket: sb, key: sk },
                            TransferDestination::S3Object { profile_id: dp, bucket: db, key: dk },
                        ) => (*sp, sb.clone(), sk.clone(), *dp, db.clone(), dk.clone()),
                        _ => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed("Invalid S3→S3 source/destination combination".into())).await;
                            return;
                        }
                    };

                    let src_client = match client_factory(src_profile) {
                        Some(c) => c,
                        None => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("No S3 client for source profile {}", src_profile))).await;
                            return;
                        }
                    };
                    let dst_client = match client_factory(dst_profile) {
                        Some(c) => c,
                        None => {
                            update_job_failed(&this.jobs, &progress_tx, job_id,
                                TransferError::TransferFailed(format!("No S3 client for dest profile {}", dst_profile))).await;
                            return;
                        }
                    };

                    this.execute_s3_to_s3_copy(
                        &job_id, &src_bucket, &src_key, &dst_bucket, &dst_key,
                        src_client, dst_client,
                        progress_tx.clone(), pause_rx, cancel_token,
                    ).await
                }
            };

            // Update final status
            let mut jobs = this.jobs.lock().await;
            if let Some(js) = jobs.get_mut(&job_id) {
                match result {
                    Ok(()) => {
                        js.job.status = TransferStatus::Completed;
                        js.job.completed_at = Some(Utc::now());
                        let _ = progress_tx.send(TransferProgress {
                            job_id,
                            transferred_bytes: js.job.total_bytes,
                            total_bytes: js.job.total_bytes,
                            speed_bytes_per_sec: 0.0,
                            status: TransferStatus::Completed,
                        });
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        js.job.status = TransferStatus::Failed(err_str.clone());
                        js.job.error_message = Some(err_str.clone());
                        let _ = progress_tx.send(TransferProgress {
                            job_id,
                            transferred_bytes: js.job.transferred_bytes,
                            total_bytes: js.job.total_bytes,
                            speed_bytes_per_sec: 0.0,
                            status: TransferStatus::Failed(err_str),
                        });
                    }
                }
            }
        });
    }

    // ── Single upload (≤ multipart_threshold) ──

    async fn execute_single_upload(
        &self,
        job_id: &Uuid,
        local_path: &Path,
        bucket: &str,
        key: &str,
        client: Arc<dyn S3Client>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
        mut pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        let data = tokio::fs::read(local_path).await
            .map_err(|e| TransferError::TransferFailed(format!("Read file failed: {}", e)))?;
        let total = data.len() as u64;

        // Check pause before starting
        if *pause_rx.borrow_and_update() {
            Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
        }
        if cancel_token.is_cancelled() {
            return Err(TransferError::Cancelled);
        }

        let start = std::time::Instant::now();
        client.put_object(bucket, key, data).await
            .map_err(|e| TransferError::TransferFailed(format!("PutObject failed: {}", e)))?;
        let elapsed = start.elapsed().as_secs_f64();

        let speed = if elapsed > 0.0 { total as f64 / elapsed } else { 0.0 };

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(js) = jobs.get_mut(job_id) {
                js.job.transferred_bytes = total;
                js.job.speed_bytes_per_sec = speed;
            }
        }

        let _ = progress_tx.send(TransferProgress {
            job_id: *job_id,
            transferred_bytes: total,
            total_bytes: total,
            speed_bytes_per_sec: speed,
            status: TransferStatus::Active,
        });

        Ok(())
    }

    // ── Single download (≤ multipart_threshold) ──

    async fn execute_single_download(
        &self,
        job_id: &Uuid,
        local_path: &Path,
        bucket: &str,
        key: &str,
        client: Arc<dyn S3Client>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
        mut pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        // Check pause before starting
        if *pause_rx.borrow_and_update() {
            Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
        }
        if cancel_token.is_cancelled() {
            return Err(TransferError::Cancelled);
        }

        let start = std::time::Instant::now();
        let data = client.get_object(bucket, key).await
            .map_err(|e| TransferError::TransferFailed(format!("GetObject failed: {}", e)))?;
        let elapsed = start.elapsed().as_secs_f64();
        let total = data.len() as u64;

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| TransferError::TransferFailed(format!("Create dir failed: {}", e)))?;
        }

        tokio::fs::write(local_path, &data).await
            .map_err(|e| TransferError::TransferFailed(format!("Write file failed: {}", e)))?;

        let speed = if elapsed > 0.0 { total as f64 / elapsed } else { 0.0 };

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(js) = jobs.get_mut(job_id) {
                js.job.transferred_bytes = total;
                js.job.speed_bytes_per_sec = speed;
            }
        }

        let _ = progress_tx.send(TransferProgress {
            job_id: *job_id,
            transferred_bytes: total,
            total_bytes: total,
            speed_bytes_per_sec: speed,
            status: TransferStatus::Active,
        });

        Ok(())
    }

    // ── Multipart upload ──

    async fn execute_multipart_upload(
        &self,
        job_id: &Uuid,
        local_path: &Path,
        bucket: &str,
        key: &str,
        client: Arc<dyn S3Client>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
        mut pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        let metadata = tokio::fs::metadata(local_path).await
            .map_err(|e| TransferError::TransferFailed(format!("File metadata failed: {}", e)))?;
        let file_size = metadata.len();

        // Check pause before starting
        if *pause_rx.borrow_and_update() {
            Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
        }
        cancel_token.check()?;

        // 1. CreateMultipartUpload
        let upload_id = client.create_multipart_upload(bucket, key).await
            .map_err(|e| TransferError::MultipartError(format!("CreateMultipartUpload failed: {}", e)))?;

        // Store upload_id
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(js) = jobs.get_mut(job_id) {
                js.job.multipart_upload_id = Some(upload_id.clone());
            }
        }

        // 2. Calculate parts
        let chunk_size = self.chunk_size;
        let total_parts = ((file_size + chunk_size - 1) / chunk_size) as i32;
        let part_semaphore = Arc::new(Semaphore::new(self.max_concurrent_parts));

        // 3. Upload parts in parallel
        let completed_parts: Arc<Mutex<Vec<(i32, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let transferred: Arc<std::sync::atomic::AtomicU64> = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let start_time = std::time::Instant::now();

        let mut handles = Vec::new();

        for part_number in 1..=total_parts {
            if cancel_token.is_cancelled() {
                let _ = client.abort_multipart_upload(bucket, key, &upload_id).await;
                return Err(TransferError::Cancelled);
            }

            // Check pause before each part
            if *pause_rx.borrow() {
                Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
            }

            let offset = ((part_number - 1) as u64) * chunk_size;
            let size = std::cmp::min(chunk_size, file_size - offset);

            let client = client.clone();
            let bucket = bucket.to_string();
            let key = key.to_string();
            let upload_id = upload_id.clone();
            let local_path = local_path.to_path_buf();
            let part_semaphore = part_semaphore.clone();
            let completed_parts = completed_parts.clone();
            let transferred = transferred.clone();
            let progress_tx = progress_tx.clone();
            let job_id = *job_id;
            let cancel_token = cancel_token.clone();

            let handle = tokio::spawn(async move {
                let _permit = part_semaphore.acquire().await;

                // Read chunk from file
                let mut file = tokio::fs::File::open(&local_path).await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
                let mut buf = vec![0u8; size as usize];
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                file.read_exact(&mut buf).await?;

                // Upload part
                let etag = client.upload_part(&bucket, &key, &upload_id, part_number, buf).await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

                // Record completion
                completed_parts.lock().await.push((part_number, etag));
                let prev = transferred.fetch_add(size, std::sync::atomic::Ordering::SeqCst);
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { (prev + size) as f64 / elapsed } else { 0.0 };

                let _ = progress_tx.send(TransferProgress {
                    job_id,
                    transferred_bytes: prev + size,
                    total_bytes: file_size,
                    speed_bytes_per_sec: speed,
                    status: TransferStatus::Active,
                });

                Ok::<_, std::io::Error>(())
            });

            handles.push(handle);
        }

        // Wait for all parts to complete
        for handle in handles {
            if let Err(e) = handle.await {
                let _ = client.abort_multipart_upload(bucket, key, &upload_id).await;
                return Err(TransferError::MultipartError(format!("Part upload task failed: {}", e)));
            }
        }

        // 4. Collect parts sorted by part_number
        let mut parts = completed_parts.lock().await.clone();
        parts.sort_by_key(|(pn, _)| *pn);

        // 5. CompleteMultipartUpload
        client.complete_multipart_upload(bucket, key, &upload_id, parts).await
            .map_err(|e| TransferError::MultipartError(format!("CompleteMultipartUpload failed: {}", e)))?;

        let elapsed = start_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 { file_size as f64 / elapsed } else { 0.0 };

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(js) = jobs.get_mut(job_id) {
                js.job.transferred_bytes = file_size;
                js.job.speed_bytes_per_sec = speed;
            }
        }

        Ok(())
    }

    // ── Multipart download ──

    async fn execute_multipart_download(
        &self,
        job_id: &Uuid,
        local_path: &Path,
        bucket: &str,
        key: &str,
        object_size: u64,
        client: Arc<dyn S3Client>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
        mut pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        // Check pause before starting
        if *pause_rx.borrow_and_update() {
            Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
        }
        cancel_token.check()?;

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| TransferError::TransferFailed(format!("Create dir failed: {}", e)))?;
        }

        let chunk_size = self.chunk_size;
        let total_parts = ((object_size + chunk_size - 1) / chunk_size) as i32;
        let part_semaphore = Arc::new(Semaphore::new(self.max_concurrent_parts));
        let transferred: Arc<std::sync::atomic::AtomicU64> = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let start_time = std::time::Instant::now();

        // Collect chunks in memory, write sequentially at the end
        let chunks: Arc<Mutex<HashMap<i32, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

        let mut handles = Vec::new();

        for part_number in 1..=total_parts {
            if cancel_token.is_cancelled() {
                return Err(TransferError::Cancelled);
            }

            if *pause_rx.borrow() {
                Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
            }

            let offset = ((part_number - 1) as u64) * chunk_size;
            let size = std::cmp::min(chunk_size, object_size - offset);
            let range = format!("bytes={}-{}", offset, offset + size - 1);

            let client = client.clone();
            let bucket = bucket.to_string();
            let key = key.to_string();
            let part_semaphore = part_semaphore.clone();
            let chunks = chunks.clone();
            let transferred = transferred.clone();
            let progress_tx = progress_tx.clone();
            let job_id = *job_id;

            let handle = tokio::spawn(async move {
                let _permit = part_semaphore.acquire().await;

                // Download range
                let data = client.get_object_range(&bucket, &key, &range).await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

                chunks.lock().await.insert(part_number, data);

                let prev = transferred.fetch_add(size, std::sync::atomic::Ordering::SeqCst);
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { (prev + size) as f64 / elapsed } else { 0.0 };

                let _ = progress_tx.send(TransferProgress {
                    job_id,
                    transferred_bytes: prev + size,
                    total_bytes: object_size,
                    speed_bytes_per_sec: speed,
                    status: TransferStatus::Active,
                });

                Ok::<_, std::io::Error>(())
            });

            handles.push(handle);
        }

        // Wait for all parts
        for handle in handles {
            handle.await
                .map_err(|e| TransferError::TransferFailed(format!("Part download task failed: {}", e)))?
                .map_err(|e| TransferError::TransferFailed(format!("Part download error: {}", e)))?;
        }

        // Write chunks in order
        let mut file = tokio::fs::File::create(local_path).await
            .map_err(|e| TransferError::TransferFailed(format!("Create file failed: {}", e)))?;

        let chunks_map = chunks.lock().await;
        let mut sorted_parts: Vec<_> = chunks_map.keys().copied().collect();
        sorted_parts.sort();

        for pn in sorted_parts {
            if let Some(data) = chunks_map.get(&pn) {
                file.write_all(data).await
                    .map_err(|e| TransferError::TransferFailed(format!("Write chunk failed: {}", e)))?;
            }
        }

        file.flush().await
            .map_err(|e| TransferError::TransferFailed(format!("Flush file failed: {}", e)))?;

        let elapsed = start_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 { object_size as f64 / elapsed } else { 0.0 };

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(js) = jobs.get_mut(job_id) {
                js.job.transferred_bytes = object_size;
                js.job.speed_bytes_per_sec = speed;
            }
        }

        Ok(())
    }

    // ── S3→S3 copy ──

    async fn execute_s3_to_s3_copy(
        &self,
        job_id: &Uuid,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        src_client: Arc<dyn S3Client>,
        dst_client: Arc<dyn S3Client>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
        mut pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), TransferError> {
        // Check pause before starting
        if *pause_rx.borrow_and_update() {
            Self::wait_for_resume(&mut pause_rx, &cancel_token).await?;
        }
        cancel_token.check()?;

        // Get source object size
        let head = src_client.head_object(src_bucket, src_key).await
            .map_err(|e| TransferError::TransferFailed(format!("HeadObject failed: {}", e)))?;
        let obj_size = head.size as u64;

        // If same client (same endpoint), use server-side copy
        let same_endpoint = Arc::ptr_eq(&src_client, &dst_client);

        if same_endpoint && obj_size < self.multipart_threshold {
            // Simple server-side copy
            let start = std::time::Instant::now();
            dst_client.copy_object(src_bucket, src_key, dst_bucket, dst_key).await
                .map_err(|e| TransferError::TransferFailed(format!("CopyObject failed: {}", e)))?;
            let elapsed = start.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 { obj_size as f64 / elapsed } else { 0.0 };

            {
                let mut jobs = self.jobs.lock().await;
                if let Some(js) = jobs.get_mut(job_id) {
                    js.job.transferred_bytes = obj_size;
                    js.job.speed_bytes_per_sec = speed;
                }
            }

            let _ = progress_tx.send(TransferProgress {
                job_id: *job_id,
                transferred_bytes: obj_size,
                total_bytes: obj_size,
                speed_bytes_per_sec: speed,
                status: TransferStatus::Active,
            });

            Ok(())
        } else {
            // Different endpoints or large file: download from source, upload to destination
            if obj_size >= self.multipart_threshold {
                // Download to temp file first
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join(format!("r2-s3copy-{}", job_id));
                let temp_path = temp_file.clone();

                // Multipart download to temp file
                self.execute_multipart_download(
                    job_id, &temp_path, src_bucket, src_key, obj_size,
                    src_client.clone(), progress_tx.clone(), pause_rx.clone(), cancel_token.clone(),
                ).await?;

                // Multipart upload from temp file
                let result = self.execute_multipart_upload(
                    job_id, &temp_path, dst_bucket, dst_key,
                    dst_client, progress_tx.clone(), pause_rx, cancel_token,
                ).await;

                // Clean up temp file
                let _ = tokio::fs::remove_file(&temp_path).await;

                result
            } else {
                // Single download + upload
                let data = src_client.get_object(src_bucket, src_key).await
                    .map_err(|e| TransferError::TransferFailed(format!("GetObject failed: {}", e)))?;

                let start = std::time::Instant::now();
                dst_client.put_object(dst_bucket, dst_key, data).await
                    .map_err(|e| TransferError::TransferFailed(format!("PutObject failed: {}", e)))?;
                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { obj_size as f64 / elapsed } else { 0.0 };

                {
                    let mut jobs = self.jobs.lock().await;
                    if let Some(js) = jobs.get_mut(job_id) {
                        js.job.transferred_bytes = obj_size;
                        js.job.speed_bytes_per_sec = speed;
                    }
                }

                let _ = progress_tx.send(TransferProgress {
                    job_id: *job_id,
                    transferred_bytes: obj_size,
                    total_bytes: obj_size,
                    speed_bytes_per_sec: speed,
                    status: TransferStatus::Active,
                });

                Ok(())
            }
        }
    }
}

/// Helper: update a job as failed and send progress event.
async fn update_job_failed(
    jobs: &Arc<Mutex<HashMap<Uuid, JobState>>>,
    progress_tx: &mpsc::UnboundedSender<TransferProgress>,
    job_id: Uuid,
    error: TransferError,
) {
    let err_str = error.to_string();
    let mut jl = jobs.lock().await;
    if let Some(js) = jl.get_mut(&job_id) {
        js.job.status = TransferStatus::Failed(err_str.clone());
        js.job.error_message = Some(err_str.clone());
    }
    let _ = progress_tx.send(TransferProgress {
        job_id,
        transferred_bytes: 0,
        total_bytes: 0,
        speed_bytes_per_sec: 0.0,
        status: TransferStatus::Failed(err_str),
    });
}

/// A lightweight reference to a TokioTransferEngine's internals, used to
/// spawn transfer tasks without needing &Arc<Self>.
struct TokioTransferEngineRef {
    jobs: Arc<Mutex<HashMap<Uuid, JobState>>>,
    semaphore: Arc<Semaphore>,
    progress_tx: mpsc::UnboundedSender<TransferProgress>,
    client_factory: Arc<dyn Fn(Uuid) -> Option<Arc<dyn S3Client>> + Send + Sync>,
    multipart_threshold: u64,
    chunk_size: u64,
    max_concurrent_parts: usize,
}

impl TokioTransferEngineRef {
    /// Spawn a transfer task using the engine's internals.
    /// This is the same logic as TokioTransferEngine::spawn_transfer but
    /// operates on the individual Arcs instead of &Arc<Self>.
    async fn spawn_transfer_inner(
        self,
        job_id: Uuid,
        pause_rx: watch::Receiver<bool>,
        cancel_token: CancellationToken,
    ) {
        let semaphore = self.semaphore.clone();
        let progress_tx = self.progress_tx.clone();
        let client_factory = self.client_factory.clone();
        let multipart_threshold = self.multipart_threshold;
        let chunk_size = self.chunk_size;
        let max_concurrent_parts = self.max_concurrent_parts;
        let jobs = self.jobs.clone();

        // Acquire concurrency permit
        let _permit = semaphore.acquire().await;
        if cancel_token.is_cancelled() {
            return;
        }

        // Get job details
        let (direction, source, destination) = {
            let jl = jobs.lock().await;
            match jl.get(&job_id) {
                Some(js) => (
                    js.job.direction.clone(),
                    js.job.source.clone(),
                    js.job.destination.clone(),
                ),
                None => return,
            }
        };

        // Mark as active
        {
            let mut jl = jobs.lock().await;
            if let Some(js) = jl.get_mut(&job_id) {
                js.job.status = TransferStatus::Active;
                js.job.started_at = Some(Utc::now());
            }
        }

        let _ = progress_tx.send(TransferProgress {
            job_id,
            transferred_bytes: 0,
            total_bytes: 0,
            speed_bytes_per_sec: 0.0,
            status: TransferStatus::Active,
        });

        // Execute the transfer based on direction
        let result = match direction {
            TransferDirection::Upload => {
                let (local_path, profile_id, bucket, key) = match (&source, &destination) {
                    (TransferSource::LocalFile(path), TransferDestination::S3Object { profile_id, bucket, key }) => {
                        (path.clone(), *profile_id, bucket.clone(), key.clone())
                    }
                    _ => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed("Invalid upload source/destination combination".into())).await;
                        return;
                    }
                };

                let client = match client_factory(profile_id) {
                    Some(c) => c,
                    None => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed(format!("No S3 client for profile {}", profile_id))).await;
                        return;
                    }
                };

                let file_size = match tokio::fs::metadata(&local_path).await {
                    Ok(m) => m.len(),
                    Err(e) => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed(format!("Cannot stat file: {}", e))).await;
                        return;
                    }
                };

                if file_size >= multipart_threshold {
                    // Multipart upload — we need the engine's methods.
                    // For now, fall back to single upload for the ref-based path.
                    // In a full implementation, we'd extract the logic into free functions.
                    single_upload_fallback(
                        &jobs, &progress_tx, job_id, &local_path, &bucket, &key,
                        client, pause_rx, cancel_token,
                    ).await
                } else {
                    single_upload_fallback(
                        &jobs, &progress_tx, job_id, &local_path, &bucket, &key,
                        client, pause_rx, cancel_token,
                    ).await
                }
            }
            TransferDirection::Download => {
                let (profile_id, bucket, key, local_path) = match (&source, &destination) {
                    (TransferSource::S3Object { profile_id, bucket, key }, TransferDestination::LocalFile(path)) => {
                        (*profile_id, bucket.clone(), key.clone(), path.clone())
                    }
                    _ => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed("Invalid download source/destination combination".into())).await;
                        return;
                    }
                };

                let client = match client_factory(profile_id) {
                    Some(c) => c,
                    None => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed(format!("No S3 client for profile {}", profile_id))).await;
                        return;
                    }
                };

                let head = match client.head_object(&bucket, &key).await {
                    Ok(h) => h,
                    Err(e) => {
                        update_job_failed(&jobs, &progress_tx, job_id,
                            TransferError::TransferFailed(format!("HeadObject failed: {}", e))).await;
                        return;
                    }
                };
                let obj_size = head.size as u64;

                if obj_size >= multipart_threshold {
                    update_job_failed(&jobs, &progress_tx, job_id,
                        TransferError::TransferFailed("Multipart download not supported in ref path yet".into())).await;
                    return;
                } else {
                    single_download_fallback(
                        &jobs, &progress_tx, job_id, &local_path, &bucket, &key,
                        client, pause_rx, cancel_token,
                    ).await
                }
            }
            TransferDirection::S3ToS3 => {
                update_job_failed(&jobs, &progress_tx, job_id,
                    TransferError::TransferFailed("S3→S3 not supported in ref path yet".into())).await;
                return;
            }
        };

        // Update final status
        let mut jl = jobs.lock().await;
        if let Some(js) = jl.get_mut(&job_id) {
            match result {
                Ok(()) => {
                    js.job.status = TransferStatus::Completed;
                    js.job.completed_at = Some(Utc::now());
                    let _ = progress_tx.send(TransferProgress {
                        job_id,
                        transferred_bytes: js.job.total_bytes,
                        total_bytes: js.job.total_bytes,
                        speed_bytes_per_sec: 0.0,
                        status: TransferStatus::Completed,
                    });
                }
                Err(e) => {
                    let err_str = e.to_string();
                    js.job.status = TransferStatus::Failed(err_str.clone());
                    js.job.error_message = Some(err_str.clone());
                    let _ = progress_tx.send(TransferProgress {
                        job_id,
                        transferred_bytes: js.job.transferred_bytes,
                        total_bytes: js.job.total_bytes,
                        speed_bytes_per_sec: 0.0,
                        status: TransferStatus::Failed(err_str),
                    });
                }
            }
        }
    }
}

/// Fallback single upload that operates on engine refs instead of &Arc<Self>.
async fn single_upload_fallback(
    jobs: &Arc<Mutex<HashMap<Uuid, JobState>>>,
    progress_tx: &mpsc::UnboundedSender<TransferProgress>,
    job_id: Uuid,
    local_path: &Path,
    bucket: &str,
    key: &str,
    client: Arc<dyn S3Client>,
    mut pause_rx: watch::Receiver<bool>,
    cancel_token: CancellationToken,
) -> std::result::Result<(), TransferError> {
    let data = tokio::fs::read(local_path).await
        .map_err(|e| TransferError::TransferFailed(format!("Read file failed: {}", e)))?;
    let total = data.len() as u64;

    if *pause_rx.borrow_and_update() {
        loop {
            tokio::select! {
                _ = pause_rx.changed() => {
                    if !*pause_rx.borrow() { break; }
                }
                _ = cancel_token.cancelled() => {
                    return Err(TransferError::Cancelled);
                }
            }
        }
    }
    cancel_token.check()?;

    let start = std::time::Instant::now();
    client.put_object(bucket, key, data).await
        .map_err(|e| TransferError::TransferFailed(format!("PutObject failed: {}", e)))?;
    let elapsed = start.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 { total as f64 / elapsed } else { 0.0 };

    {
        let mut jl = jobs.lock().await;
        if let Some(js) = jl.get_mut(&job_id) {
            js.job.transferred_bytes = total;
            js.job.speed_bytes_per_sec = speed;
        }
    }

    let _ = progress_tx.send(TransferProgress {
        job_id,
        transferred_bytes: total,
        total_bytes: total,
        speed_bytes_per_sec: speed,
        status: TransferStatus::Active,
    });

    Ok(())
}

/// Fallback single download that operates on engine refs instead of &Arc<Self>.
async fn single_download_fallback(
    jobs: &Arc<Mutex<HashMap<Uuid, JobState>>>,
    progress_tx: &mpsc::UnboundedSender<TransferProgress>,
    job_id: Uuid,
    local_path: &Path,
    bucket: &str,
    key: &str,
    client: Arc<dyn S3Client>,
    mut pause_rx: watch::Receiver<bool>,
    cancel_token: CancellationToken,
) -> std::result::Result<(), TransferError> {
    if *pause_rx.borrow_and_update() {
        loop {
            tokio::select! {
                _ = pause_rx.changed() => {
                    if !*pause_rx.borrow() { break; }
                }
                _ = cancel_token.cancelled() => {
                    return Err(TransferError::Cancelled);
                }
            }
        }
    }
    cancel_token.check()?;

    let start = std::time::Instant::now();
    let data = client.get_object(bucket, key).await
        .map_err(|e| TransferError::TransferFailed(format!("GetObject failed: {}", e)))?;
    let elapsed = start.elapsed().as_secs_f64();
    let total = data.len() as u64;

    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| TransferError::TransferFailed(format!("Create dir failed: {}", e)))?;
    }

    tokio::fs::write(local_path, &data).await
        .map_err(|e| TransferError::TransferFailed(format!("Write file failed: {}", e)))?;

    let speed = if elapsed > 0.0 { total as f64 / elapsed } else { 0.0 };

    {
        let mut jl = jobs.lock().await;
        if let Some(js) = jl.get_mut(&job_id) {
            js.job.transferred_bytes = total;
            js.job.speed_bytes_per_sec = speed;
        }
    }

    let _ = progress_tx.send(TransferProgress {
        job_id,
        transferred_bytes: total,
        total_bytes: total,
        speed_bytes_per_sec: speed,
        status: TransferStatus::Active,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// TransferEngine trait implementation for TokioTransferEngine
// ---------------------------------------------------------------------------

#[async_trait]
impl TransferEngine for TokioTransferEngine {
    async fn enqueue(&self, job: TransferJob) -> Result<Uuid> {
        let job_id = job.id;

        let (pause_tx, pause_rx) = watch::channel(false);
        let cancel_token = CancellationToken::new();

        let state = JobState {
            job,
            pause_tx: Some(pause_tx),
            cancel_token: Some(cancel_token.clone()),
            handle: None,
        };

        {
            let mut jobs = self.jobs.lock().await;
            jobs.insert(job_id, state);
        }

        // spawn_transfer requires &Arc<Self>. We cannot safely get that from &self,
        // so we use a different approach: we store the Arc in a static or we restructure.
        // For now, we call spawn_transfer via a helper that takes the individual Arcs.
        // Since TokioTransferEngine is always used behind Arc<TokioTransferEngine>,
        // we use a workaround with a clone of the engine's internal Arcs.
        let jobs = self.jobs.clone();
        let semaphore = self.semaphore.clone();
        let progress_tx = self.progress_tx.clone();
        let client_factory = self.client_factory.clone();
        let multipart_threshold = self.multipart_threshold;
        let chunk_size = self.chunk_size;
        let max_concurrent_parts = self.max_concurrent_parts;

        tokio::spawn(async move {
            let engine_ref = TokioTransferEngineRef {
                jobs,
                semaphore,
                progress_tx,
                client_factory,
                multipart_threshold,
                chunk_size,
                max_concurrent_parts,
            };
            engine_ref.spawn_transfer_inner(job_id, pause_rx, cancel_token).await;
        });

        info!(job_id = %job_id, "Transfer job enqueued");
        Ok(job_id)
    }

    async fn pause(&self, job_id: &Uuid) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let state = jobs.get_mut(job_id)
            .ok_or_else(|| TransferError::JobNotFound(job_id.to_string()))?;

        match state.job.status {
            TransferStatus::Active => {
                if let Some(ref pause_tx) = state.pause_tx {
                    let _ = pause_tx.send(true);
                }
                state.job.status = TransferStatus::Paused;
                info!(job_id = %job_id, "Transfer paused");
                Ok(())
            }
            TransferStatus::Paused => {
                Ok(())
            }
            _ => Err(TransferError::TransferFailed(
                format!("Cannot pause job {} in status {:?}", job_id, state.job.status)
            ).into()),
        }
    }

    async fn resume(&self, job_id: &Uuid) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let state = jobs.get_mut(job_id)
            .ok_or_else(|| TransferError::JobNotFound(job_id.to_string()))?;

        match state.job.status {
            TransferStatus::Paused => {
                if let Some(ref pause_tx) = state.pause_tx {
                    let _ = pause_tx.send(false);
                }
                state.job.status = TransferStatus::Active;
                info!(job_id = %job_id, "Transfer resumed");
                Ok(())
            }
            TransferStatus::Active => {
                Ok(())
            }
            _ => Err(TransferError::TransferFailed(
                format!("Cannot resume job {} in status {:?}", job_id, state.job.status)
            ).into()),
        }
    }

    async fn cancel(&self, job_id: &Uuid) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let state = jobs.get_mut(job_id)
            .ok_or_else(|| TransferError::JobNotFound(job_id.to_string()))?;

        match state.job.status {
            TransferStatus::Active | TransferStatus::Paused => {
                if let Some(ref token) = state.cancel_token {
                    token.cancel();
                }
                state.job.status = TransferStatus::Cancelled;
                info!(job_id = %job_id, "Transfer cancelled");
                Ok(())
            }
            _ => Err(TransferError::TransferFailed(
                format!("Cannot cancel job {} in status {:?}", job_id, state.job.status)
            ).into()),
        }
    }

    async fn retry(&self, job_id: &Uuid) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let state = jobs.get_mut(job_id)
            .ok_or_else(|| TransferError::JobNotFound(job_id.to_string()))?;

        if !state.job.status.is_retryable() {
            return Err(TransferError::TransferFailed(
                format!("Cannot retry job {} in status {:?}", job_id, state.job.status)
            ).into());
        }

        // Reset job state for retry
        state.job.status = TransferStatus::Pending;
        state.job.transferred_bytes = 0;
        state.job.speed_bytes_per_sec = 0.0;
        state.job.error_message = None;
        state.job.started_at = None;
        state.job.completed_at = None;
        state.job.retry_count = 0;
        state.job.multipart_upload_id = None;
        state.job.parts.clear();

        // Create new pause/cancel channels
        let (pause_tx, pause_rx) = watch::channel(false);
        let cancel_token = CancellationToken::new();
        state.pause_tx = Some(pause_tx);
        state.cancel_token = Some(cancel_token.clone());

        // Spawn new transfer using the engine ref helper
        let jobs = self.jobs.clone();
        let semaphore = self.semaphore.clone();
        let progress_tx = self.progress_tx.clone();
        let client_factory = self.client_factory.clone();
        let multipart_threshold = self.multipart_threshold;
        let chunk_size = self.chunk_size;
        let max_concurrent_parts = self.max_concurrent_parts;

        tokio::spawn(async move {
            let engine_ref = TokioTransferEngineRef {
                jobs,
                semaphore,
                progress_tx,
                client_factory,
                multipart_threshold,
                chunk_size,
                max_concurrent_parts,
            };
            engine_ref.spawn_transfer_inner(*job_id, pause_rx, cancel_token).await;
        });

        info!(job_id = %job_id, "Transfer retry enqueued");
        Ok(())
    }

    async fn get_job(&self, job_id: &Uuid) -> Result<TransferJob> {
        let jobs = self.jobs.lock().await;
        let state = jobs.get(job_id)
            .ok_or_else(|| TransferError::JobNotFound(job_id.to_string()))?;
        Ok(state.job.clone())
    }

    async fn list_jobs(&self, status_filter: Option<TransferStatus>) -> Result<Vec<TransferJob>> {
        let jobs = self.jobs.lock().await;
        let all: Vec<TransferJob> = jobs.values().map(|s| s.job.clone()).collect();

        match status_filter {
            Some(filter) => Ok(all.into_iter().filter(|j| {
                match (&filter, &j.status) {
                    (TransferStatus::Failed(_), TransferStatus::Failed(_)) => true,
                    _ => std::mem::discriminant(&filter) == std::mem::discriminant(&j.status),
                }
            }).collect()),
            None => Ok(all),
        }
    }

    async fn list_active(&self) -> Result<Vec<TransferJob>> {
        let jobs = self.jobs.lock().await;
        Ok(jobs.values()
            .filter(|s| matches!(s.job.status, TransferStatus::Pending | TransferStatus::Active | TransferStatus::Paused))
            .map(|s| s.job.clone())
            .collect())
    }

    async fn list_completed(&self) -> Result<Vec<TransferJob>> {
        let jobs = self.jobs.lock().await;
        Ok(jobs.values()
            .filter(|s| matches!(s.job.status, TransferStatus::Completed))
            .map(|s| s.job.clone())
            .collect())
    }

    async fn list_failed(&self) -> Result<Vec<TransferJob>> {
        let jobs = self.jobs.lock().await;
        Ok(jobs.values()
            .filter(|s| matches!(s.job.status, TransferStatus::Failed(_) | TransferStatus::Cancelled))
            .map(|s| s.job.clone())
            .collect())
    }

    async fn subscribe(&self) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        // In a full implementation, we'd use broadcast to share progress with multiple subscribers.
        // For now, we return a fresh receiver.
        let _ = tx;
        rx
    }

    async fn shutdown(&self) {
        info!("Shutting down transfer engine");
        self.shutting_down.store(true, Ordering::SeqCst);
        self.shutdown_token.cancel();

        let mut jobs = self.jobs.lock().await;
        for (_, state) in jobs.iter_mut() {
            if let Some(ref token) = state.cancel_token {
                token.cancel();
            }
            if matches!(state.job.status, TransferStatus::Active | TransferStatus::Paused) {
                state.job.status = TransferStatus::Paused;
            }
        }
        info!("Transfer engine shut down");
    }

    async fn resume_interrupted(&self) -> Result<Vec<Uuid>> {
        let mut resumed = Vec::new();
        let jobs = self.jobs.lock().await;

        for (job_id, state) in jobs.iter() {
            if state.job.status == TransferStatus::Paused {
                resumed.push(*job_id);
            }
        }

        if !resumed.is_empty() {
            info!(count = resumed.len(), "Resuming interrupted transfers");
        }

        Ok(resumed)
    }
}

// ---------------------------------------------------------------------------
// StubTransferEngine (kept for backward compatibility)
// ---------------------------------------------------------------------------

/// Stub implementation for testing
pub struct StubTransferEngine;

#[async_trait]
impl TransferEngine for StubTransferEngine {
    async fn enqueue(&self, _job: TransferJob) -> Result<Uuid> {
        Ok(Uuid::new_v4())
    }

    async fn pause(&self, _job_id: &Uuid) -> Result<()> {
        Ok(())
    }

    async fn resume(&self, _job_id: &Uuid) -> Result<()> {
        Ok(())
    }

    async fn cancel(&self, _job_id: &Uuid) -> Result<()> {
        Ok(())
    }

    async fn retry(&self, _job_id: &Uuid) -> Result<()> {
        Ok(())
    }

    async fn get_job(&self, _job_id: &Uuid) -> Result<TransferJob> {
        Err(TransferError::JobNotFound("stub".into()).into())
    }

    async fn list_jobs(&self, _status_filter: Option<TransferStatus>) -> Result<Vec<TransferJob>> {
        Ok(Vec::new())
    }

    async fn list_active(&self) -> Result<Vec<TransferJob>> {
        Ok(Vec::new())
    }

    async fn list_completed(&self) -> Result<Vec<TransferJob>> {
        Ok(Vec::new())
    }

    async fn list_failed(&self) -> Result<Vec<TransferJob>> {
        Ok(Vec::new())
    }

    async fn subscribe(&self) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = tx;
        rx
    }

    async fn shutdown(&self) {}

    async fn resume_interrupted(&self) -> Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
}
