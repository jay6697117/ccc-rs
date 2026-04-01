//! FileReadTool — reads file content with line numbers and optional range.
//! Corresponds to TS `src/tools/FileReadTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct FileReadTool;

#[derive(Deserialize)]
struct Input {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for FileReadTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Read",
            description: "Reads a file from the local filesystem with line numbers.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from (default: 1)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of lines to read (default: read whole file)"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let path = ctx.cwd.join(&inp.file_path);
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::Io(e))?;

        let mut lines: Vec<String> = Vec::new();
        let start = inp.offset.unwrap_or(1).saturating_sub(1);
        let count = inp.limit.unwrap_or(usize::MAX);

        for (i, line) in content.lines().enumerate().skip(start).take(count) {
            lines.push(format!("{}\t{}", i + 1, line));
        }

        if lines.is_empty() {
            Ok(ToolOutput::text("(empty file or range)"))
        } else {
            Ok(ToolOutput::text(lines.join("\n")))
        }
    }
}
