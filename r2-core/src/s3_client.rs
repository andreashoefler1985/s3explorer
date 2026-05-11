//! r2-core — S3 Client module
//! 
//! Provides S3Client trait and aws-sdk-s3 implementation.

pub mod client;
pub mod types;

pub use client::{AwsSdkS3Client, S3Client};
pub use types::{BucketInfo, ObjectInfo};