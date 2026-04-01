//! MCP Client implementation.

use crate::types::*;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

pub struct McpClient {
    _child: Child,
    writer: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
}

impl McpClient {
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<Self> {
        use std::process::Stdio;
        let mut cmd = Command::new(command);
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

        let writer = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());

        Ok(Self {
            _child: child,
            writer,
            reader,
        })
    }

    pub async fn initialize(&mut self) -> Result<()> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "ccc-rs",
                "version": "0.1.0"
            }
        });
        self.send_request("initialize", params).await?;
        self.send_notification("notifications/initialized", json!({}))
            .await?;
        Ok(())
    }

    pub async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let json = serde_json::to_string(&req)? + "\n";
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Value> {
        self.send_request("tools/list", json!({})).await
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let params = json!({
            "name": name,
            "arguments": arguments
        });
        self.send_request("tools/call", params).await
    }

    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(params),
            id: json!(1), // TODO: generate unique ID
        };

        let json = serde_json::to_string(&req)? + "\n";
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        let resp: McpResponse = serde_json::from_str(&line)?;

        if let Some(err) = resp.error {
            anyhow::bail!("MCP Error {}: {}", err.code, err.message);
        }

        Ok(resp.result.unwrap_or(Value::Null))
    }
}
