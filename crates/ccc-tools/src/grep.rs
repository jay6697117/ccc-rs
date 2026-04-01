//! GrepTool — fast string search.
//! Corresponds to TS `src/tools/GrepTool/`.

use async_trait::async_trait;
use grep_printer::StandardBuilder;
use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Cursor;
use termcolor::NoColor;
use walkdir::WalkDir;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct GrepTool;

#[derive(Deserialize)]
struct Input {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "Glob filtering is part of the public input schema but not implemented yet."
    )]
    glob: Option<String>,
}

#[async_trait]
impl Tool for GrepTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Grep",
            description: "Fast string search tool. Searches for content in files.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional directory to search in (defaults to current directory)"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Optional glob pattern to restrict search (e.g. \"*.rs\")"
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

        let matcher =
            RegexMatcher::new(&inp.pattern).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let mut printer_buf = Vec::new();
        let mut printer = StandardBuilder::new()
            .color_specs(grep_printer::ColorSpecs::default())
            .build(NoColor::new(Cursor::new(&mut printer_buf)));

        let mut searcher = Searcher::new();

        for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                // Skip binary or other junk if needed (optional for basic impl)
                let _ = searcher.search_path(
                    &matcher,
                    entry.path(),
                    printer.sink_with_path(&matcher, entry.path()),
                );
            }
        }

        let output = String::from_utf8_lossy(&printer_buf).into_owned();
        if output.is_empty() {
            Ok(ToolOutput::text("No matches found"))
        } else {
            Ok(ToolOutput::text(output))
        }
    }
}
