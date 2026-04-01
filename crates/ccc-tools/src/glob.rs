//! GlobTool — fast file pattern matching.
//! Corresponds to TS `src/tools/GlobTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use walkdir::WalkDir;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct GlobTool;

#[derive(Deserialize)]
struct Input {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

#[async_trait]
impl Tool for GlobTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Glob",
            description: "Fast file pattern matching tool that works with any codebase size. \
                Supports glob patterns like \"*\", \"**/*\", etc.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The glob pattern to match against (e.g. \"**/*.js\")"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional directory to search in (defaults to current directory)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let root = match &inp.path {
            Some(p) => ctx.cwd.join(p),
            None => ctx.cwd.clone(),
        };

        let matcher = globset::GlobBuilder::new(&inp.pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?
            .compile_matcher();

        let mut results = Vec::new();
        for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&root).unwrap_or(entry.path());
                if matcher.is_match(rel_path) {
                    results.push(rel_path.to_string_lossy().into_owned());
                }
            }
        }

        if results.is_empty() {
            Ok(ToolOutput::text("No files found"))
        } else {
            Ok(ToolOutput::text(results.join("\n")))
        }
    }
}
