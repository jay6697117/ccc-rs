//! Secure credential storage.
//! Corresponds to TS `src/utils/secureStorage/`.
//!
//! Strategy (mirrors TS):
//!   macOS  → keyring (Security.framework) with plaintext fallback
//!   others → plaintext `.credentials.json` (mode 0600)

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};

use ccc_core::claude_config_dir;

use crate::types::SecureStorageData;
use anyhow::{Context, Result};

// ── Storage path ──────────────────────────────────────────────────────────────

fn credentials_path() -> PathBuf {
    claude_config_dir().join(".credentials.json")
}

// ── Plaintext storage ─────────────────────────────────────────────────────────

pub struct PlainTextStorage {
    path: PathBuf,
}

impl PlainTextStorage {
    pub fn new() -> Self {
        Self { path: credentials_path() }
    }

    pub fn read(&self) -> Result<SecureStorageData> {
        let text = fs::read_to_string(&self.path)
            .unwrap_or_else(|_| "{}".into());
        serde_json::from_str(&text).context("parse .credentials.json")
    }

    /// Write data and set permissions to 0600.
    pub fn write(&self, data: &SecureStorageData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .context("create ~/.claude directory")?;
        }
        let json = serde_json::to_string_pretty(data)
            .context("serialize credentials")?;
        fs::write(&self.path, json).context("write .credentials.json")?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
            .context("chmod 0600 .credentials.json")?;
        Ok(())
    }

    pub fn delete(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path).context("delete .credentials.json")?;
        }
        Ok(())
    }
}

// ── Keyring storage (macOS keychain / libsecret) ──────────────────────────────

const KEYRING_SERVICE: &str = "claude-code";
const KEYRING_USER: &str = "oauth";

pub struct KeyringStorage;

impl KeyringStorage {
    pub fn read(&self) -> Result<Option<SecureStorageData>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("create keyring entry")?;
        match entry.get_password() {
            Ok(raw) => {
                let data = serde_json::from_str(&raw)
                    .context("parse keyring JSON")?;
                Ok(Some(data))
            }
            Err(keyring::Error::NoEntry) | Err(keyring::Error::NoStorageAccess(_)) => Ok(None),
            Err(e) => Err(e).context("keyring read"),
        }
    }

    pub fn write(&self, data: &SecureStorageData) -> Result<()> {
        let json = serde_json::to_string(data).context("serialize for keyring")?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("create keyring entry")?;
        entry.set_password(&json).context("keyring write")?;
        Ok(())
    }

    pub fn delete(&self) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("create keyring entry")?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("keyring delete"),
        }
    }
}

// ── Unified facade ─────────────────────────────────────────────────────────────

/// Reads credentials: keyring → plaintext fallback (macOS)
///                    plaintext only (other platforms).
pub fn read_credentials() -> Result<SecureStorageData> {
    #[cfg(target_os = "macos")]
    {
        let kr = KeyringStorage;
        if let Ok(Some(data)) = kr.read() {
            return Ok(data);
        }
    }
    PlainTextStorage::new().read()
}

/// Writes credentials: tries keyring first on macOS, falls back to plaintext.
pub fn write_credentials(data: &SecureStorageData) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let kr = KeyringStorage;
        if kr.write(data).is_ok() {
            // Migrate: if we just wrote to keyring for the first time, remove
            // the plaintext copy so the two don't diverge.
            let _ = PlainTextStorage::new().delete();
            return Ok(());
        }
    }
    PlainTextStorage::new().write(data)
}

/// Deletes credentials from all backends.
pub fn delete_credentials() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = KeyringStorage.delete();
    }
    PlainTextStorage::new().delete()
}
