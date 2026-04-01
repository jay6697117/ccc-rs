use std::path::PathBuf;

use ccc_core::{config::McpServerConfig, normalize_project_key};

use crate::cli::ChatArgs;
use crate::commands::config::ConfigSnapshot;
use crate::error::CliError;

pub const DEFAULT_MODEL: &str = "claude-opus-4-6";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    ResumeLast,
    Ephemeral,
}

#[derive(Debug, Clone)]
pub struct ChatRuntimeConfig {
    pub model: String,
    pub system_prompt: Option<String>,
    pub project_key: String,
    pub session_mode: SessionMode,
    pub mcp_servers: Vec<(String, McpServerConfig)>,
}

pub fn build_chat_runtime(
    args: ChatArgs,
    snapshot: &ConfigSnapshot,
    cwd: PathBuf,
) -> Result<ChatRuntimeConfig, CliError> {
    Ok(ChatRuntimeConfig {
        model: args.model.unwrap_or_else(|| DEFAULT_MODEL.into()),
        system_prompt: args.system_prompt,
        project_key: normalize_project_key(&cwd),
        session_mode: if args.print {
            SessionMode::Ephemeral
        } else {
            SessionMode::ResumeLast
        },
        mcp_servers: select_mcp_servers(snapshot),
    })
}

fn select_mcp_servers(snapshot: &ConfigSnapshot) -> Vec<(String, McpServerConfig)> {
    let mut enabled = Vec::new();

    for (name, config) in &snapshot.global.mcp_servers {
        if snapshot
            .project
            .disabled_mcp_json_servers
            .iter()
            .any(|disabled| disabled == name)
        {
            continue;
        }

        let explicitly_enabled = snapshot
            .project
            .enabled_mcp_json_servers
            .iter()
            .any(|allowed| allowed == name);

        if explicitly_enabled || snapshot.project.enable_all_project_mcp_servers {
            enabled.push((name.clone(), config.clone()));
        }
    }

    enabled
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ccc_core::{config::McpServerConfig, GlobalConfig, ProjectConfig};

    use crate::cli::{ChatArgs, OutputFormat};
    use crate::commands::config::ConfigSnapshot;

    use super::{build_chat_runtime, SessionMode};

    fn snapshot(project: ProjectConfig, global: GlobalConfig) -> ConfigSnapshot {
        ConfigSnapshot {
            global,
            project,
            project_settings: None,
            project_local_settings: None,
            global_path: PathBuf::from("/tmp/settings.json"),
        }
    }

    fn sample_server(command: &str) -> McpServerConfig {
        McpServerConfig {
            command: command.into(),
            args: vec![],
            env: Default::default(),
        }
    }

    #[test]
    fn interactive_chat_uses_resume_last_mode() {
        let runtime = build_chat_runtime(
            ChatArgs {
                model: None,
                system_prompt: None,
                print: false,
                output_format: OutputFormat::Text,
                include_partial_messages: false,
                prompt: vec![],
            },
            &snapshot(ProjectConfig::default(), GlobalConfig::default()),
            PathBuf::from("/tmp/project"),
        )
        .unwrap();

        assert_eq!(runtime.session_mode, SessionMode::ResumeLast);
    }

    #[test]
    fn print_chat_uses_ephemeral_mode() {
        let runtime = build_chat_runtime(
            ChatArgs {
                model: None,
                system_prompt: None,
                print: true,
                output_format: OutputFormat::Text,
                include_partial_messages: false,
                prompt: vec![],
            },
            &snapshot(ProjectConfig::default(), GlobalConfig::default()),
            PathBuf::from("/tmp/project"),
        )
        .unwrap();

        assert_eq!(runtime.session_mode, SessionMode::Ephemeral);
    }

    #[test]
    fn mcp_selection_respects_disabled_then_enabled_then_enable_all() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("allowed".into(), sample_server("allowed"));
        global
            .mcp_servers
            .insert("blocked".into(), sample_server("blocked"));
        global
            .mcp_servers
            .insert("fallback".into(), sample_server("fallback"));

        let project = ProjectConfig {
            enabled_mcp_json_servers: vec!["allowed".into()],
            disabled_mcp_json_servers: vec!["blocked".into()],
            enable_all_project_mcp_servers: true,
            ..ProjectConfig::default()
        };

        let runtime = build_chat_runtime(
            ChatArgs {
                model: None,
                system_prompt: None,
                print: false,
                output_format: OutputFormat::Text,
                include_partial_messages: false,
                prompt: vec![],
            },
            &snapshot(project, global),
            PathBuf::from("/tmp/project"),
        )
        .unwrap();

        let enabled: Vec<_> = runtime
            .mcp_servers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        assert!(enabled.contains(&"allowed"));
        assert!(enabled.contains(&"fallback"));
        assert!(!enabled.contains(&"blocked"));
    }
}
