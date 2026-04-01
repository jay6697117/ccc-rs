//! FileEditTool — exact-string replacement in files.
//! Corresponds to TS `src/tools/FileEditTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct FileEditTool;

#[derive(Deserialize)]
struct Input {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[async_trait]
impl Tool for FileEditTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Edit",
            description: "Performs exact string replacements in a file. \
                The old_string must match exactly (including whitespace and newlines). \
                Use replace_all=true to replace every occurrence.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact string to find and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement string"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false, replaces only the first)"
                    }
                },
                "required": ["file_path", "old_string", "new_string"]
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

        let count = if inp.replace_all {
            content.matches(&inp.old_string).count()
        } else {
            if content.contains(&inp.old_string) {
                1
            } else {
                0
            }
        };

        if count == 0 {
            return Ok(ToolOutput::error(format!(
                "String not found in {}: {:?}",
                inp.file_path,
                // show first 80 chars of old_string for debugging
                inp.old_string.chars().take(80).collect::<String>()
            )));
        }

        let new_content = if inp.replace_all {
            content.replace(&inp.old_string, &inp.new_string)
        } else {
            content.replacen(&inp.old_string, &inp.new_string, 1)
        };

        fs::write(&path, new_content)
            .await
            .map_err(|e| ToolError::Io(e))?;

        Ok(ToolOutput::text(format!(
            "The file {} has been updated successfully.",
            inp.file_path
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    async fn run(
        file_path: &str,
        old: &str,
        new: &str,
        replace_all: bool,
        cwd: &std::path::Path,
    ) -> ToolOutput {
        let tool = FileEditTool;
        let input = json!({
            "file_path": file_path,
            "old_string": old,
            "new_string": new,
            "replace_all": replace_all,
        });
        let ctx = ToolContext {
            cwd: cwd.to_path_buf(),
        };
        tool.call(input, &ctx).await.unwrap()
    }

    #[tokio::test]
    async fn replaces_first_occurrence() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "foo foo foo").unwrap();
        let name = f.path().file_name().unwrap().to_str().unwrap().to_owned();
        let dir = f.path().parent().unwrap().to_path_buf();
        let out = run(&name, "foo", "bar", false, &dir).await;
        assert!(!out.is_error());
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(content, "bar foo foo");
    }

    #[tokio::test]
    async fn replaces_all_occurrences() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "foo foo foo").unwrap();
        let name = f.path().file_name().unwrap().to_str().unwrap().to_owned();
        let dir = f.path().parent().unwrap().to_path_buf();
        let out = run(&name, "foo", "bar", true, &dir).await;
        assert!(!out.is_error());
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(content, "bar bar bar");
    }

    #[tokio::test]
    async fn returns_error_when_not_found() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();
        let name = f.path().file_name().unwrap().to_str().unwrap().to_owned();
        let dir = f.path().parent().unwrap().to_path_buf();
        let out = run(&name, "nothere", "x", false, &dir).await;
        assert!(out.is_error());
    }
}
