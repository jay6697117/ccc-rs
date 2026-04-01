//! BashTool — execute shell commands via `bash -c`.
//! Corresponds to TS `src/tools/BashTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

/// Default timeout: 2 minutes (matches TS default).
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
/// Hard cap: 10 minutes.
const MAX_TIMEOUT_MS: u64 = 600_000;
/// Max output characters returned to the model (~100 KB).
const MAX_OUTPUT_CHARS: usize = 100_000;

pub struct BashTool;

#[derive(Deserialize)]
struct Input {
    command: String,
    #[serde(default)]
    timeout: Option<u64>, // milliseconds
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "Descriptions are kept for UI display but the current CLI path does not surface them."
    )]
    description: Option<String>,
}

#[async_trait]
impl Tool for BashTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "Bash",
            description: "Executes a bash command and returns combined stdout+stderr. \
                Commands run in the current working directory. \
                Timeout defaults to 120 000 ms (2 min), hard-capped at 600 000 ms (10 min).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Optional timeout in milliseconds (max 600000)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional human-readable description shown in the UI"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        if inp.command.trim().is_empty() {
            return Err(ToolError::InvalidInput("command must not be empty".into()));
        }

        let timeout_ms = inp
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let fut = Command::new("bash")
            .arg("-c")
            .arg(&inp.command)
            .current_dir(&ctx.cwd)
            .output();

        let result = timeout(Duration::from_millis(timeout_ms), fut)
            .await
            .map_err(|_| ToolError::Other(format!("Command timed out after {timeout_ms}ms")))?
            .map_err(|e| ToolError::Io(e))?;

        let mut output = String::new();
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        if !stdout.is_empty() {
            output.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&stderr);
        }

        // Truncate if too long
        if output.len() > MAX_OUTPUT_CHARS {
            output.truncate(MAX_OUTPUT_CHARS);
            output.push_str("\n[output truncated]");
        }

        if output.is_empty() {
            output = "(no output)".to_string();
        }

        if result.status.success() {
            Ok(ToolOutput::text(output))
        } else {
            let code = result.status.code().unwrap_or(-1);
            Ok(ToolOutput::error(format!("Exit {code}\n{output}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolContext {
        ToolContext::default()
    }

    #[tokio::test]
    async fn echo_works() {
        let tool = BashTool;
        let out = tool
            .call(json!({"command": "echo hello"}), &ctx())
            .await
            .unwrap();
        assert!(!out.is_error());
        assert!(out.as_str().contains("hello"));
    }

    #[tokio::test]
    async fn non_zero_exit_is_error_output() {
        let tool = BashTool;
        let out = tool
            .call(json!({"command": "exit 1"}), &ctx())
            .await
            .unwrap();
        assert!(out.is_error());
    }

    #[tokio::test]
    async fn captures_stderr() {
        let tool = BashTool;
        let out = tool
            .call(json!({"command": "echo err >&2"}), &ctx())
            .await
            .unwrap();
        assert!(out.as_str().contains("err"));
    }

    #[tokio::test]
    async fn timeout_is_respected() {
        let tool = BashTool;
        let out = tool
            .call(json!({"command": "sleep 10", "timeout": 100}), &ctx())
            .await;
        assert!(out.is_err()); // ToolError::Other (timed out)
    }

    #[tokio::test]
    async fn empty_command_is_error() {
        let tool = BashTool;
        let out = tool.call(json!({"command": "   "}), &ctx()).await;
        assert!(out.is_err());
    }
}
