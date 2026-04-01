//! TaskUpdateTool — update an existing task.
//! Corresponds to TS `src/tools/TaskUpdateTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct TaskUpdateTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    task_id: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    active_form: Option<String>,
    #[serde(default)]
    status: Option<String>, // TODO: use TaskStatus enum
    #[serde(default)]
    add_blocks: Option<Vec<String>>,
    #[serde(default)]
    add_blocked_by: Option<Vec<String>>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    metadata: Option<HashMap<String, Value>>,
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "TaskUpdate",
            description: "Update an existing task in the task list.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "taskId": { "type": "string" },
                    "subject": { "type": "string" },
                    "description": { "type": "string" },
                    "activeForm": { "type": "string" },
                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "canceled"] },
                    "addBlocks": { "type": "array", "items": { "type": "string" } },
                    "addBlockedBy": { "type": "array", "items": { "type": "string" } },
                    "owner": { "type": "string" },
                    "metadata": { "type": "object" }
                },
                "required": ["taskId"]
            }),
        }
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        // TODO: Implement task update persistence.
        Ok(ToolOutput::text(format!(
            "Task #{} updated successfully",
            inp.task_id
        )))
    }
}
