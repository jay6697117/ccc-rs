//! AgentTool — launch a subagent.
//! Corresponds to TS `src/tools/AgentTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct AgentTool;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[expect(
    dead_code,
    reason = "Agent tool input defines the accepted schema before spawning is implemented."
)]
struct Input {
    prompt: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    run_in_background: bool,
    #[serde(default)]
    isolation: Option<String>,
}

#[async_trait]
impl Tool for AgentTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Agent",
            description: "Launch a new agent to handle complex, multi-step tasks autonomously.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The objective or task for the agent to complete"
                    },
                    "description": {
                        "type": "string",
                        "description": "A brief summary of what this agent is doing"
                    },
                    "subagent_type": {
                        "type": "string",
                        "description": "Optional specialized agent type (e.g. 'Explore', 'Plan')"
                    },
                    "model": {
                        "type": "string",
                        "enum": ["sonnet", "opus", "haiku"],
                        "description": "The Claude model to use for this agent"
                    },
                    "run_in_background": {
                        "type": "boolean",
                        "description": "Whether to run the agent in the background"
                    },
                    "isolation": {
                        "type": "string",
                        "enum": ["worktree"],
                        "description": "Optional isolation mode"
                    }
                },
                "required": ["prompt", "description"]
            }),
        }
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let _inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        // TODO: Implement agent spawning (interfacing with ccc-agent crate).
        Ok(ToolOutput::text("Agent launched (simulation)"))
    }
}
