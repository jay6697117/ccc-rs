use std::collections::HashMap;

use ccc_core::{
    McpBootstrapPlan, McpConnectionSnapshot, McpConnectionStatus, ResolvedMcpServer,
};

#[derive(Debug, Clone, PartialEq)]
pub struct McpBootstrapReport {
    pub snapshots: Vec<McpConnectionSnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
pub struct McpConnectionRegistry {
    snapshots: HashMap<String, McpConnectionSnapshot>,
}

impl McpConnectionRegistry {
    pub fn from_plan(plan: &McpBootstrapPlan) -> Self {
        let mut registry = Self::default();

        for planned in &plan.planned {
            registry.snapshots.insert(
                planned.server.name.clone(),
                McpConnectionSnapshot {
                    name: planned.server.name.clone(),
                    transport: planned.server.config.transport_kind(),
                    status: McpConnectionStatus::Pending,
                    reconnect_attempt: None,
                    max_reconnect_attempts: None,
                    error: None,
                    source_scope: planned.server.source_scope,
                },
            );
        }

        for blocked in &plan.blocked {
            registry.snapshots.insert(
                blocked.server.name.clone(),
                disabled_snapshot(&blocked.server, Some(blocked.decision.message.clone())),
            );
        }

        registry
    }

    pub fn upsert(&mut self, snapshot: McpConnectionSnapshot) {
        self.snapshots.insert(snapshot.name.clone(), snapshot);
    }

    pub fn snapshots(&self) -> Vec<McpConnectionSnapshot> {
        let mut snapshots = self.snapshots.values().cloned().collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.name.cmp(&right.name));
        snapshots
    }

    pub fn bootstrap_report(&self, plan: &McpBootstrapPlan) -> McpBootstrapReport {
        let mut warnings = plan.warnings.clone();
        for blocked in &plan.blocked {
            warnings.push(blocked.decision.message.clone());
        }
        for snapshot in self.snapshots.values() {
            if matches!(
                snapshot.status,
                McpConnectionStatus::Failed | McpConnectionStatus::NeedsAuth
            ) {
                warnings.push(match &snapshot.error {
                    Some(error) => format!("MCP server {}: {}", snapshot.name, error),
                    None => format!("MCP server {} is {}", snapshot.name, render_status(snapshot.status)),
                });
            }
        }

        McpBootstrapReport {
            snapshots: self.snapshots(),
            warnings,
        }
    }
}

fn disabled_snapshot(server: &ResolvedMcpServer, error: Option<String>) -> McpConnectionSnapshot {
    McpConnectionSnapshot {
        name: server.name.clone(),
        transport: server.config.transport_kind(),
        status: McpConnectionStatus::Disabled,
        reconnect_attempt: None,
        max_reconnect_attempts: None,
        error,
        source_scope: server.source_scope,
    }
}

fn render_status(status: McpConnectionStatus) -> &'static str {
    match status {
        McpConnectionStatus::Pending => "pending",
        McpConnectionStatus::Connected => "connected",
        McpConnectionStatus::Failed => "failed",
        McpConnectionStatus::NeedsAuth => "needs-auth",
        McpConnectionStatus::Disabled => "disabled",
    }
}

#[cfg(test)]
mod tests {
    use ccc_core::{
        BlockedMcpServer, McpPolicyDecision, McpPolicyDecisionKind, PlannedMcpServer,
        config::McpServerConfig, McpSourceScope,
    };

    use super::*;

    fn sample_server(name: &str, scope: McpSourceScope) -> ResolvedMcpServer {
        ResolvedMcpServer {
            name: name.into(),
            config: McpServerConfig::Stdio {
                command: "echo".into(),
                args: Vec::new(),
                env: HashMap::new(),
            },
            source_scope: scope,
            source_label: "/tmp/settings.json".into(),
            plugin_source: None,
            dedup_signature: None,
            default_enabled: true,
        }
    }

    #[test]
    fn registry_initializes_pending_and_disabled_snapshots_from_plan() {
        let plan = McpBootstrapPlan {
            planned: vec![PlannedMcpServer {
                server: sample_server("connected-later", McpSourceScope::Global),
                initial_status: McpConnectionStatus::Pending,
            }],
            blocked: vec![BlockedMcpServer {
                server: sample_server("disabled", McpSourceScope::Plugin),
                decision: McpPolicyDecision {
                    name: "disabled".into(),
                    kind: McpPolicyDecisionKind::BlockedByAllowlist,
                    message: "blocked".into(),
                },
            }],
            warnings: vec!["plan warning".into()],
        };

        let registry = McpConnectionRegistry::from_plan(&plan);
        let report = registry.bootstrap_report(&plan);

        assert_eq!(report.snapshots.len(), 2);
        assert!(report
            .snapshots
            .iter()
            .any(|snapshot| snapshot.status == McpConnectionStatus::Pending));
        assert!(report
            .snapshots
            .iter()
            .any(|snapshot| snapshot.status == McpConnectionStatus::Disabled));
        assert!(report.warnings.iter().any(|warning| warning == "plan warning"));
    }
}
