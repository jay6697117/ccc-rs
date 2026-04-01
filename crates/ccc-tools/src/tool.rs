//! The `Tool` trait — every tool implements this.

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{ToolContext, ToolError, ToolOutput};

/// Metadata describing a tool for the Anthropic API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolMeta {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the input (matches Anthropic `input_schema`).
    pub input_schema: Value,
}

/// Core tool trait.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Static metadata (name, description, schema).
    fn meta(&self) -> ToolMeta;

    /// Execute the tool with a JSON-decoded input value.
    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}
