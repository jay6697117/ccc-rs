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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
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
}
