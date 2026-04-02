use anyhow::Result;
use ccc_agent::{
    session_store::{PersistedSession, SessionStore},
    SessionRunner,
};
use ccc_core::{McpBootstrapPlan, McpConnectionSnapshot, types::Message, SessionId};
use ccc_vim::VimState;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

/// UI focus areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Conversation,
    Input,
    TaskList,
}

/// TUI Application state.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub model: String,
    pub system_prompt: Option<String>,
    pub initial_messages: Vec<Message>,
    pub session_id: Option<SessionId>,
    pub cwd: String,
    pub mcp_bootstrap: McpBootstrapPlan,
    pub session_store: Option<SessionStore>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-6".into(),
            system_prompt: None,
            initial_messages: Vec::new(),
            session_id: None,
            cwd: std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .into_owned(),
            mcp_bootstrap: McpBootstrapPlan::default(),
            session_store: None,
        }
    }
}

pub struct App {
    pub messages: Arc<Mutex<Vec<Message>>>,
    pub input: String,
    pub cursor_pos: usize,
    pub focus: Focus,
    pub vim: VimState,
    pub vim_persistent: ccc_vim::types::PersistentState,
    pub should_quit: bool,
    pub runner: Arc<Mutex<SessionRunner>>,
    pub mcp_connections: Arc<Mutex<Vec<McpConnectionSnapshot>>>,
    pub session_store: Option<SessionStore>,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let session = match config.session_id {
            Some(session_id) => PersistedSession::new(
                session_id,
                config.cwd.clone(),
                config.model.clone(),
                config.system_prompt.clone(),
                config.initial_messages.clone(),
            ),
            None => PersistedSession::fresh(
                config.cwd.clone(),
                config.model.clone(),
                config.system_prompt.clone(),
                config.initial_messages.clone(),
            ),
        };
        let runner = SessionRunner::from_persisted_session(session)?;
        let messages = runner.messages().clone();

        Ok(Self {
            messages: Arc::new(Mutex::new(messages)),
            input: String::new(),
            cursor_pos: 0,
            focus: Focus::Input,
            vim: VimState::default(),
            vim_persistent: ccc_vim::types::PersistentState::default(),
            should_quit: false,
            runner: Arc::new(Mutex::new(runner)),
            mcp_connections: Arc::new(Mutex::new(Vec::new())),
            session_store: config.session_store,
        })
    }

    pub async fn bootstrap_mcp_plan(&self, plan: &McpBootstrapPlan) -> Result<()> {
        let report = {
            let mut runner = self.runner.lock().await;
            runner.bootstrap_mcp_plan(plan).await?
        };

        {
            let mut snapshots = self.mcp_connections.lock().await;
            *snapshots = report.snapshots.clone();
        }

        for warning in report.warnings {
            warn!(warning = %warning, "MCP bootstrap warning");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccc_agent::latest_assistant_text;
    use ccc_core::{ContentBlock, Role, SessionId};

    #[test]
    fn app_uses_custom_model() {
        let app = App::new(AppConfig {
            model: "claude-test-model".into(),
            system_prompt: None,
            initial_messages: vec![],
            session_id: None,
            cwd: "/tmp/project".into(),
            mcp_bootstrap: McpBootstrapPlan::default(),
            session_store: None,
        })
        .unwrap();

        assert!(app.messages.try_lock().unwrap().is_empty());
    }

    #[test]
    fn app_restores_initial_messages_and_session_id() {
        let app = App::new(AppConfig {
            model: "claude-test-model".into(),
            system_prompt: Some("system".into()),
            initial_messages: vec![Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "restored".into(),
                }],
            }],
            session_id: Some(SessionId::new("sess-1")),
            cwd: "/tmp/project".into(),
            mcp_bootstrap: McpBootstrapPlan::default(),
            session_store: None,
        })
        .unwrap();

        assert_eq!(
            latest_assistant_text(&app.messages.try_lock().unwrap()),
            "restored"
        );
        assert_eq!(
            app.runner
                .try_lock()
                .unwrap()
                .session_id()
                .map(|id| id.as_str()),
            Some("sess-1")
        );
    }
}
