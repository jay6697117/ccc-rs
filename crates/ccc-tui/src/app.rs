use anyhow::Result;
use ccc_agent::SessionRunner;
use ccc_core::types::Message;
use ccc_vim::VimState;
use std::sync::Arc;
use tokio::sync::Mutex;

/// UI focus areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Conversation,
    Input,
    TaskList,
}

/// TUI Application state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub model: String,
    pub system_prompt: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-6".into(),
            system_prompt: None,
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
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let runner = SessionRunner::new(config.model, config.system_prompt)?;

        Ok(Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            input: String::new(),
            cursor_pos: 0,
            focus: Focus::Input,
            vim: VimState::default(),
            vim_persistent: ccc_vim::types::PersistentState::default(),
            should_quit: false,
            runner: Arc::new(Mutex::new(runner)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_uses_custom_model() {
        let app = App::new(AppConfig {
            model: "claude-test-model".into(),
            system_prompt: None,
        })
        .unwrap();

        assert!(app.messages.try_lock().unwrap().is_empty());
    }
}
