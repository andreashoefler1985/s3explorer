//! r2 — Integration tests
//!
//! Basic integration tests for Sprint 1-4.

#[cfg(test)]
mod tests {
    use r2_core::cache::manager::{CacheManager, CacheStats, SqliteCacheManager};
    use r2_core::credentials::profile::Profile;
    use r2_core::s3_client::types::{
        AclGrant, BucketInfo, Grantee, ObjectInfo, ObjectVersion,
    };
    use uuid::Uuid;

    // ── Cache Tests ──

    /// Test that the cache module can be initialized
    #[test]
    fn test_cache_initialization() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone());
        assert!(cache.is_ok(), "Cache should initialize successfully");
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test cache and retrieve buckets
    #[test]
    fn test_cache_buckets() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        let buckets = vec![
            BucketInfo {
                name: "test-bucket-1".to_string(),
                creation_date: None,
            },
            BucketInfo {
                name: "test-bucket-2".to_string(),
                creation_date: None,
            },
        ];

        // Cache buckets
        let result = cache.cache_buckets(&profile_id, &buckets);
        assert!(result.is_ok(), "Should cache buckets successfully");

        // Retrieve cached buckets
        let cached = cache.get_cached_buckets(&profile_id).unwrap();
        assert_eq!(cached.len(), 2, "Should retrieve 2 buckets");
        assert_eq!(cached[0].name, "test-bucket-1");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test cache and retrieve objects
    #[test]
    fn test_cache_objects() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        let objects = vec![
            ObjectInfo {
                key: "file1.txt".to_string(),
                size: 100,
                last_modified: None,
                e_tag: Some("abc123".to_string()),
                storage_class: Some("STANDARD".to_string()),
                is_prefix: false,
            },
            ObjectInfo {
                key: "folder/".to_string(),
                size: 0,
                last_modified: None,
                e_tag: None,
                storage_class: None,
                is_prefix: true,
            },
        ];

        // Cache objects
        let result = cache.cache_objects(&profile_id, "test-bucket", "", &objects);
        assert!(result.is_ok(), "Should cache objects successfully");

        // Retrieve cached objects
        let cached = cache.get_cached_objects(&profile_id, "test-bucket", "").unwrap();
        assert_eq!(cached.len(), 2, "Should retrieve 2 objects");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test cache staleness check
    #[test]
    fn test_cache_staleness() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        // No cache yet → should be stale
        let stale = cache.is_cache_stale(&profile_id, "test-bucket", "").unwrap();
        assert!(stale, "Empty cache should be stale");

        // Cache some objects
        let objects = vec![ObjectInfo {
            key: "test.txt".to_string(),
            size: 100,
            last_modified: None,
            e_tag: None,
            storage_class: None,
            is_prefix: false,
        }];
        cache.cache_objects(&profile_id, "test-bucket", "", &objects).unwrap();

        // Should not be stale immediately
        let stale = cache.is_cache_stale(&profile_id, "test-bucket", "").unwrap();
        assert!(!stale, "Fresh cache should not be stale");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test clear cache
    #[test]
    fn test_clear_cache() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        let buckets = vec![BucketInfo {
            name: "test-bucket".to_string(),
            creation_date: None,
        }];
        cache.cache_buckets(&profile_id, &buckets).unwrap();

        // Clear cache
        cache.clear_cache(&profile_id).unwrap();

        // Should be empty
        let cached = cache.get_cached_buckets(&profile_id).unwrap();
        assert_eq!(cached.len(), 0, "Cache should be empty after clear");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test cache stats
    #[test]
    fn test_cache_stats() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        // Empty cache stats
        let stats = cache.get_cache_stats(&profile_id).unwrap();
        assert_eq!(stats.bucket_count, 0);
        assert_eq!(stats.object_count, 0);
        assert!(stats.cache_age_secs.is_none());

        // Add some data
        let buckets = vec![BucketInfo {
            name: "test-bucket".to_string(),
            creation_date: None,
        }];
        cache.cache_buckets(&profile_id, &buckets).unwrap();

        let objects = vec![ObjectInfo {
            key: "test.txt".to_string(),
            size: 100,
            last_modified: None,
            e_tag: None,
            storage_class: None,
            is_prefix: false,
        }];
        cache.cache_objects(&profile_id, "test-bucket", "", &objects).unwrap();

        let stats = cache.get_cache_stats(&profile_id).unwrap();
        assert_eq!(stats.bucket_count, 1);
        assert_eq!(stats.object_count, 1);
        assert!(stats.cache_age_secs.is_some());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// Test cache age
    #[test]
    fn test_cache_age() {
        let tmp_dir = std::env::temp_dir().join(format!("r2-test-{}", Uuid::new_v4()));
        let cache = SqliteCacheManager::new(tmp_dir.clone()).unwrap();
        let profile_id = Uuid::new_v4();

        // No data → no age
        let age = cache.get_cache_age(&profile_id).unwrap();
        assert!(age.is_none());

        // Add data → age should be 0 or very small
        let objects = vec![ObjectInfo {
            key: "test.txt".to_string(),
            size: 100,
            last_modified: None,
            e_tag: None,
            storage_class: None,
            is_prefix: false,
        }];
        cache.cache_objects(&profile_id, "test-bucket", "", &objects).unwrap();

        let age = cache.get_cache_age(&profile_id).unwrap();
        assert!(age.is_some());
        assert!(age.unwrap() < 5, "Cache age should be < 5 seconds");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // ── Profile Tests ──

    /// Test profile creation
    #[test]
    fn test_profile_creation() {
        let profile = Profile::new(
            "Test Profile".to_string(),
            "http://localhost:9000".to_string(),
            "us-east-1".to_string(),
            Some("default-bucket".to_string()),
            true,
        );

        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.endpoint_url, "http://localhost:9000");
        assert_eq!(profile.region, "us-east-1");
        assert_eq!(profile.default_bucket, Some("default-bucket".to_string()));
        assert!(profile.path_style);
    }

    /// Test S3ClientConfig defaults
    #[test]
    fn test_s3_client_config_defaults() {
        let config = r2_core::S3ClientConfig::default();
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.connect_timeout_secs, 30);
        assert_eq!(config.operation_timeout_secs, 120);
        assert_eq!(config.max_retries, 3);
        assert!(!config.path_style);
    }

    // ── ACL Serialization Tests ──

    /// Test ACL grant serialization round-trip
    #[test]
    fn test_acl_grant_serialization() {
        let grant = AclGrant {
            grantee: Grantee {
                id: Some("test-id".into()),
                display_name: None,
                uri: None,
                grantee_type: "CanonicalUser".into(),
            },
            permission: "FULL_CONTROL".into(),
        };
        let json = serde_json::to_string(&grant).unwrap();
        let deserialized: AclGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.permission, "FULL_CONTROL");
        assert_eq!(deserialized.grantee.id, Some("test-id".into()));
        assert_eq!(deserialized.grantee.grantee_type, "CanonicalUser");
    }

    /// Test ACL grant with AllUsers group
    #[test]
    fn test_acl_grant_all_users() {
        let grant = AclGrant {
            grantee: Grantee {
                id: None,
                display_name: None,
                uri: Some("http://acs.amazonaws.com/groups/global/AllUsers".into()),
                grantee_type: "Group".into(),
            },
            permission: "READ".into(),
        };
        let json = serde_json::to_string(&grant).unwrap();
        let deserialized: AclGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.permission, "READ");
        assert!(deserialized.grantee.uri.is_some());
        assert!(deserialized.grantee.uri.unwrap().contains("AllUsers"));
    }

    // ── ObjectVersion Tests ──

    /// Test ObjectVersion creation
    #[test]
    fn test_object_version_creation() {
        use chrono::Utc;
        let version = ObjectVersion {
            key: "test.txt".to_string(),
            version_id: "abc123".to_string(),
            is_latest: true,
            size: 1024,
            last_modified: Utc::now(),
            e_tag: Some("\"etag123\"".to_string()),
            storage_class: Some("STANDARD".to_string()),
        };
        assert_eq!(version.key, "test.txt");
        assert!(version.is_latest);
        assert_eq!(version.size, 1024);
    }

    // ── Utility function tests ──

    /// Test parent_prefix function
    #[test]
    fn test_parent_prefix() {
        assert_eq!(r2_core::s3_client::parent_prefix("images/2025/"), "images/");
        assert_eq!(r2_core::s3_client::parent_prefix("images/"), "");
        assert_eq!(r2_core::s3_client::parent_prefix(""), "");
        assert_eq!(r2_core::s3_client::parent_prefix("a/b/c/"), "a/b/");
    }

    /// Test bytes formatting
    #[test]
    fn test_bytes_formatting() {
        use r2_core::s3_client::format_bytes;
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }
}
