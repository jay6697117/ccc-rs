pub mod agent;
pub mod ask_user_question;
pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob;
pub mod grep;
pub mod task_create;
pub mod task_get;
pub mod task_list;
pub mod task_update;
pub mod tool;
pub mod types;

use crate::tool::Tool;
use std::sync::Arc;

pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: vec![
                Arc::new(bash::BashTool),
                Arc::new(file_read::FileReadTool),
                Arc::new(file_write::FileWriteTool),
                Arc::new(file_edit::FileEditTool),
                Arc::new(glob::GlobTool),
                Arc::new(grep::GrepTool),
                Arc::new(task_create::TaskCreateTool),
                Arc::new(task_update::TaskUpdateTool),
                Arc::new(task_list::TaskListTool),
                Arc::new(task_get::TaskGetTool),
                Arc::new(ask_user_question::AskUserQuestionTool),
                Arc::new(agent::AgentTool),
            ],
        }
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.meta().name == name).cloned()
    }

    pub fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }
}
