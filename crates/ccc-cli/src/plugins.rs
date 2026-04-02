use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ccc_core::{McpSourceScope, config::McpServerConfig};
use serde::Deserialize;

use crate::error::CliError;

pub const BUILTIN_PLUGIN_MCP_FILE: &str = "builtin-mcp-providers.json";
pub const ENABLED_PLUGIN_MCP_FILE: &str = "enabled-mcp-providers.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMcpServer {
    pub name: String,
    pub config: McpServerConfig,
    pub default_enabled: bool,
    pub dedup_signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMcpSource {
    pub source_scope: McpSourceScope,
    pub plugin_source: String,
    pub marketplace: Option<String>,
    pub channel_capable: bool,
    pub source_label: String,
    pub servers: Vec<PluginMcpServer>,
}

pub trait PluginMcpSourceLoader {
    fn load_builtin_sources(&self) -> Result<Vec<PluginMcpSource>, CliError>;
    fn load_enabled_sources(&self) -> Result<Vec<PluginMcpSource>, CliError>;
}

#[derive(Debug, Clone)]
pub struct FilesystemPluginMcpSourceLoader {
    root: PathBuf,
}

impl FilesystemPluginMcpSourceLoader {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn load_file(
        &self,
        path: &Path,
        source_scope: McpSourceScope,
    ) -> Result<Vec<PluginMcpSource>, CliError> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let text = fs::read_to_string(path)?;
        let providers: Vec<RawPluginProvider> = serde_json::from_str(&text)?;
        Ok(providers
            .into_iter()
            .map(|provider| provider.into_source(source_scope, path))
            .collect())
    }
}

impl PluginMcpSourceLoader for FilesystemPluginMcpSourceLoader {
    fn load_builtin_sources(&self) -> Result<Vec<PluginMcpSource>, CliError> {
        self.load_file(&self.root.join(BUILTIN_PLUGIN_MCP_FILE), McpSourceScope::BuiltinPlugin)
    }

    fn load_enabled_sources(&self) -> Result<Vec<PluginMcpSource>, CliError> {
        self.load_file(&self.root.join(ENABLED_PLUGIN_MCP_FILE), McpSourceScope::Plugin)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawPluginProvider {
    name: String,
    marketplace: Option<String>,
    channel_capable: bool,
    servers: HashMap<String, RawPluginServer>,
}

impl Default for RawPluginProvider {
    fn default() -> Self {
        Self {
            name: String::new(),
            marketplace: None,
            channel_capable: false,
            servers: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawPluginServer {
    #[serde(flatten)]
    config: McpServerConfig,
    default_enabled: bool,
    dedup_signature: Option<String>,
}

impl Default for RawPluginServer {
    fn default() -> Self {
        Self {
            config: McpServerConfig::Sdk {
                name: String::new(),
            },
            default_enabled: true,
            dedup_signature: None,
        }
    }
}

impl RawPluginProvider {
    fn into_source(self, source_scope: McpSourceScope, source_path: &Path) -> PluginMcpSource {
        let plugin_source = match &self.marketplace {
            Some(marketplace) => format!("{}@{marketplace}", self.name),
            None => self.name.clone(),
        };

        PluginMcpSource {
            source_scope,
            plugin_source: plugin_source.clone(),
            marketplace: self.marketplace,
            channel_capable: self.channel_capable,
            source_label: format!("plugin:{} ({})", plugin_source, source_path.display()),
            servers: self
                .servers
                .into_iter()
                .map(|(name, server)| PluginMcpServer {
                    name,
                    config: server.config,
                    default_enabled: server.default_enabled,
                    dedup_signature: server.dedup_signature,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_plugin_snapshot_files_return_empty_sources() {
        let temp = tempfile::tempdir().unwrap();
        let loader = FilesystemPluginMcpSourceLoader::new(temp.path().to_path_buf());

        assert!(loader.load_builtin_sources().unwrap().is_empty());
        assert!(loader.load_enabled_sources().unwrap().is_empty());
    }

    #[test]
    fn filesystem_loader_reads_plugin_source_snapshots() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(ENABLED_PLUGIN_MCP_FILE),
            serde_json::json!([
                {
                    "name": "slack",
                    "marketplace": "anthropic",
                    "channelCapable": true,
                    "servers": {
                        "slack-main": {
                            "type": "stdio",
                            "command": "npx",
                            "args": ["slack-mcp"]
                        }
                    }
                }
            ])
            .to_string(),
        )
        .unwrap();

        let loader = FilesystemPluginMcpSourceLoader::new(temp.path().to_path_buf());
        let sources = loader.load_enabled_sources().unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].plugin_source, "slack@anthropic");
        assert!(sources[0].channel_capable);
        assert_eq!(sources[0].servers[0].name, "slack-main");
    }
}
