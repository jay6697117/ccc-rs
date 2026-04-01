//! TaskListTool — list all tasks in the task list.
//! Corresponds to TS `src/tools/TaskListTool/`.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct TaskListTool;

#[async_trait]
impl Tool for TaskListTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "TaskList",
            description: "List all tasks in the task list.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        // TODO: Implement task listing from persistence.
        Ok(ToolOutput::text("No tasks found."))
    }
}
