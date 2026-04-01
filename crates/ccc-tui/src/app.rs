use ccc_agent::Agent;
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
pub struct App {
    pub messages: Arc<Mutex<Vec<Message>>>,
    pub input: String,
    pub cursor_pos: usize,
    pub focus: Focus,
    pub vim: VimState,
    pub vim_persistent: ccc_vim::types::PersistentState,
    pub should_quit: bool,
    pub agent: Arc<Mutex<Agent>>,
}

impl App {
    pub fn new() -> Self {
        let agent = Agent::new("claude-opus-4-6").expect("Failed to create agent");

        // TODO: Load from ~/.claude/ccc-rs.toml
        // For now, let's try to add a default MCP if it exists or just leave it empty

        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            input: String::new(),
            cursor_pos: 0,
            focus: Focus::Input,
            vim: VimState::default(),
            vim_persistent: ccc_vim::types::PersistentState::default(),
            should_quit: false,
            agent: Arc::new(Mutex::new(agent)),
        }
    }
}
