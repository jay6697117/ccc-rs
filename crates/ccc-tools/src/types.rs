//! Shared types for all tools.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Context passed to every tool invocation.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current working directory.
    pub cwd: std::path::PathBuf,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }
}

/// The result of a tool call — mirrors Anthropic `tool_result` content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolOutput {
    Text(String),
    Error(String),
}

impl ToolOutput {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
    pub fn error(s: impl Into<String>) -> Self {
        Self::Error(s.into())
    }
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text(s) | Self::Error(s) => s,
        }
    }
}

/// Tool execution error.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}
