use std::fs;
use std::path::{Path, PathBuf};

use ccc_core::{
    ManagedSettingsFreshness, ManagedSettingsSnapshot, RemoteManagedSettingsCache,
    config::McpServerConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CliError;

pub const MANAGED_SETTINGS_FILE: &str = "managed-settings.json";
pub const MANAGED_SETTINGS_DROPIN_DIR: &str = "managed-settings.d";
pub const REMOTE_MANAGED_SETTINGS_CACHE_FILE: &str = "remote-managed-settings-cache.json";
pub const ENTERPRISE_MCP_FILE: &str = "managed-mcp.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EnterpriseMcpSnapshot {
    pub servers: std::collections::HashMap<String, McpServerConfig>,
    pub warnings: Vec<String>,
    pub source_label: String,
}

impl Default for EnterpriseMcpSnapshot {
    fn default() -> Self {
        Self {
            servers: Default::default(),
            warnings: Vec::new(),
            source_label: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EnterpriseMcpFile {
    mcp_servers: std::collections::HashMap<String, McpServerConfig>,
    servers: std::collections::HashMap<String, McpServerConfig>,
}

impl Default for EnterpriseMcpFile {
    fn default() -> Self {
        Self {
            mcp_servers: Default::default(),
            servers: Default::default(),
        }
    }
}

pub fn managed_root(config_dir: &Path) -> PathBuf {
    config_dir.join("managed")
}

pub fn load_managed_settings(root: &Path) -> Result<ManagedSettingsSnapshot, CliError> {
    let mut warnings = Vec::new();
    let mut merged = Value::Object(Map::new());
    let mut file_layer_exists = false;

    let base_file = root.join(MANAGED_SETTINGS_FILE);
    if base_file.exists() {
        file_layer_exists = true;
        merge_json_file(&base_file, &mut merged, &mut warnings, false)?;
    }

    let dropins_dir = root.join(MANAGED_SETTINGS_DROPIN_DIR);
    if dropins_dir.exists() {
        let mut entries = fs::read_dir(&dropins_dir)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            file_layer_exists = true;
            merge_json_file(&entry.path(), &mut merged, &mut warnings, true)?;
        }
    }

    let remote_cache = load_remote_cache(root, &mut warnings)?;
    if let Some(cache) = &remote_cache {
        deep_merge(&mut merged, cache.settings.clone());
    }

    let freshness = if remote_cache.is_some() || file_layer_exists {
        ManagedSettingsFreshness::Fresh
    } else {
        ManagedSettingsFreshness::Missing
    };

    Ok(ManagedSettingsSnapshot {
        merged_settings: merged,
        freshness,
        warnings,
        remote_cache,
    })
}

pub fn load_enterprise_mcp_snapshot(root: &Path) -> Result<EnterpriseMcpSnapshot, CliError> {
    let path = root.join(ENTERPRISE_MCP_FILE);
    if !path.exists() {
        return Ok(EnterpriseMcpSnapshot {
            source_label: path.display().to_string(),
            ..EnterpriseMcpSnapshot::default()
        });
    }

    let text = fs::read_to_string(&path)?;
    let parsed = match serde_json::from_str::<EnterpriseMcpFile>(&text) {
        Ok(value) => value,
        Err(error) => {
            return Ok(EnterpriseMcpSnapshot {
                warnings: vec![format!(
                    "failed to parse enterprise MCP file {}: {error}",
                    path.display()
                )],
                source_label: path.display().to_string(),
                ..EnterpriseMcpSnapshot::default()
            });
        }
    };

    let servers = if parsed.mcp_servers.is_empty() {
        parsed.servers
    } else {
        parsed.mcp_servers
    };

    Ok(EnterpriseMcpSnapshot {
        servers,
        warnings: Vec::new(),
        source_label: path.display().to_string(),
    })
}

fn load_remote_cache(
    root: &Path,
    warnings: &mut Vec<String>,
) -> Result<Option<RemoteManagedSettingsCache>, CliError> {
    let path = root.join(REMOTE_MANAGED_SETTINGS_CACHE_FILE);
    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path)?;
    let cache: RemoteManagedSettingsCache = serde_json::from_str(&text)?;
    let checksum = checksum_json_value(&cache.settings)?;
    if checksum != cache.checksum {
        warnings.push(format!(
            "remote managed settings cache checksum mismatch: {}",
            path.display()
        ));
        return Ok(None);
    }

    Ok(Some(cache))
}

fn merge_json_file(
    path: &Path,
    target: &mut Value,
    warnings: &mut Vec<String>,
    ignore_parse_errors: bool,
) -> Result<(), CliError> {
    let text = fs::read_to_string(path)?;
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => {
            deep_merge(target, value);
            Ok(())
        }
        Err(error) if ignore_parse_errors => {
            warnings.push(format!(
                "failed to parse managed settings drop-in {}: {error}",
                path.display()
            ));
            Ok(())
        }
        Err(error) => Err(CliError::new(
            format!("failed to parse managed settings file {}: {error}", path.display()),
            1,
        )),
    }
}

fn deep_merge(target: &mut Value, overlay: Value) {
    match (target, overlay) {
        (Value::Object(target_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match target_map.get_mut(&key) {
                    Some(target_value) => deep_merge(target_value, overlay_value),
                    None => {
                        target_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (target_slot, overlay_value) => *target_slot = overlay_value,
    }
}

fn checksum_json_value(value: &Value) -> Result<String, CliError> {
    use sha2::{Digest, Sha256};

    let bytes = serde_json::to_vec(value)?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use ccc_core::ManagedSettingsFreshness;

    #[test]
    fn managed_file_settings_merge_dropins_in_sorted_order() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(MANAGED_SETTINGS_DROPIN_DIR)).unwrap();
        fs::write(
            root.join(MANAGED_SETTINGS_FILE),
            serde_json::json!({
                "channelsEnabled": false,
                "allowedMcpServers": [{"serverName": "base"}]
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            root.join(MANAGED_SETTINGS_DROPIN_DIR).join("20-channels.json"),
            serde_json::json!({
                "channelsEnabled": true
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_managed_settings(root).unwrap();

        assert_eq!(snapshot.freshness, ManagedSettingsFreshness::Fresh);
        assert_eq!(snapshot.merged_settings["channelsEnabled"], true);
    }

    #[test]
    fn missing_managed_settings_returns_missing_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = load_managed_settings(temp.path()).unwrap();

        assert_eq!(snapshot.freshness, ManagedSettingsFreshness::Missing);
        assert_eq!(snapshot.merged_settings, serde_json::json!({}));
    }

    #[test]
    fn remote_cache_overrides_file_layer_when_checksum_matches() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join(MANAGED_SETTINGS_FILE),
            serde_json::json!({
                "channelsEnabled": false,
                "strictPluginOnlyCustomization": false
            })
            .to_string(),
        )
        .unwrap();

        let remote_settings = serde_json::json!({
            "channelsEnabled": true,
            "strictPluginOnlyCustomization": ["mcp"]
        });
        let checksum = checksum_json_value(&remote_settings).unwrap();
        fs::write(
            root.join(REMOTE_MANAGED_SETTINGS_CACHE_FILE),
            serde_json::json!({
                "uuid": "cache-1",
                "checksum": checksum,
                "fetchedAtUnixMs": 123,
                "settings": remote_settings
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_managed_settings(root).unwrap();

        assert_eq!(snapshot.freshness, ManagedSettingsFreshness::Fresh);
        assert_eq!(snapshot.merged_settings["channelsEnabled"], true);
        assert_eq!(
            snapshot.merged_settings["strictPluginOnlyCustomization"],
            serde_json::json!(["mcp"])
        );
        assert!(snapshot.remote_cache.is_some());
    }

    #[test]
    fn invalid_dropin_is_skipped_with_warning() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(MANAGED_SETTINGS_DROPIN_DIR)).unwrap();
        fs::write(
            root.join(MANAGED_SETTINGS_DROPIN_DIR).join("10-invalid.json"),
            "{not-json",
        )
        .unwrap();

        let snapshot = load_managed_settings(root).unwrap();

        assert_eq!(snapshot.freshness, ManagedSettingsFreshness::Fresh);
        assert_eq!(snapshot.warnings.len(), 1);
    }

    #[test]
    fn invalid_remote_cache_checksum_is_ignored_with_warning() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join(REMOTE_MANAGED_SETTINGS_CACHE_FILE),
            serde_json::json!({
                "uuid": "cache-1",
                "checksum": "bad",
                "fetchedAtUnixMs": 123,
                "settings": { "channelsEnabled": true }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_managed_settings(root).unwrap();

        assert_eq!(snapshot.freshness, ManagedSettingsFreshness::Missing);
        assert!(snapshot.remote_cache.is_none());
        assert_eq!(snapshot.warnings.len(), 1);
    }

    #[test]
    fn enterprise_mcp_snapshot_reads_servers() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join(ENTERPRISE_MCP_FILE),
            serde_json::json!({
                "mcpServers": {
                    "enterprise": {
                        "type": "stdio",
                        "command": "npx",
                        "args": ["enterprise-server"]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_enterprise_mcp_snapshot(root).unwrap();

        assert!(snapshot.warnings.is_empty());
        assert!(snapshot.servers.contains_key("enterprise"));
    }
}
