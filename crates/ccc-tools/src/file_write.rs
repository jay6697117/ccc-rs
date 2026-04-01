//! FileWriteTool — creates or overwrites a file.
//! Corresponds to TS `src/tools/FileWriteTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct FileWriteTool;

#[derive(Deserialize)]
struct Input {
    file_path: String,
    content: String,
}

#[async_trait]
impl Tool for FileWriteTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Write",
            description: "Writes a file to the local filesystem. Overwrites existing file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to create"
                    },
                    "content": {
                        "type": "string",
                        "description": "The file content to write"
                    }
                },
                "required": ["file_path", "content"]
            }),
        }
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let path = ctx.cwd.join(&inp.file_path);

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::Io(e))?;
        }

        fs::write(&path, &inp.content)
            .await
            .map_err(|e| ToolError::Io(e))?;

        Ok(ToolOutput::text(format!(
            "The file {} has been written successfully.",
            inp.file_path
        )))
    }
}
