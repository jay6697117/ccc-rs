//! TaskGetTool — retrieve a task by its ID.
//! Corresponds to TS `src/tools/TaskGetTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct TaskGetTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    task_id: String,
}

#[async_trait]
impl Tool for TaskGetTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "TaskGet",
            description: "Retrieve a task by its ID from the task list.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "taskId": {
                        "type": "string",
                        "description": "The ID of the task to retrieve"
                    }
                },
                "required": ["taskId"]
            }),
        }
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        // TODO: Implement task retrieval from persistence.
        Ok(ToolOutput::error(format!(
            "Task #{} not found",
            inp.task_id
        )))
    }
}
