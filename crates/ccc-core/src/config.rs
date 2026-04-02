use std::collections::HashMap;

/// Corresponds to TS `GlobalConfig` (src/utils/config.ts).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalConfig {
    pub has_completed_onboarding: bool,
    pub theme: Theme,
    pub preferred_notify_sound: bool,
    pub custom_api_key_responses: HashMap<String, String>,
    pub mcps_agreed_to_terms: bool,
    pub auto_updater_disabled: bool,
    pub mcp_servers: HashMap<String, McpServerConfig>,
    pub projects: HashMap<String, ProjectConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpServerConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    Sse {
        url: String,
        headers: HashMap<String, String>,
        headers_helper: Option<String>,
    },
    Http {
        url: String,
        headers: HashMap<String, String>,
        headers_helper: Option<String>,
    },
    Ws {
        url: String,
        headers: HashMap<String, String>,
        headers_helper: Option<String>,
    },
    Sdk {
        name: String,
    },
    ClaudeAiProxy {
        url: String,
        id: String,
    },
}

impl McpServerConfig {
    pub fn transport_kind(&self) -> McpTransportKind {
        match self {
            Self::Stdio { .. } => McpTransportKind::Stdio,
            Self::Sse { .. } => McpTransportKind::Sse,
            Self::Http { .. } => McpTransportKind::Http,
            Self::Ws { .. } => McpTransportKind::Ws,
            Self::Sdk { .. } => McpTransportKind::Sdk,
            Self::ClaudeAiProxy { .. } => McpTransportKind::ClaudeAiProxy,
        }
    }

    pub fn stdio_parts(&self) -> Option<(&str, &[String], &HashMap<String, String>)> {
        match self {
            Self::Stdio { command, args, env } => Some((command.as_str(), args.as_slice(), env)),
            _ => None,
        }
    }
}

impl serde::Serialize for McpServerConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        McpTaggedServerConfig::from(self.clone()).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for McpServerConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match RawMcpServerConfig::deserialize(deserializer)? {
            RawMcpServerConfig::LegacyStdio(config) => Ok(Self::Stdio {
                command: config.command,
                args: config.args,
                env: config.env,
            }),
            RawMcpServerConfig::Tagged(config) => Ok(config.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransportKind {
    Stdio,
    Sse,
    Http,
    Ws,
    Sdk,
    ClaudeAiProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpSourceScope {
    Global,
    Project,
    Local,
    BuiltinPlugin,
    Plugin,
    Managed,
    Enterprise,
    Dynamic,
    ClaudeAi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpConnectionStatus {
    Pending,
    Connected,
    Failed,
    NeedsAuth,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedSettingsFreshness {
    Fresh,
    Stale,
    Missing,
    Ineligible,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteManagedEligibility {
    Eligible,
    Ineligible,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpPolicyDecisionKind {
    Allowed,
    BlockedByPluginOnlyPolicy,
    BlockedByDenylist,
    BlockedByAllowlist,
    DisabledByProject,
    DisabledByBuiltinDefault,
    SuppressedDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMcpServer {
    pub name: String,
    pub config: McpServerConfig,
    pub source_scope: McpSourceScope,
    pub source_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedup_signature: Option<String>,
    pub default_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPolicyDecision {
    pub name: String,
    pub kind: McpPolicyDecisionKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMcpServer {
    pub server: ResolvedMcpServer,
    pub initial_status: McpConnectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedMcpServer {
    pub server: ResolvedMcpServer,
    pub decision: McpPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpBootstrapPlan {
    pub planned: Vec<PlannedMcpServer>,
    pub blocked: Vec<BlockedMcpServer>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionSnapshot {
    pub name: String,
    pub transport: McpTransportKind,
    pub status: McpConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_reconnect_attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub source_scope: McpSourceScope,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpControlAction {
    Reconnect,
    Enable,
    Disable,
    RefreshAuth,
    ReplacePlan,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteManagedSettingsCache {
    pub uuid: String,
    pub checksum: String,
    pub fetched_at_unix_ms: i64,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSettingsSnapshot {
    pub merged_settings: serde_json::Value,
    pub freshness: ManagedSettingsFreshness,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_cache: Option<RemoteManagedSettingsCache>,
}

impl ManagedSettingsSnapshot {
    pub fn missing() -> Self {
        Self {
            merged_settings: serde_json::json!({}),
            freshness: ManagedSettingsFreshness::Missing,
            warnings: Vec::new(),
            remote_cache: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStdioMcpServerConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum McpTaggedServerConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers_helper: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers_helper: Option<String>,
    },
    Ws {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers_helper: Option<String>,
    },
    Sdk {
        name: String,
    },
    ClaudeAiProxy {
        url: String,
        id: String,
    },
}

impl From<McpTaggedServerConfig> for McpServerConfig {
    fn from(value: McpTaggedServerConfig) -> Self {
        match value {
            McpTaggedServerConfig::Stdio { command, args, env } => Self::Stdio { command, args, env },
            McpTaggedServerConfig::Sse {
                url,
                headers,
                headers_helper,
            } => Self::Sse {
                url,
                headers,
                headers_helper,
            },
            McpTaggedServerConfig::Http {
                url,
                headers,
                headers_helper,
            } => Self::Http {
                url,
                headers,
                headers_helper,
            },
            McpTaggedServerConfig::Ws {
                url,
                headers,
                headers_helper,
            } => Self::Ws {
                url,
                headers,
                headers_helper,
            },
            McpTaggedServerConfig::Sdk { name } => Self::Sdk { name },
            McpTaggedServerConfig::ClaudeAiProxy { url, id } => Self::ClaudeAiProxy { url, id },
        }
    }
}

impl From<McpServerConfig> for McpTaggedServerConfig {
    fn from(value: McpServerConfig) -> Self {
        match value {
            McpServerConfig::Stdio { command, args, env } => Self::Stdio { command, args, env },
            McpServerConfig::Sse {
                url,
                headers,
                headers_helper,
            } => Self::Sse {
                url,
                headers,
                headers_helper,
            },
            McpServerConfig::Http {
                url,
                headers,
                headers_helper,
            } => Self::Http {
                url,
                headers,
                headers_helper,
            },
            McpServerConfig::Ws {
                url,
                headers,
                headers_helper,
            } => Self::Ws {
                url,
                headers,
                headers_helper,
            },
            McpServerConfig::Sdk { name } => Self::Sdk { name },
            McpServerConfig::ClaudeAiProxy { url, id } => Self::ClaudeAiProxy { url, id },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum RawMcpServerConfig {
    LegacyStdio(LegacyStdioMcpServerConfig),
    Tagged(McpTaggedServerConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// Corresponds to TS `ProjectConfig` (src/utils/config.ts).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectConfig {
    pub allowed_tools: Vec<String>,
    pub mcp_context_uris: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_trust_dialog_accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cost: Option<f64>,
    pub project_onboarding_seen_count: u32,
    pub enabled_mcp_servers: Vec<String>,
    pub disabled_mcp_servers: Vec<String>,
    pub enabled_mcp_json_servers: Vec<String>,
    pub disabled_mcp_json_servers: Vec<String>,
    pub enable_all_project_mcp_servers: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_config_default_serde() {
        let cfg = GlobalConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn project_config_optional_fields_omitted() {
        let cfg = ProjectConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("hasTrustDialogAccepted"),
            "should be omitted when None"
        );
        assert!(
            !json.contains("lastSessionId"),
            "should be omitted when None"
        );
    }

    #[test]
    fn theme_serde() {
        assert_eq!(serde_json::to_string(&Theme::Dark).unwrap(), "\"dark\"");
        assert_eq!(serde_json::to_string(&Theme::Light).unwrap(), "\"light\"");
    }

    #[test]
    fn global_config_unknown_fields_ignored() {
        let json = r#"{"hasCompletedOnboarding":true,"unknownFutureField":42}"#;
        let cfg: GlobalConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.has_completed_onboarding);
    }

    #[test]
    fn global_config_preserves_projects_map() {
        let mut cfg = GlobalConfig::default();
        cfg.projects.insert(
            "/tmp/demo".into(),
            ProjectConfig {
                allowed_tools: vec!["bash".into()],
                ..ProjectConfig::default()
            },
        );

        let json = serde_json::to_string(&cfg).unwrap();
        let back: GlobalConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            back.projects["/tmp/demo"].allowed_tools,
            vec!["bash".to_string()]
        );
    }

    #[test]
    fn legacy_stdio_mcp_server_config_deserializes_without_type() {
        let json = r#"{"command":"npx","args":["server"],"env":{"A":"1"}}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();

        assert_eq!(
            config,
            McpServerConfig::Stdio {
                command: "npx".into(),
                args: vec!["server".into()],
                env: HashMap::from([(String::from("A"), String::from("1"))]),
            }
        );
    }

    #[test]
    fn remote_mcp_server_config_roundtrips_with_explicit_type() {
        let json = r#"{"type":"sse","url":"https://example.com/sse","headers":{"Authorization":"Bearer token"}}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();

        assert_eq!(
            config,
            McpServerConfig::Sse {
                url: "https://example.com/sse".into(),
                headers: HashMap::from([(
                    String::from("Authorization"),
                    String::from("Bearer token"),
                )]),
                headers_helper: None,
            }
        );

        let encoded = serde_json::to_string(&config).unwrap();
        assert!(encoded.contains("\"type\":\"sse\""));
        let decoded: McpServerConfig = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn bootstrap_plan_serializes_with_planned_and_blocked_servers() {
        let plan = McpBootstrapPlan::default();
        let json = serde_json::to_string(&plan).unwrap();

        assert!(json.contains("planned"));
        assert!(json.contains("blocked"));
    }

    #[test]
    fn canonical_enabled_disabled_fields_roundtrip() {
        let json = r#"{"enabledMcpServers":["a"],"disabledMcpServers":["b"]}"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();

        assert_eq!(cfg.enabled_mcp_servers, vec!["a".to_string()]);
        assert_eq!(cfg.disabled_mcp_servers, vec!["b".to_string()]);
    }
}
