//! r2-core — S3Client trait and AWS SDK implementation

use async_trait::async_trait;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{
    BucketVersioningStatus, Grantee as AwsGrantee, Permission, Tag,
};
use aws_sdk_s3::Client as AwsClient;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tracing::{debug, info, warn};

use super::types::{
    AclGrant, BucketInfo, Grantee, ObjectInfo, ObjectVersion, S3ClientConfig,
};
use crate::error::{Result, S3Error};

/// S3Client trait — all S3 operations are async
#[async_trait]
pub trait S3Client: Send + Sync {
    /// List all buckets for the connected profile
    async fn list_buckets(&self) -> Result<Vec<BucketInfo>>;

    /// Create a new bucket
    async fn create_bucket(&self, name: &str) -> Result<()>;

    /// Delete a bucket
    async fn delete_bucket(&self, name: &str) -> Result<()>;

    /// Get bucket metadata
    async fn head_bucket(&self, name: &str) -> Result<BucketInfo>;

    /// List objects in a bucket/prefix (paginated)
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: i32,
        start_after: Option<String>,
    ) -> Result<Vec<ObjectInfo>>;

    /// Get object metadata
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectInfo>;

    /// Delete an object
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;

    /// Copy an object within the same endpoint
    async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        dest_bucket: &str,
        dest_key: &str,
    ) -> Result<()>;

    // --- Multipart ---

    /// Initialize a multipart upload
    async fn create_multipart_upload(&self, bucket: &str, key: &str) -> Result<String>;

    /// Upload a part for a multipart upload
    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: i32,
        body: Vec<u8>,
    ) -> Result<String>;

    /// Complete a multipart upload
    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()>;

    /// Abort a multipart upload
    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<()>;

    /// Upload an object (PUT)
    async fn put_object(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<()>;

    /// Get an object (GET)
    async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>>;

    /// Get a byte range of an object (GET with Range header)
    async fn get_object_range(&self, bucket: &str, key: &str, range: &str) -> Result<Vec<u8>>;

    // --- Versioning ---

    /// Get the versioning status of a bucket
    async fn get_bucket_versioning(&self, bucket: &str) -> Result<Option<String>>;

    /// Enable or suspend versioning on a bucket
    async fn set_bucket_versioning(&self, bucket: &str, status: &str) -> Result<()>;

    /// List all versions of objects in a bucket/prefix
    async fn list_object_versions(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectVersion>>;

    /// Get a specific version of an object
    async fn get_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Vec<u8>>;

    /// Delete a specific version of an object
    async fn delete_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<()>;

    /// Restore a specific version by copying it to the current version
    async fn restore_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<()>;

    // --- ACL ---

    /// Get the ACL of a bucket
    async fn get_bucket_acl(&self, bucket: &str) -> Result<Vec<AclGrant>>;

    /// Set the ACL of a bucket
    async fn set_bucket_acl(&self, bucket: &str, grants: &[AclGrant]) -> Result<()>;

    /// Get the ACL of an object
    async fn get_object_acl(&self, bucket: &str, key: &str) -> Result<Vec<AclGrant>>;

    /// Set the ACL of an object
    async fn set_object_acl(&self, bucket: &str, key: &str, grants: &[AclGrant]) -> Result<()>;
}

/// AWS SDK-based S3 client implementation
pub struct AwsSdkS3Client {
    client: AwsClient,
    #[allow(dead_code)]
    config: S3ClientConfig,
}

impl AwsSdkS3Client {
    /// Create a new AwsSdkS3Client from configuration
    pub async fn new(config: S3ClientConfig) -> Result<Self> {
        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
            .operation_timeout(Duration::from_secs(config.operation_timeout_secs))
            .build();

        let creds = Credentials::new(
            &config.access_key,
            &config.secret_key,
            None,
            None,
            "r2",
        );

        let region = Region::new(config.region.clone());

        let mut s3_config_builder = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(region)
            .credentials_provider(creds)
            .timeout_config(timeout_config)
            .force_path_style(config.path_style);

        // Set custom endpoint if provided
        if !config.endpoint_url.is_empty() {
            s3_config_builder = s3_config_builder.endpoint_url(&config.endpoint_url);
        }

        let s3_config = s3_config_builder.build();
        let client = AwsClient::from_conf(s3_config);

        info!(
            endpoint = %config.endpoint_url,
            region = %config.region,
            path_style = %config.path_style,
            "S3 client created"
        );

        Ok(Self { client, config })
    }

    /// Test the connection by listing buckets
    pub async fn test_connection(&self) -> Result<bool> {
        match self.client.list_buckets().send().await {
            Ok(_) => {
                info!("S3 connection test successful");
                Ok(true)
            }
            Err(e) => {
                let err_msg = format!("Connection test failed: {}", e);
                warn!("{}", err_msg);
                Err(S3Error::NetworkError(err_msg).into())
            }
        }
    }

    /// Map an AWS SDK error to our S3Error type
    fn map_sdk_error(err: &aws_sdk_s3::Error) -> S3Error {
        let err_str = err.to_string();
        if err_str.contains("NoSuchBucket") {
            S3Error::BucketNotFound(err_str)
        } else if err_str.contains("NoSuchKey") {
            S3Error::ObjectNotFound(err_str)
        } else if err_str.contains("AccessDenied") {
            S3Error::AccessDenied(err_str)
        } else if err_str.contains("timeout") || err_str.contains("Timeout") {
            S3Error::Timeout(120)
        } else {
            S3Error::Aws(err_str)
        }
    }
}

#[async_trait]
impl S3Client for AwsSdkS3Client {
    // ── Bucket operations ──

    async fn list_buckets(&self) -> Result<Vec<BucketInfo>> {
        debug!("Listing buckets");
        let output = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let buckets: Vec<BucketInfo> = output
            .buckets()
            .iter()
            .map(|b| BucketInfo {
                name: b.name().unwrap_or_default().to_string(),
                creation_date: b.creation_date().map(|d| aws_datetime_to_chrono(d)),
            })
            .collect();

        info!(count = buckets.len(), "Buckets listed");
        Ok(buckets)
    }

    async fn create_bucket(&self, name: &str) -> Result<()> {
        debug!(bucket = %name, "Creating bucket");
        self.client
            .create_bucket()
            .bucket(name)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %name, "Bucket created");
        Ok(())
    }

    async fn delete_bucket(&self, name: &str) -> Result<()> {
        debug!(bucket = %name, "Deleting bucket");
        self.client
            .delete_bucket()
            .bucket(name)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %name, "Bucket deleted");
        Ok(())
    }

    async fn head_bucket(&self, name: &str) -> Result<BucketInfo> {
        debug!(bucket = %name, "Head bucket");
        let _output = self
            .client
            .head_bucket()
            .bucket(name)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        Ok(BucketInfo {
            name: name.to_string(),
            creation_date: None,
        })
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: i32,
        start_after: Option<String>,
    ) -> Result<Vec<ObjectInfo>> {
        debug!(
            bucket = %bucket,
            prefix = %prefix,
            delimiter = %delimiter,
            max_keys = %max_keys,
            "Listing objects"
        );

        let mut req = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(max_keys);

        if !delimiter.is_empty() {
            req = req.delimiter(delimiter);
        }
        if let Some(token) = start_after {
            req = req.start_after(token);
        }

        let output = req.send().await.map_err(|e| Self::map_sdk_error(&e.into()))?;

        let mut objects = Vec::new();

        // Process common prefixes (directories)
        for cp in output.common_prefixes() {
            if let Some(p) = cp.prefix() {
                objects.push(ObjectInfo {
                    key: p.to_string(),
                    size: 0,
                    last_modified: None,
                    e_tag: None,
                    storage_class: None,
                    is_prefix: true,
                });
            }
        }

        // Process objects (files)
        for obj in output.contents() {
            objects.push(ObjectInfo {
                key: obj.key().unwrap_or_default().to_string(),
                size: obj.size().unwrap_or(0),
                last_modified: obj.last_modified().map(|d| aws_datetime_to_chrono(d)),
                e_tag: obj.e_tag().map(|s| s.to_string()),
                storage_class: obj.storage_class().map(|s| s.as_str().to_string()),
                is_prefix: false,
            });
        }

        info!(
            bucket = %bucket,
            prefix = %prefix,
            count = objects.len(),
            "Objects listed"
        );
        Ok(objects)
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectInfo> {
        debug!(bucket = %bucket, key = %key, "Head object");
        let output = self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        Ok(ObjectInfo {
            key: key.to_string(),
            size: output.content_length().unwrap_or(0),
            last_modified: output.last_modified().map(|d| aws_datetime_to_chrono(d)),
            e_tag: output.e_tag().map(|s| s.to_string()),
            storage_class: output.storage_class().map(|s| s.as_str().to_string()),
            is_prefix: false,
        })
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        debug!(bucket = %bucket, key = %key, "Deleting object");
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %bucket, key = %key, "Object deleted");
        Ok(())
    }

    async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        dest_bucket: &str,
        dest_key: &str,
    ) -> Result<()> {
        let copy_source = format!("{}/{}", source_bucket, source_key);
        debug!(
            source = %copy_source,
            dest_bucket = %dest_bucket,
            dest_key = %dest_key,
            "Copying object"
        );

        self.client
            .copy_object()
            .copy_source(&copy_source)
            .bucket(dest_bucket)
            .key(dest_key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(
            source = %copy_source,
            dest = %format!("{}/{}", dest_bucket, dest_key),
            "Object copied"
        );
        Ok(())
    }

    async fn create_multipart_upload(&self, bucket: &str, key: &str) -> Result<String> {
        debug!(bucket = %bucket, key = %key, "Creating multipart upload");
        let output = self
            .client
            .create_multipart_upload()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let upload_id = output.upload_id().unwrap_or_default().to_string();

        info!(
            bucket = %bucket,
            key = %key,
            upload_id = %upload_id,
            "Multipart upload created"
        );
        Ok(upload_id)
    }

    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: i32,
        body: Vec<u8>,
    ) -> Result<String> {
        debug!(
            bucket = %bucket,
            key = %key,
            part = %part_number,
            "Uploading part"
        );

        let byte_stream = ByteStream::from(body);
        let output = self
            .client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(byte_stream)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let e_tag = output.e_tag().unwrap_or_default().to_string();
        debug!(part = %part_number, e_tag = %e_tag, "Part uploaded");
        Ok(e_tag)
    }

    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()> {
        debug!(
            bucket = %bucket,
            key = %key,
            part_count = parts.len(),
            "Completing multipart upload"
        );

        let completed_parts: Vec<aws_sdk_s3::types::CompletedPart> = parts
            .into_iter()
            .map(|(part_number, e_tag)| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(&e_tag)
                    .build()
            })
            .collect();

        let completed_multipart = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed_multipart)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(bucket = %bucket, key = %key, "Multipart upload completed");
        Ok(())
    }

    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<()> {
        debug!(
            bucket = %bucket,
            key = %key,
            upload_id = %upload_id,
            "Aborting multipart upload"
        );

        self.client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(bucket = %bucket, key = %key, "Multipart upload aborted");
        Ok(())
    }

    async fn put_object(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<()> {
        debug!(bucket = %bucket, key = %key, size = data.len(), "PutObject");
        let byte_stream = ByteStream::from(data);
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(byte_stream)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %bucket, key = %key, "Object uploaded");
        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>> {
        debug!(bucket = %bucket, key = %key, "GetObject");
        let output = self.client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let data = output.body.collect().await
            .map_err(|e| S3Error::Aws(format!("Failed to read body: {}", e)))?;
        Ok(data.to_vec())
    }

    async fn get_object_range(&self, bucket: &str, key: &str, range: &str) -> Result<Vec<u8>> {
        debug!(bucket = %bucket, key = %key, range = %range, "GetObject with range");
        let output = self.client
            .get_object()
            .bucket(bucket)
            .key(key)
            .range(range)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let data = output.body.collect().await
            .map_err(|e| S3Error::Aws(format!("Failed to read body: {}", e)))?;
        Ok(data.to_vec())
    }
}

    // ── Versioning ──

    async fn get_bucket_versioning(&self, bucket: &str) -> Result<Option<String>> {
        debug!(bucket = %bucket, "Get bucket versioning");
        match self.client.get_bucket_versioning().bucket(bucket).send().await {
            Ok(output) => {
                let status = output.status().map(|s| s.as_str().to_string());
                info!(bucket = %bucket, status = ?status, "Bucket versioning status retrieved");
                Ok(status)
            }
            Err(e) => {
                let err = Self::map_sdk_error(&e.into());
                // Some backends don't support versioning — return None instead of error
                if matches!(err, S3Error::Aws(_)) && e.to_string().contains("NotImplemented") {
                    warn!(bucket = %bucket, "Versioning not implemented by this backend");
                    Ok(None)
                } else {
                    Err(err.into())
                }
            }
        }
    }

    async fn set_bucket_versioning(&self, bucket: &str, status: &str) -> Result<()> {
        debug!(bucket = %bucket, status = %status, "Set bucket versioning");
        let versioning_status = match status {
            "Enabled" => BucketVersioningStatus::Enabled,
            "Suspended" => BucketVersioningStatus::Suspended,
            _ => return Err(S3Error::Aws(format!("Invalid versioning status: {}", status)).into()),
        };

        self.client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(
                aws_sdk_s3::types::VersioningConfiguration::builder()
                    .status(versioning_status)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(bucket = %bucket, status = %status, "Bucket versioning updated");
        Ok(())
    }

    async fn list_object_versions(&self, bucket: &str, prefix: &str) -> Result<Vec<ObjectVersion>> {
        debug!(bucket = %bucket, prefix = %prefix, "List object versions");

        let mut versions = Vec::new();
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_object_versions()
                .bucket(bucket)
                .prefix(prefix)
                .max_keys(200);

            if let Some(ref km) = key_marker {
                req = req.key_marker(km);
            }
            if let Some(ref vm) = version_id_marker {
                req = req.version_id_marker(vm);
            }

            let output = req.send().await.map_err(|e| Self::map_sdk_error(&e.into()))?;

            for v in output.versions() {
                if let Some(key) = v.key() {
                    let last_modified = v.last_modified()
                        .map(|d| aws_datetime_to_chrono(d))
                        .unwrap_or_else(|| Utc::now());

                    versions.push(ObjectVersion {
                        key: key.to_string(),
                        version_id: v.version_id().unwrap_or("null").to_string(),
                        is_latest: v.is_latest().unwrap_or(false),
                        size: v.size().unwrap_or(0),
                        last_modified,
                        e_tag: v.e_tag().map(|s| s.to_string()),
                        storage_class: v.storage_class().map(|s| s.as_str().to_string()),
                    });
                }
            }

            if !output.is_truncated() {
                break;
            }
            key_marker = output.next_key_marker().map(|s| s.to_string());
            version_id_marker = output.next_version_id_marker().map(|s| s.to_string());
        }

        info!(
            bucket = %bucket,
            prefix = %prefix,
            count = versions.len(),
            "Object versions listed"
        );
        Ok(versions)
    }

    async fn get_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Vec<u8>> {
        debug!(bucket = %bucket, key = %key, version_id = %version_id, "Get object version");
        let output = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let data = output
            .body
            .collect()
            .await
            .map_err(|e| S3Error::Aws(format!("Failed to read body: {}", e)))?;
        Ok(data.to_vec())
    }

    async fn delete_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<()> {
        debug!(bucket = %bucket, key = %key, version_id = %version_id, "Delete object version");
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %bucket, key = %key, version_id = %version_id, "Object version deleted");
        Ok(())
    }

    async fn restore_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<()> {
        debug!(bucket = %bucket, key = %key, version_id = %version_id, "Restore object version");
        // Copy the specific version back to the current key (overwrites current version)
        let copy_source = format!("{}/{}?versionId={}", bucket, key, version_id);
        self.client
            .copy_object()
            .copy_source(&copy_source)
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;
        info!(bucket = %bucket, key = %key, version_id = %version_id, "Object version restored");
        Ok(())
    }

    // ── ACL ──

    async fn get_bucket_acl(&self, bucket: &str) -> Result<Vec<AclGrant>> {
        debug!(bucket = %bucket, "Get bucket ACL");
        let output = self
            .client
            .get_bucket_acl()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let grants = parse_acl_grants(output.grants());
        info!(bucket = %bucket, count = grants.len(), "Bucket ACL retrieved");
        Ok(grants)
    }

    async fn set_bucket_acl(&self, bucket: &str, grants: &[AclGrant]) -> Result<()> {
        debug!(bucket = %bucket, count = grants.len(), "Set bucket ACL");
        let aws_grants: Vec<aws_sdk_s3::types::Grant> = grants.iter().map(|g| grant_to_aws(g)).collect();

        self.client
            .put_bucket_acl()
            .bucket(bucket)
            .set_access_control_policy(Some(
                aws_sdk_s3::types::AccessControlPolicy::builder()
                    .set_grants(Some(aws_grants))
                    .build(),
            ))
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(bucket = %bucket, count = grants.len(), "Bucket ACL updated");
        Ok(())
    }

    async fn get_object_acl(&self, bucket: &str, key: &str) -> Result<Vec<AclGrant>> {
        debug!(bucket = %bucket, key = %key, "Get object ACL");
        let output = self
            .client
            .get_object_acl()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        let grants = parse_acl_grants(output.grants());
        info!(bucket = %bucket, key = %key, count = grants.len(), "Object ACL retrieved");
        Ok(grants)
    }

    async fn set_object_acl(&self, bucket: &str, key: &str, grants: &[AclGrant]) -> Result<()> {
        debug!(bucket = %bucket, key = %key, count = grants.len(), "Set object ACL");
        let aws_grants: Vec<aws_sdk_s3::types::Grant> = grants.iter().map(|g| grant_to_aws(g)).collect();

        self.client
            .put_object_acl()
            .bucket(bucket)
            .key(key)
            .set_access_control_policy(Some(
                aws_sdk_s3::types::AccessControlPolicy::builder()
                    .set_grants(Some(aws_grants))
                    .build(),
            ))
            .send()
            .await
            .map_err(|e| Self::map_sdk_error(&e.into()))?;

        info!(bucket = %bucket, key = %key, count = grants.len(), "Object ACL updated");
        Ok(())
    }
}

/// Convert AWS SDK timestamp to chrono DateTime<Utc>
fn aws_datetime_to_chrono(dt: &aws_sdk_s3::primitives::DateTime) -> DateTime<Utc> {
    let nanos = dt.as_nanos();
    let secs = nanos / 1_000_000_000;
    let nsecs = (nanos % 1_000_000_000) as u32;
    DateTime::from_timestamp(secs as i64, nsecs).unwrap_or_else(|| Utc::now())
}

/// Parse AWS SDK grants into our AclGrant type
fn parse_acl_grants(aws_grants: &[aws_sdk_s3::types::Grant]) -> Vec<AclGrant> {
    aws_grants
        .iter()
        .filter_map(|g| {
            let grantee = g.grantee()?;
            let permission = g.permission()?.as_str().to_string();
            Some(AclGrant {
                grantee: Grantee {
                    id: grantee.id().map(|s| s.to_string()),
                    display_name: grantee.display_name().map(|s| s.to_string()),
                    uri: grantee.uri().map(|s| s.to_string()),
                    grantee_type: grantee.r#type().as_str().to_string(),
                },
                permission,
            })
        })
        .collect()
}

/// Convert our AclGrant to AWS SDK Grant
fn grant_to_aws(grant: &AclGrant) -> aws_sdk_s3::types::Grant {
    let mut grantee_builder = AwsGrantee::builder()
        .r#type(aws_sdk_s3::types::Type::from(grant.grantee.grantee_type.as_str()));

    if let Some(ref id) = grant.grantee.id {
        grantee_builder = grantee_builder.id(id);
    }
    if let Some(ref name) = grant.grantee.display_name {
        grantee_builder = grantee_builder.display_name(name);
    }
    if let Some(ref uri) = grant.grantee.uri {
        grantee_builder = grantee_builder.uri(uri);
    }

    aws_sdk_s3::types::Grant::builder()
        .grantee(grantee_builder.build())
        .permission(Permission::from(grant.permission.as_str()))
        .build()
}
