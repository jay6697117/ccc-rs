use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ccc_core::{
    BlockedMcpServer, McpBootstrapPlan, McpConnectionStatus, McpPolicyDecision,
    McpPolicyDecisionKind, McpSourceScope, PlannedMcpServer, ResolvedMcpServer,
    claude_config_dir,
    config::McpServerConfig,
    normalize_project_key,
};
use serde_json::Value;

use crate::cli::ChatArgs;
use crate::commands::config::ConfigSnapshot;
use crate::error::CliError;
use crate::plugins::{
    FilesystemPluginMcpSourceLoader, PluginMcpSource, PluginMcpSourceLoader,
};

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
    pub mcp_bootstrap: McpBootstrapPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpPolicyEntry {
    ServerName(String),
    ServerCommand(Vec<String>),
    ServerUrl(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ManagedMcpPolicy {
    allow_managed_mcp_servers_only: bool,
    strict_plugin_only_mcp: bool,
    allowed_mcp_servers: Option<Vec<McpPolicyEntry>>,
    denied_mcp_servers: Vec<McpPolicyEntry>,
    blocked_marketplaces: HashSet<String>,
    strict_known_marketplaces: HashSet<String>,
    channels_enabled: bool,
    allowed_channel_plugins: Option<HashSet<String>>,
    warnings: Vec<String>,
}

pub fn build_chat_runtime(
    args: ChatArgs,
    snapshot: &ConfigSnapshot,
    cwd: PathBuf,
) -> Result<ChatRuntimeConfig, CliError> {
    let loader = FilesystemPluginMcpSourceLoader::new(claude_config_dir().join("plugins"));
    build_chat_runtime_with_loader(args, snapshot, cwd, &loader)
}

fn build_chat_runtime_with_loader<L>(
    args: ChatArgs,
    snapshot: &ConfigSnapshot,
    cwd: PathBuf,
    loader: &L,
) -> Result<ChatRuntimeConfig, CliError>
where
    L: PluginMcpSourceLoader,
{
    let mcp_bootstrap = build_mcp_bootstrap(snapshot, loader);
    let mcp_servers = mcp_bootstrap
        .planned
        .iter()
        .map(|planned| (planned.server.name.clone(), planned.server.config.clone()))
        .collect();

    Ok(ChatRuntimeConfig {
        model: args.model.unwrap_or_else(|| DEFAULT_MODEL.into()),
        system_prompt: args.system_prompt,
        project_key: normalize_project_key(&cwd),
        session_mode: if args.print {
            SessionMode::Ephemeral
        } else {
            SessionMode::ResumeLast
        },
        mcp_servers,
        mcp_bootstrap,
    })
}

fn build_mcp_bootstrap<L>(snapshot: &ConfigSnapshot, loader: &L) -> McpBootstrapPlan
where
    L: PluginMcpSourceLoader,
{
    let mut plan = McpBootstrapPlan::default();
    let policy = ManagedMcpPolicy::from_snapshot(snapshot);
    plan.warnings.extend(snapshot.managed_settings.warnings.clone());
    plan.warnings.extend(snapshot.enterprise_mcp.warnings.clone());
    plan.warnings.extend(policy.warnings.clone());

    if !snapshot.enterprise_mcp.servers.is_empty() {
        let enterprise_sources = snapshot
            .enterprise_mcp
            .servers
            .iter()
            .map(|(name, config)| ResolvedMcpServer {
                name: name.clone(),
                config: config.clone(),
                source_scope: McpSourceScope::Enterprise,
                source_label: snapshot.enterprise_mcp.source_label.clone(),
                plugin_source: None,
                dedup_signature: Some(default_dedup_signature(config)),
                default_enabled: true,
            })
            .collect::<Vec<_>>();
        finalize_candidates(snapshot, &policy, enterprise_sources, &mut plan);
        return plan;
    }

    let manual_sources = merge_manual_sources(snapshot);
    let mut candidates = Vec::new();

    if policy.strict_plugin_only_mcp {
        for server in manual_sources {
            plan.blocked.push(blocked_server(
                server,
                McpPolicyDecisionKind::BlockedByPluginOnlyPolicy,
                "manual MCP configuration is disabled by strictPluginOnlyCustomization".into(),
            ));
        }
    } else {
        candidates.extend(manual_sources);
    }

    let plugin_sources = load_plugin_sources(loader, &mut plan.warnings);
    candidates.extend(merge_plugin_sources(plugin_sources, &candidates, &policy, &mut plan));

    finalize_candidates(snapshot, &policy, candidates, &mut plan);
    plan
}

fn finalize_candidates(
    snapshot: &ConfigSnapshot,
    policy: &ManagedMcpPolicy,
    candidates: Vec<ResolvedMcpServer>,
    plan: &mut McpBootstrapPlan,
) {
    let enabled_servers = canonical_or_legacy_enabled(snapshot);
    let disabled_servers = canonical_or_legacy_disabled(snapshot);

    for server in candidates {
        if let Some(decision) = evaluate_policy(&server, policy) {
            plan.blocked.push(BlockedMcpServer {
                server,
                decision,
            });
            continue;
        }

        if disabled_servers.iter().any(|disabled| disabled == &server.name) {
            plan.blocked.push(blocked_server(
                server,
                McpPolicyDecisionKind::DisabledByProject,
                "server is disabled by project configuration".into(),
            ));
            continue;
        }

        let explicitly_enabled = enabled_servers.iter().any(|enabled| enabled == &server.name);
        let enabled_by_default = match server.source_scope {
            McpSourceScope::BuiltinPlugin => server.default_enabled,
            _ => true,
        };

        if matches!(server.source_scope, McpSourceScope::BuiltinPlugin)
            && !server.default_enabled
            && !explicitly_enabled
        {
            plan.blocked.push(blocked_server(
                server,
                McpPolicyDecisionKind::DisabledByBuiltinDefault,
                "builtin MCP server requires explicit opt-in".into(),
            ));
            continue;
        }

        if explicitly_enabled
            || snapshot.project.enable_all_project_mcp_servers
            || enabled_by_default
        {
            plan.planned.push(PlannedMcpServer {
                server,
                initial_status: McpConnectionStatus::Pending,
            });
            continue;
        }

        plan.blocked.push(blocked_server(
            server,
            McpPolicyDecisionKind::DisabledByProject,
            "server is not enabled by the current project configuration".into(),
        ));
    }
}

fn evaluate_policy(server: &ResolvedMcpServer, policy: &ManagedMcpPolicy) -> Option<McpPolicyDecision> {
    if matches_policy_entries(&policy.denied_mcp_servers, server) {
        return Some(McpPolicyDecision {
            name: server.name.clone(),
            kind: McpPolicyDecisionKind::BlockedByDenylist,
            message: "server is blocked by denylist policy".into(),
        });
    }

    match &policy.allowed_mcp_servers {
        None => None,
        Some(entries) if entries.is_empty() => Some(McpPolicyDecision {
            name: server.name.clone(),
            kind: McpPolicyDecisionKind::BlockedByAllowlist,
            message: "server is not allowed by policy allowlist".into(),
        }),
        Some(entries) if is_allowed_by_entries(entries, server) => None,
        Some(_) => Some(McpPolicyDecision {
            name: server.name.clone(),
            kind: McpPolicyDecisionKind::BlockedByAllowlist,
            message: "server is not allowed by policy allowlist".into(),
        }),
    }
}

fn load_plugin_sources<L>(loader: &L, warnings: &mut Vec<String>) -> Vec<PluginMcpSource>
where
    L: PluginMcpSourceLoader,
{
    let mut sources = Vec::new();

    match loader.load_builtin_sources() {
        Ok(builtin) => sources.extend(builtin),
        Err(error) => warnings.push(format!("failed to load builtin plugin MCP sources: {error}")),
    }

    match loader.load_enabled_sources() {
        Ok(enabled) => sources.extend(enabled),
        Err(error) => warnings.push(format!("failed to load plugin MCP sources: {error}")),
    }

    sources
}

fn merge_manual_sources(snapshot: &ConfigSnapshot) -> Vec<ResolvedMcpServer> {
    let mut merged = HashMap::new();

    for (name, config) in &snapshot.global.mcp_servers {
        merged.insert(
            name.clone(),
            ResolvedMcpServer {
                name: name.clone(),
                config: config.clone(),
                source_scope: McpSourceScope::Global,
                source_label: snapshot.global_path.display().to_string(),
                plugin_source: None,
                dedup_signature: Some(default_dedup_signature(config)),
                default_enabled: true,
            },
        );
    }

    for server in extract_scoped_mcp_servers(
        snapshot.project_settings.as_ref(),
        McpSourceScope::Project,
        &snapshot.project_settings_path,
    ) {
        merged.insert(server.name.clone(), server);
    }

    for server in extract_scoped_mcp_servers(
        snapshot.project_local_settings.as_ref(),
        McpSourceScope::Local,
        &snapshot.project_local_settings_path,
    ) {
        merged.insert(server.name.clone(), server);
    }

    merged.into_values().collect()
}

fn merge_plugin_sources(
    sources: Vec<PluginMcpSource>,
    manual_sources: &[ResolvedMcpServer],
    policy: &ManagedMcpPolicy,
    plan: &mut McpBootstrapPlan,
) -> Vec<ResolvedMcpServer> {
    let manual_names = manual_sources
        .iter()
        .map(|server| server.name.clone())
        .collect::<HashSet<_>>();
    let manual_signatures = manual_sources
        .iter()
        .filter_map(|server| server.dedup_signature.clone())
        .collect::<HashSet<_>>();
    let mut seen_names = HashSet::new();
    let mut seen_signatures = HashSet::new();
    let mut merged = Vec::new();

    for source in sources {
        if let Some(reason) = plugin_source_block_reason(&source, policy, plan) {
            plan.warnings.push(reason);
            continue;
        }

        for server in source.servers {
            let dedup_signature = server
                .dedup_signature
                .clone()
                .or_else(|| Some(default_dedup_signature(&server.config)));
            let resolved = ResolvedMcpServer {
                name: server.name.clone(),
                config: server.config.clone(),
                source_scope: source.source_scope,
                source_label: source.source_label.clone(),
                plugin_source: Some(source.plugin_source.clone()),
                dedup_signature: dedup_signature.clone(),
                default_enabled: server.default_enabled,
            };

            let duplicate = manual_names.contains(&resolved.name)
                || dedup_signature
                    .as_ref()
                    .is_some_and(|signature| manual_signatures.contains(signature))
                || seen_names.contains(&resolved.name)
                || dedup_signature
                    .as_ref()
                    .is_some_and(|signature| seen_signatures.contains(signature));

            if duplicate {
                plan.blocked.push(blocked_server(
                    resolved,
                    McpPolicyDecisionKind::SuppressedDuplicate,
                    "plugin MCP server is suppressed by a higher-priority duplicate".into(),
                ));
                continue;
            }

            seen_names.insert(server.name);
            if let Some(signature) = dedup_signature {
                seen_signatures.insert(signature);
            }
            merged.push(resolved);
        }
    }

    merged
}

fn plugin_source_block_reason(
    source: &PluginMcpSource,
    policy: &ManagedMcpPolicy,
    plan: &mut McpBootstrapPlan,
) -> Option<String> {
    if let Some(marketplace) = &source.marketplace {
        if policy.blocked_marketplaces.contains(marketplace) {
            return Some(format!(
                "plugin MCP source {} is blocked by managed marketplace policy",
                source.plugin_source
            ));
        }
        if !policy.strict_known_marketplaces.is_empty()
            && !policy.strict_known_marketplaces.contains(marketplace)
        {
            return Some(format!(
                "plugin MCP source {} is not in strictKnownMarketplaces",
                source.plugin_source
            ));
        }
    } else if !policy.strict_known_marketplaces.is_empty() {
        return Some(format!(
            "plugin MCP source {} has no marketplace metadata under strictKnownMarketplaces",
            source.plugin_source
        ));
    }

    if source.channel_capable {
        if !policy.channels_enabled {
            if policy.allowed_channel_plugins.is_some() {
                plan.warnings.push(
                    "allowedChannelPlugins is set but channelsEnabled is false; channel plugins remain disabled"
                        .into(),
                );
            }
            return Some(format!(
                "plugin MCP source {} is channel-capable but channels are disabled",
                source.plugin_source
            ));
        }

        if let Some(allowed) = &policy.allowed_channel_plugins {
            let base_name = source
                .plugin_source
                .split('@')
                .next()
                .unwrap_or(source.plugin_source.as_str());
            if !allowed.contains(&source.plugin_source) && !allowed.contains(base_name) {
                return Some(format!(
                    "plugin MCP source {} is not in allowedChannelPlugins",
                    source.plugin_source
                ));
            }
        }
    }

    None
}

fn extract_scoped_mcp_servers(
    settings: Option<&Value>,
    scope: McpSourceScope,
    path: &Path,
) -> Vec<ResolvedMcpServer> {
    settings
        .and_then(|value| value.get("mcpServers"))
        .and_then(Value::as_object)
        .map(|servers| {
            servers
                .iter()
                .filter_map(|(name, raw)| {
                    serde_json::from_value::<McpServerConfig>(raw.clone())
                        .ok()
                        .map(|config| ResolvedMcpServer {
                            name: name.clone(),
                            dedup_signature: Some(default_dedup_signature(&config)),
                            config,
                            source_scope: scope,
                            source_label: path.display().to_string(),
                            plugin_source: None,
                            default_enabled: true,
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn blocked_server(
    server: ResolvedMcpServer,
    kind: McpPolicyDecisionKind,
    message: String,
) -> BlockedMcpServer {
    BlockedMcpServer {
        decision: McpPolicyDecision {
            name: server.name.clone(),
            kind,
            message,
        },
        server,
    }
}

fn default_dedup_signature(config: &McpServerConfig) -> String {
    match config {
        McpServerConfig::Stdio { command, args, .. } => {
            format!("stdio:{command}:{}", args.join("\u{0}"))
        }
        McpServerConfig::Sse { url, .. } => format!("sse:{url}"),
        McpServerConfig::Http { url, .. } => format!("http:{url}"),
        McpServerConfig::Ws { url, .. } => format!("ws:{url}"),
        McpServerConfig::Sdk { name } => format!("sdk:{name}"),
        McpServerConfig::ClaudeAiProxy { url, id } => format!("claudeai-proxy:{id}:{url}"),
    }
}

fn matches_policy_entries(entries: &[McpPolicyEntry], server: &ResolvedMcpServer) -> bool {
    entries.iter().any(|entry| matches_policy_entry(entry, server))
}

fn is_allowed_by_entries(entries: &[McpPolicyEntry], server: &ResolvedMcpServer) -> bool {
    let has_command_entries = entries
        .iter()
        .any(|entry| matches!(entry, McpPolicyEntry::ServerCommand(_)));
    let has_url_entries = entries
        .iter()
        .any(|entry| matches!(entry, McpPolicyEntry::ServerUrl(_)));

    match &server.config {
        McpServerConfig::Stdio { command, args, .. } if has_command_entries => entries.iter().any(
            |entry| match entry {
                McpPolicyEntry::ServerCommand(expected) => {
                    command_array(command, args) == expected.as_slice()
                }
                _ => false,
            },
        ),
        McpServerConfig::Sse { url, .. }
        | McpServerConfig::Http { url, .. }
        | McpServerConfig::Ws { url, .. }
        | McpServerConfig::ClaudeAiProxy { url, .. }
            if has_url_entries =>
        {
            entries.iter().any(|entry| match entry {
                McpPolicyEntry::ServerUrl(pattern) => wildcard_matches(url, pattern),
                _ => false,
            })
        }
        _ => entries.iter().any(|entry| match entry {
            McpPolicyEntry::ServerName(name) => name == &server.name,
            _ => false,
        }),
    }
}

fn matches_policy_entry(entry: &McpPolicyEntry, server: &ResolvedMcpServer) -> bool {
    match entry {
        McpPolicyEntry::ServerName(name) => name == &server.name,
        McpPolicyEntry::ServerCommand(expected) => match &server.config {
            McpServerConfig::Stdio { command, args, .. } => command_array(command, args) == expected.as_slice(),
            _ => false,
        },
        McpPolicyEntry::ServerUrl(pattern) => match &server.config {
            McpServerConfig::Sse { url, .. }
            | McpServerConfig::Http { url, .. }
            | McpServerConfig::Ws { url, .. }
            | McpServerConfig::ClaudeAiProxy { url, .. } => wildcard_matches(url, pattern),
            _ => false,
        },
    }
}

fn command_array<'a>(command: &'a str, args: &'a [String]) -> Vec<String> {
    std::iter::once(command.to_string())
        .chain(args.iter().cloned())
        .collect()
}

fn wildcard_matches(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return value == pattern;
    }

    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut remainder = value;

    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if index == 0 && !remainder.starts_with(part) {
            return false;
        }

        if let Some(found) = remainder.find(part) {
            remainder = &remainder[found + part.len()..];
        } else {
            return false;
        }
    }

    if !pattern.ends_with('*') {
        parts
            .last()
            .is_none_or(|part| value.ends_with(part))
    } else {
        true
    }
}

fn canonical_or_legacy_enabled(snapshot: &ConfigSnapshot) -> &[String] {
    if !snapshot.project.enabled_mcp_servers.is_empty() {
        &snapshot.project.enabled_mcp_servers
    } else {
        &snapshot.project.enabled_mcp_json_servers
    }
}

fn canonical_or_legacy_disabled(snapshot: &ConfigSnapshot) -> &[String] {
    if !snapshot.project.disabled_mcp_servers.is_empty() {
        &snapshot.project.disabled_mcp_servers
    } else {
        &snapshot.project.disabled_mcp_json_servers
    }
}

impl ManagedMcpPolicy {
    fn from_snapshot(snapshot: &ConfigSnapshot) -> Self {
        let managed = &snapshot.managed_settings.merged_settings;
        let global = snapshot.global_settings.as_ref();
        let project = snapshot.project_settings.as_ref();
        let local = snapshot.project_local_settings.as_ref();
        let null = Value::Null;

        let allow_managed_mcp_servers_only =
            managed.get("allowManagedMcpServersOnly").and_then(Value::as_bool) == Some(true);
        let strict_plugin_only_mcp = strict_plugin_only_mcp(managed);

        let allowed_sources = if allow_managed_mcp_servers_only {
            vec![managed]
        } else {
            vec![
                global.unwrap_or(&null),
                project.unwrap_or(&null),
                local.unwrap_or(&null),
                managed,
            ]
        };
        let denied_sources = vec![
            global.unwrap_or(&null),
            project.unwrap_or(&null),
            local.unwrap_or(&null),
            managed,
        ];

        let allowed_mcp_servers = extract_policy_entries(&allowed_sources, "allowedMcpServers");
        let denied_mcp_servers = extract_policy_entries(&denied_sources, "deniedMcpServers")
            .unwrap_or_default();
        let blocked_marketplaces =
            extract_string_set(managed, "blockedMarketplaces").unwrap_or_default();
        let strict_known_marketplaces =
            extract_string_set(managed, "strictKnownMarketplaces").unwrap_or_default();
        let channels_enabled = managed.get("channelsEnabled").and_then(Value::as_bool).unwrap_or(false);
        let allowed_channel_plugins = extract_string_set(managed, "allowedChannelPlugins");

        let mut warnings = Vec::new();
        if allowed_channel_plugins.is_some() && !channels_enabled {
            warnings.push(
                "allowedChannelPlugins is configured but channelsEnabled is false".into(),
            );
        }

        Self {
            allow_managed_mcp_servers_only,
            strict_plugin_only_mcp,
            allowed_mcp_servers,
            denied_mcp_servers,
            blocked_marketplaces,
            strict_known_marketplaces,
            channels_enabled,
            allowed_channel_plugins,
            warnings,
        }
    }
}

fn strict_plugin_only_mcp(settings: &Value) -> bool {
    match settings.get("strictPluginOnlyCustomization") {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Array(entries)) => entries.iter().any(|entry| entry.as_str() == Some("mcp")),
        _ => false,
    }
}

fn extract_policy_entries(sources: &[&Value], key: &str) -> Option<Vec<McpPolicyEntry>> {
    let mut entries = Vec::new();
    let mut seen_any = false;

    for source in sources {
        if let Some(array) = source.get(key).and_then(Value::as_array) {
            seen_any = true;
            for item in array {
                if let Some(name) = item.get("serverName").and_then(Value::as_str) {
                    entries.push(McpPolicyEntry::ServerName(name.to_string()));
                } else if let Some(command) = item.get("serverCommand").and_then(Value::as_array) {
                    let parts = command
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>();
                    if !parts.is_empty() {
                        entries.push(McpPolicyEntry::ServerCommand(parts));
                    }
                } else if let Some(url) = item.get("serverUrl").and_then(Value::as_str) {
                    entries.push(McpPolicyEntry::ServerUrl(url.to_string()));
                }
            }
        }
    }

    seen_any.then_some(entries)
}

fn extract_string_set(settings: &Value, key: &str) -> Option<HashSet<String>> {
    settings.get(key).and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ccc_core::{GlobalConfig, ManagedSettingsSnapshot, ProjectConfig};

    use crate::cli::{ChatArgs, OutputFormat};
    use crate::commands::config::ConfigSnapshot;
    use crate::managed::EnterpriseMcpSnapshot;
    use crate::plugins::{PluginMcpServer, PluginMcpSource, PluginMcpSourceLoader};

    use super::{build_chat_runtime, build_chat_runtime_with_loader, SessionMode};
    use super::*;

    #[derive(Default)]
    struct TestLoader {
        builtin: Vec<PluginMcpSource>,
        enabled: Vec<PluginMcpSource>,
    }

    impl PluginMcpSourceLoader for TestLoader {
        fn load_builtin_sources(&self) -> Result<Vec<PluginMcpSource>, CliError> {
            Ok(self.builtin.clone())
        }

        fn load_enabled_sources(&self) -> Result<Vec<PluginMcpSource>, CliError> {
            Ok(self.enabled.clone())
        }
    }

    fn snapshot(project: ProjectConfig, global: GlobalConfig) -> ConfigSnapshot {
        ConfigSnapshot {
            global,
            global_settings: None,
            project,
            project_settings: None,
            project_local_settings: None,
            managed_settings: ManagedSettingsSnapshot::missing(),
            enterprise_mcp: EnterpriseMcpSnapshot::default(),
            global_path: PathBuf::from("/tmp/settings.json"),
            project_settings_path: PathBuf::from("/tmp/.claude/settings.json"),
            project_local_settings_path: PathBuf::from("/tmp/.claude/settings.local.json"),
        }
    }

    fn sample_server(command: &str) -> McpServerConfig {
        McpServerConfig::Stdio {
            command: command.into(),
            args: vec![],
            env: Default::default(),
        }
    }

    fn args(print: bool) -> ChatArgs {
        ChatArgs {
            model: None,
            system_prompt: None,
            print,
            output_format: OutputFormat::Text,
            include_partial_messages: false,
            prompt: vec![],
        }
    }

    fn plugin_source(
        scope: McpSourceScope,
        plugin_source: &str,
        marketplace: Option<&str>,
        name: &str,
        command: &str,
    ) -> PluginMcpSource {
        PluginMcpSource {
            source_scope: scope,
            plugin_source: plugin_source.into(),
            marketplace: marketplace.map(|value| value.into()),
            channel_capable: false,
            source_label: format!("plugin:{plugin_source}"),
            servers: vec![PluginMcpServer {
                name: name.into(),
                config: sample_server(command),
                default_enabled: true,
                dedup_signature: None,
            }],
        }
    }

    #[test]
    fn interactive_chat_uses_resume_last_mode() {
        let runtime = build_chat_runtime(
            args(false),
            &snapshot(ProjectConfig::default(), GlobalConfig::default()),
            PathBuf::from("/tmp/project"),
        )
        .unwrap();

        assert_eq!(runtime.session_mode, SessionMode::ResumeLast);
    }

    #[test]
    fn print_chat_uses_ephemeral_mode() {
        let runtime = build_chat_runtime(
            args(true),
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

        let runtime = build_chat_runtime(args(false), &snapshot(project, global), PathBuf::from("/tmp/project")).unwrap();

        let planned: Vec<_> = runtime
            .mcp_bootstrap
            .planned
            .iter()
            .map(|server| server.server.name.as_str())
            .collect();

        assert!(planned.contains(&"allowed"));
        assert!(planned.contains(&"fallback"));
        assert!(!planned.contains(&"blocked"));
        assert_eq!(runtime.mcp_bootstrap.blocked.len(), 1);
        assert_eq!(
            runtime.mcp_bootstrap.blocked[0].decision.kind,
            ccc_core::McpPolicyDecisionKind::DisabledByProject
        );
    }

    #[test]
    fn build_chat_runtime_returns_bootstrap_plan() {
        let runtime = build_chat_runtime(
            args(false),
            &snapshot(ProjectConfig::default(), GlobalConfig::default()),
            PathBuf::from("/tmp/project"),
        )
        .unwrap();

        assert!(runtime.mcp_bootstrap.planned.is_empty());
        assert!(runtime.mcp_bootstrap.blocked.is_empty());
    }

    #[test]
    fn disabled_server_becomes_blocked_decision() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("blocked".into(), sample_server("blocked"));

        let project = ProjectConfig {
            disabled_mcp_json_servers: vec!["blocked".into()],
            ..ProjectConfig::default()
        };

        let runtime = build_chat_runtime(args(false), &snapshot(project, global), PathBuf::from("/tmp/project")).unwrap();

        assert_eq!(runtime.mcp_bootstrap.blocked.len(), 1);
    }

    #[test]
    fn canonical_enabled_disabled_fields_override_legacy_fields() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("legacy-blocked".into(), sample_server("legacy-blocked"));
        global
            .mcp_servers
            .insert("canonical-allowed".into(), sample_server("canonical-allowed"));

        let project = ProjectConfig {
            enabled_mcp_json_servers: vec!["legacy-blocked".into()],
            disabled_mcp_json_servers: vec![],
            enabled_mcp_servers: vec!["canonical-allowed".into()],
            disabled_mcp_servers: vec!["legacy-blocked".into()],
            ..ProjectConfig::default()
        };

        let runtime = build_chat_runtime(args(false), &snapshot(project, global), PathBuf::from("/tmp/project")).unwrap();

        let planned: Vec<_> = runtime
            .mcp_bootstrap
            .planned
            .iter()
            .map(|server| server.server.name.as_str())
            .collect();
        let blocked: Vec<_> = runtime
            .mcp_bootstrap
            .blocked
            .iter()
            .map(|server| server.server.name.as_str())
            .collect();

        assert!(planned.contains(&"canonical-allowed"));
        assert!(blocked.contains(&"legacy-blocked"));
    }

    #[test]
    fn plugin_only_policy_skips_manual_sources_but_keeps_plugin_sources() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("manual".into(), sample_server("manual"));
        let mut snapshot = snapshot(ProjectConfig::default(), global);
        snapshot.managed_settings.merged_settings = serde_json::json!({
            "strictPluginOnlyCustomization": ["mcp"]
        });
        let loader = TestLoader {
            builtin: vec![plugin_source(
                McpSourceScope::BuiltinPlugin,
                "builtin-core",
                Some("anthropic"),
                "builtin-server",
                "builtin-command",
            )],
            enabled: vec![plugin_source(
                McpSourceScope::Plugin,
                "slack@anthropic",
                Some("anthropic"),
                "plugin-server",
                "plugin-command",
            )],
        };

        let runtime = build_chat_runtime_with_loader(
            args(false),
            &snapshot,
            PathBuf::from("/tmp/project"),
            &loader,
        )
        .unwrap();

        assert!(runtime
            .mcp_bootstrap
            .planned
            .iter()
            .all(|server| matches!(
                server.server.source_scope,
                McpSourceScope::Plugin | McpSourceScope::BuiltinPlugin
            )));
        assert!(runtime
            .mcp_bootstrap
            .blocked
            .iter()
            .any(|blocked| blocked.decision.kind == McpPolicyDecisionKind::BlockedByPluginOnlyPolicy));
    }

    #[test]
    fn blocked_marketplace_plugin_is_removed_before_selector() {
        let mut snapshot = snapshot(ProjectConfig::default(), GlobalConfig::default());
        snapshot.managed_settings.merged_settings = serde_json::json!({
            "blockedMarketplaces": ["blocked-market"]
        });
        let loader = TestLoader {
            enabled: vec![
                plugin_source(
                    McpSourceScope::Plugin,
                    "good@allowed",
                    Some("allowed"),
                    "good-server",
                    "good-command",
                ),
                plugin_source(
                    McpSourceScope::Plugin,
                    "bad@blocked-market",
                    Some("blocked-market"),
                    "blocked-server",
                    "blocked-command",
                ),
            ],
            ..TestLoader::default()
        };

        let runtime = build_chat_runtime_with_loader(
            args(false),
            &snapshot,
            PathBuf::from("/tmp/project"),
            &loader,
        )
        .unwrap();

        let planned: Vec<_> = runtime
            .mcp_bootstrap
            .planned
            .iter()
            .map(|server| server.server.name.as_str())
            .collect();
        assert!(planned.contains(&"good-server"));
        assert!(!planned.contains(&"blocked-server"));
        assert!(runtime
            .mcp_bootstrap
            .warnings
            .iter()
            .any(|warning| warning.contains("blocked-market")));
    }

    #[test]
    fn enterprise_servers_replace_non_enterprise_sources() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("manual".into(), sample_server("manual"));
        let mut snapshot = snapshot(ProjectConfig::default(), global);
        snapshot.enterprise_mcp = EnterpriseMcpSnapshot {
            servers: HashMap::from([("enterprise".into(), sample_server("enterprise"))]),
            warnings: Vec::new(),
            source_label: "/tmp/managed/managed-mcp.json".into(),
        };
        let loader = TestLoader {
            enabled: vec![plugin_source(
                McpSourceScope::Plugin,
                "plugin@anthropic",
                Some("anthropic"),
                "plugin-server",
                "plugin-command",
            )],
            ..TestLoader::default()
        };

        let runtime = build_chat_runtime_with_loader(
            args(false),
            &snapshot,
            PathBuf::from("/tmp/project"),
            &loader,
        )
        .unwrap();

        assert_eq!(runtime.mcp_bootstrap.planned.len(), 1);
        assert_eq!(runtime.mcp_bootstrap.planned[0].server.name, "enterprise");
        assert_eq!(
            runtime.mcp_bootstrap.planned[0].server.source_scope,
            McpSourceScope::Enterprise
        );
    }

    #[test]
    fn allowlist_and_denylist_apply_command_and_url_rules() {
        let mut global = GlobalConfig::default();
        global
            .mcp_servers
            .insert("stdio-ok".into(), sample_server("npx"));
        global.mcp_servers.insert(
            "remote-ok".into(),
            McpServerConfig::Sse {
                url: "https://example.com/sse".into(),
                headers: Default::default(),
                headers_helper: None,
            },
        );
        global.mcp_servers.insert(
            "remote-blocked".into(),
            McpServerConfig::Sse {
                url: "https://blocked.example.com/sse".into(),
                headers: Default::default(),
                headers_helper: None,
            },
        );

        let mut snapshot = snapshot(
            ProjectConfig {
                enable_all_project_mcp_servers: true,
                ..ProjectConfig::default()
            },
            global,
        );
        snapshot.managed_settings.merged_settings = serde_json::json!({
            "allowedMcpServers": [
                { "serverCommand": ["npx"] },
                { "serverUrl": "https://example.com/*" }
            ],
            "deniedMcpServers": [
                { "serverUrl": "https://blocked.example.com/*" }
            ]
        });

        let runtime = build_chat_runtime_with_loader(
            args(false),
            &snapshot,
            PathBuf::from("/tmp/project"),
            &TestLoader::default(),
        )
        .unwrap();

        let planned: Vec<_> = runtime
            .mcp_bootstrap
            .planned
            .iter()
            .map(|server| server.server.name.as_str())
            .collect();
        let blocked: Vec<_> = runtime
            .mcp_bootstrap
            .blocked
            .iter()
            .map(|server| (server.server.name.as_str(), server.decision.kind.clone()))
            .collect::<Vec<_>>();

        assert!(planned.contains(&"stdio-ok"));
        assert!(planned.contains(&"remote-ok"));
        assert!(blocked.iter().any(|(name, kind)| {
            *name == "remote-blocked" && *kind == McpPolicyDecisionKind::BlockedByDenylist
        }));
    }
}
