//! TaskCreateTool — create a new task in the task list.
//! Corresponds to TS `src/tools/TaskCreateTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct TaskCreateTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    subject: String,
    description: String,
    #[serde(default)]
    active_form: Option<String>,
    #[serde(default)]
    metadata: Option<HashMap<String, Value>>,
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "TaskCreate",
            description: "Create a new task in the task list for tracking progress.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "subject": {
                        "type": "string",
                        "description": "A brief, actionable title in imperative form (e.g., \"Fix authentication bug in login flow\")"
                    },
                    "description": {
                        "type": "string",
                        "description": "What needs to be done"
                    },
                    "activeForm": {
                        "type": "string",
                        "description": "Optional present continuous form shown in the spinner when the task is in_progress (e.g., \"Fixing authentication bug\")"
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional arbitrary metadata to attach to the task"
                    }
                },
                "required": ["subject", "description"]
            }),
        }
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        // TODO: Implement task persistence (writing to disk/state).
        // For now, return success to simulate behavior.
        let task_id = "1"; // Mock ID for now.

        Ok(ToolOutput::text(format!(
            "Task #{} created successfully: {}",
            task_id, inp.subject
        )))
    }
}
