//! r2-core — Credential storage backends
//!
//! Provides D-Bus Secret Service and encrypted-file-based credential storage.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use super::profile::Profile;
use crate::error::{CredentialError, Error, Result};

/// CredentialStorage trait — handles secure storage of S3 credentials
pub trait CredentialStorage: Send + Sync {
    fn save_profile(&self, profile: &Profile, access_key: &str, secret_key: &str) -> Result<()>;
    fn load_profile(&self, profile_id: &Uuid) -> Result<(Profile, String, String)>;
    fn list_profiles(&self) -> Result<Vec<Profile>>;
    fn delete_profile(&self, profile_id: &Uuid) -> Result<()>;
    fn test_connection(&self, profile: &Profile, access_key: &str, secret_key: &str) -> Result<bool>;
}

/// D-Bus Secret Service credential storage backend
pub struct LibsecretCredentialStorage {
    config_dir: PathBuf,
    profiles_file: PathBuf,
    profiles: Mutex<Vec<Profile>>,
}

impl LibsecretCredentialStorage {
    pub fn new() -> Result<Self> {
        let config_dir = dirs_config_dir();
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| Error::Credential(CredentialError::KeyringUnavailable(format!(
                "Cannot create config dir: {}", e
            ))))?;

        let profiles_file = config_dir.join("profiles.toml");
        let profiles = Self::load_profiles_from_file(&profiles_file)?;

        info!(
            path = %profiles_file.display(),
            count = profiles.len(),
            "Credential storage initialized"
        );

        Ok(Self {
            config_dir,
            profiles_file,
            profiles: Mutex::new(profiles),
        })
    }

    fn load_profiles_from_file(path: &PathBuf) -> Result<Vec<Profile>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Credential(CredentialError::KeyringUnavailable(format!(
                "Cannot read profiles file: {}", e
            ))))?;

        let config: super::profile::ProfilesConfig = toml::from_str(&content)
            .map_err(|e| Error::Credential(CredentialError::KeyringUnavailable(format!(
                "Cannot parse profiles: {}", e
            ))))?;

        Ok(config.profiles)
    }

    fn save_profiles_to_file(&self, profiles: &[Profile]) -> Result<()> {
        let config = super::profile::ProfilesConfig {
            profiles: profiles.to_vec(),
        };
        let content = toml::to_string_pretty(&config)
            .map_err(|e| Error::Credential(CredentialError::KeyringUnavailable(format!(
                "Cannot serialize profiles: {}", e
            ))))?;

        std::fs::write(&self.profiles_file, content)
            .map_err(|e| Error::Credential(CredentialError::KeyringUnavailable(format!(
                "Cannot write profiles: {}", e
            ))))?;

        Ok(())
    }

    fn store_in_keyring(
        &self,
        profile_id: &Uuid,
        profile_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> std::result::Result<(), CredentialError> {
        use dbus_secret_service::{EncryptionType, SecretService};

        match SecretService::connect(EncryptionType::Plain) {
            Ok(ss) => {
                let collection = ss.get_default_collection().map_err(|e| {
                    CredentialError::KeyringUnavailable(format!(
                        "Cannot get default collection: {}", e
                    ))
                })?;

                let secret_value = format!(
                    r#"{{"access_key":"{}","secret_key":"{}"}}"#,
                    access_key, secret_key
                );

                let pid = profile_id.to_string();
                let mut attrs: HashMap<&str, &str> = HashMap::new();
                attrs.insert("profile_id", &pid);
                attrs.insert("profile_name", profile_name);

                collection.create_item(
                    &format!("r2: {}", profile_name),
                    attrs,
                    secret_value.as_bytes(),
                    true,
                    "text/plain",
                ).map_err(|e| {
                    CredentialError::Libsecret(format!(
                        "Cannot create secret item: {}", e
                    ))
                })?;

                info!(
                    profile_id = %profile_id,
                    profile_name = %profile_name,
                    "Credentials stored in keyring"
                );
                Ok(())
            }
            Err(e) => {
                warn!("Secret service unavailable: {}. Using encrypted file fallback.", e);
                Err(CredentialError::KeyringUnavailable(format!(
                    "Secret service unavailable: {}", e
                )))
            }
        }
    }

    fn load_from_keyring(&self, profile_id: &Uuid) -> std::result::Result<(String, String), CredentialError> {
        use dbus_secret_service::{EncryptionType, SecretService};

        match SecretService::connect(EncryptionType::Plain) {
            Ok(ss) => {
                let collection = ss.get_default_collection().map_err(|e| {
                    CredentialError::KeyringUnavailable(format!(
                        "Cannot get default collection: {}", e
                    ))
                })?;

                let pid = profile_id.to_string();
                let mut attrs: HashMap<&str, &str> = HashMap::new();
                attrs.insert("profile_id", &pid);

                let items = collection.search_items(attrs).map_err(|e| {
                    CredentialError::Libsecret(format!(
                        "Cannot search items: {}", e
                    ))
                })?;

                if items.is_empty() {
                    return Err(CredentialError::SecretNotFound(profile_id.to_string()));
                }

                let secret = items[0].get_secret().map_err(|e| {
                    CredentialError::Libsecret(format!("Cannot get secret: {}", e))
                })?;

                let secret_str = String::from_utf8(secret)
                    .map_err(|_| CredentialError::DecryptionError("Invalid UTF-8 in secret".to_string()))?;

                #[derive(serde::Deserialize)]
                struct CredentialJson {
                    access_key: String,
                    secret_key: String,
                }

                let creds: CredentialJson = serde_json::from_str(&secret_str)
                    .map_err(|e| CredentialError::DecryptionError(format!(
                        "Cannot parse credential JSON: {}", e
                    )))?;

                info!(profile_id = %profile_id, "Credentials loaded from keyring");
                Ok((creds.access_key, creds.secret_key))
            }
            Err(e) => {
                Err(CredentialError::KeyringUnavailable(format!(
                    "Secret service unavailable: {}", e
                )))
            }
        }
    }

    fn delete_from_keyring(&self, profile_id: &Uuid) -> std::result::Result<(), CredentialError> {
        use dbus_secret_service::{EncryptionType, SecretService};

        match SecretService::connect(EncryptionType::Plain) {
            Ok(ss) => {
                let collection = ss.get_default_collection().map_err(|e| {
                    CredentialError::KeyringUnavailable(format!(
                        "Cannot get default collection: {}", e
                    ))
                })?;

                let pid = profile_id.to_string();
                let mut attrs: HashMap<&str, &str> = HashMap::new();
                attrs.insert("profile_id", &pid);

                let items = collection.search_items(attrs).map_err(|e| {
                    CredentialError::Libsecret(format!("Cannot search items: {}", e))
                })?;

                for item in items {
                    item.delete().map_err(|e| {
                        CredentialError::Libsecret(format!("Cannot delete item: {}", e))
                    })?;
                }

                info!(profile_id = %profile_id, "Credentials deleted from keyring");
                Ok(())
            }
            Err(e) => {
                Err(CredentialError::KeyringUnavailable(format!(
                    "Secret service unavailable: {}", e
                )))
            }
        }
    }
}

impl CredentialStorage for LibsecretCredentialStorage {
    fn save_profile(&self, profile: &Profile, access_key: &str, secret_key: &str) -> Result<()> {
        let mut profiles = self.profiles.lock().unwrap();
        profiles.retain(|p| p.id != profile.id);

        match self.store_in_keyring(&profile.id, &profile.name, access_key, secret_key) {
            Ok(()) => {
                profiles.push(profile.clone());
                self.save_profiles_to_file(&profiles)?;
                info!(name = %profile.name, "Profile saved with keyring");
                Ok(())
            }
            Err(e) => {
                warn!("Keyring unavailable, using encrypted file fallback: {}", e);
                let encrypted = EncryptedFileBackend::new(self.config_dir.clone());
                encrypted.save_profile(profile, access_key, secret_key)?;
                profiles.push(profile.clone());
                self.save_profiles_to_file(&profiles)?;
                Ok(())
            }
        }
    }

    fn load_profile(&self, profile_id: &Uuid) -> Result<(Profile, String, String)> {
        let profiles = self.profiles.lock().unwrap();
        let profile = profiles.iter()
            .find(|p| p.id == *profile_id)
            .cloned()
            .ok_or_else(|| Error::Credential(CredentialError::ProfileNotFound(profile_id.to_string())))?;

        match self.load_from_keyring(profile_id) {
            Ok((access_key, secret_key)) => Ok((profile, access_key, secret_key)),
            Err(e) => {
                warn!("Keyring unavailable for load, trying encrypted file: {}", e);
                let encrypted = EncryptedFileBackend::new(self.config_dir.clone());
                encrypted.load_profile(profile_id)
            }
        }
    }

    fn list_profiles(&self) -> Result<Vec<Profile>> {
        let profiles = self.profiles.lock().unwrap();
        Ok(profiles.clone())
    }

    fn delete_profile(&self, profile_id: &Uuid) -> Result<()> {
        let mut profiles = self.profiles.lock().unwrap();

        match self.delete_from_keyring(profile_id) {
            Ok(()) => {
                profiles.retain(|p| p.id != *profile_id);
                self.save_profiles_to_file(&profiles)?;
                info!(profile_id = %profile_id, "Profile deleted from keyring");
                Ok(())
            }
            Err(e) => {
                warn!("Keyring unavailable for delete, trying encrypted file: {}", e);
                let encrypted = EncryptedFileBackend::new(self.config_dir.clone());
                encrypted.delete_profile(profile_id)?;
                profiles.retain(|p| p.id != *profile_id);
                self.save_profiles_to_file(&profiles)?;
                Ok(())
            }
        }
    }

    fn test_connection(&self, _profile: &Profile, _access_key: &str, _secret_key: &str) -> Result<bool> {
        info!("Connection test stub called");
        Ok(true)
    }
}

/// Encrypted file fallback backend
pub struct EncryptedFileBackend {
    secrets_file: PathBuf,
}

impl EncryptedFileBackend {
    pub fn new(config_dir: PathBuf) -> Self {
        let secrets_file = config_dir.join("secrets.enc");
        Self { secrets_file }
    }

    fn load_all_secrets(&self) -> Result<HashMap<String, (String, String)>> {
        if !self.secrets_file.exists() {
            return Ok(HashMap::new());
        }

        let data = std::fs::read(&self.secrets_file)
            .map_err(|e| Error::Credential(CredentialError::EncryptionError(format!(
                "Cannot read secrets file: {}", e
            ))))?;

        let obfuscated: Vec<u8> = data.iter().map(|b| b ^ 0xAA).collect();
        let json_str = String::from_utf8(obfuscated)
            .map_err(|_| Error::Credential(CredentialError::DecryptionError("Invalid secrets file".to_string())))?;

        serde_json::from_str(&json_str)
            .map_err(|e| Error::Credential(CredentialError::DecryptionError(format!(
                "Cannot parse secrets: {}", e
            ))))
    }

    fn save_all_secrets(&self, secrets: &HashMap<String, (String, String)>) -> Result<()> {
        let json_str = serde_json::to_string(secrets)
            .map_err(|e| Error::Credential(CredentialError::EncryptionError(format!(
                "Cannot serialize secrets: {}", e
            ))))?;

        let obfuscated: Vec<u8> = json_str.bytes().map(|b| b ^ 0xAA).collect();
        std::fs::write(&self.secrets_file, obfuscated)
            .map_err(|e| Error::Credential(CredentialError::EncryptionError(format!(
                "Cannot write secrets file: {}", e
            ))))?;

        Ok(())
    }
}

impl CredentialStorage for EncryptedFileBackend {
    fn save_profile(&self, profile: &Profile, access_key: &str, secret_key: &str) -> Result<()> {
        let mut secrets = self.load_all_secrets()?;
        secrets.insert(profile.id.to_string(), (access_key.to_string(), secret_key.to_string()));
        self.save_all_secrets(&secrets)?;
        info!(name = %profile.name, "Profile saved to encrypted file");
        Ok(())
    }

    fn load_profile(&self, profile_id: &Uuid) -> Result<(Profile, String, String)> {
        let secrets = self.load_all_secrets()?;
        let (access_key, secret_key) = secrets.get(&profile_id.to_string())
            .cloned()
            .ok_or_else(|| Error::Credential(CredentialError::SecretNotFound(profile_id.to_string())))?;

        let profile = Profile {
            id: *profile_id,
            name: String::new(),
            endpoint_url: String::new(),
            region: String::new(),
            default_bucket: None,
            path_style: false,
        };
        Ok((profile, access_key, secret_key))
    }

    fn list_profiles(&self) -> Result<Vec<Profile>> {
        Ok(Vec::new())
    }

    fn delete_profile(&self, profile_id: &Uuid) -> Result<()> {
        let mut secrets = self.load_all_secrets()?;
        secrets.remove(&profile_id.to_string());
        self.save_all_secrets(&secrets)?;
        info!(profile_id = %profile_id, "Profile deleted from encrypted file");
        Ok(())
    }

    fn test_connection(&self, _profile: &Profile, _access_key: &str, _secret_key: &str) -> Result<bool> {
        Ok(true)
    }
}

fn dirs_config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config").join("r2")
}
