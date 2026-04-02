use std::io::Write;

use ccc_api::types::{StreamEvent, Usage};
use ccc_core::{McpConnectionSnapshot, Message, SessionId};
use serde::Serialize;
use uuid::Uuid;

use crate::cli::OutputFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultContext {
    pub session_id: SessionId,
    pub model: String,
    pub duration_ms: u64,
    pub num_turns: usize,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

impl ResultContext {
    pub fn failed(session_id: SessionId, model: impl Into<String>) -> Self {
        Self {
            session_id,
            model: model.into(),
            duration_ms: 0,
            num_turns: 0,
            stop_reason: None,
            usage: Usage::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResultEnvelope {
    #[serde(rename = "type")]
    kind: &'static str,
    subtype: &'static str,
    is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    session_id: SessionId,
    model: String,
    duration_ms: u64,
    num_turns: usize,
    stop_reason: Option<String>,
    usage: Usage,
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    errors: Vec<String>,
    uuid: String,
}

impl ResultEnvelope {
    pub fn success(context: ResultContext, result: String, warnings: Vec<String>) -> Self {
        Self {
            kind: "result",
            subtype: "success",
            is_error: false,
            result: Some(result),
            session_id: context.session_id,
            model: context.model,
            duration_ms: context.duration_ms,
            num_turns: context.num_turns,
            stop_reason: context.stop_reason,
            usage: context.usage,
            warnings,
            errors: Vec::new(),
            uuid: next_uuid(),
        }
    }

    pub fn error(context: ResultContext, warnings: Vec<String>, errors: Vec<String>) -> Self {
        Self {
            kind: "result",
            subtype: "error_during_execution",
            is_error: true,
            result: None,
            session_id: context.session_id,
            model: context.model,
            duration_ms: context.duration_ms,
            num_turns: context.num_turns,
            stop_reason: context.stop_reason,
            usage: context.usage,
            warnings,
            errors,
            uuid: next_uuid(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemInitEvent {
    #[serde(rename = "type")]
    kind: &'static str,
    subtype: &'static str,
    session_id: SessionId,
    cwd: String,
    model: String,
    output_format: OutputFormat,
    mcp_servers: Vec<McpConnectionSnapshot>,
    uuid: String,
}

impl SystemInitEvent {
    pub fn new(
        session_id: SessionId,
        cwd: impl Into<String>,
        model: impl Into<String>,
        output_format: OutputFormat,
        mcp_servers: Vec<McpConnectionSnapshot>,
    ) -> Self {
        Self {
            kind: "system",
            subtype: "init",
            session_id,
            cwd: cwd.into(),
            model: model.into(),
            output_format,
            mcp_servers,
            uuid: next_uuid(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemWarningEvent {
    #[serde(rename = "type")]
    kind: &'static str,
    subtype: &'static str,
    message: String,
    session_id: SessionId,
    uuid: String,
}

impl SystemWarningEvent {
    pub fn new(session_id: SessionId, message: impl Into<String>) -> Self {
        Self {
            kind: "system",
            subtype: "warning",
            message: message.into(),
            session_id,
            uuid: next_uuid(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamEventEnvelope {
    #[serde(rename = "type")]
    kind: &'static str,
    event: StreamEvent,
    session_id: SessionId,
    uuid: String,
}

impl StreamEventEnvelope {
    pub fn new(session_id: SessionId, event: StreamEvent) -> Self {
        Self {
            kind: "stream_event",
            event,
            session_id,
            uuid: next_uuid(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssistantEvent {
    #[serde(rename = "type")]
    kind: &'static str,
    message: Message,
    session_id: SessionId,
    uuid: String,
}

impl AssistantEvent {
    pub fn new(session_id: SessionId, message: Message) -> Self {
        Self {
            kind: "assistant",
            message,
            session_id,
            uuid: next_uuid(),
        }
    }
}

pub struct ProtocolWriter<'a, Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    format: OutputFormat,
    stdout: &'a mut Stdout,
    stderr: &'a mut Stderr,
}

impl<'a, Stdout, Stderr> ProtocolWriter<'a, Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    pub fn new(format: OutputFormat, stdout: &'a mut Stdout, stderr: &'a mut Stderr) -> Self {
        Self {
            format,
            stdout,
            stderr,
        }
    }

    pub fn emit_init(&mut self, event: &SystemInitEvent) -> std::io::Result<()> {
        if self.format == OutputFormat::StreamJson {
            self.write_json_line(event)?;
        }

        Ok(())
    }

    pub fn emit_warning(&mut self, session_id: &SessionId, message: &str) -> std::io::Result<()> {
        match self.format {
            OutputFormat::Text => self.write_stderr_line(message),
            OutputFormat::Json => Ok(()),
            OutputFormat::StreamJson => {
                self.write_json_line(&SystemWarningEvent::new(session_id.clone(), message))
            }
        }
    }

    pub fn emit_stream_event(
        &mut self,
        session_id: &SessionId,
        event: &StreamEvent,
    ) -> std::io::Result<()> {
        if self.format == OutputFormat::StreamJson {
            self.write_json_line(&StreamEventEnvelope::new(session_id.clone(), event.clone()))?;
        }

        Ok(())
    }

    pub fn emit_assistant(
        &mut self,
        session_id: &SessionId,
        message: &Message,
    ) -> std::io::Result<()> {
        if self.format == OutputFormat::StreamJson {
            self.write_json_line(&AssistantEvent::new(session_id.clone(), message.clone()))?;
        }

        Ok(())
    }

    pub fn emit_result(&mut self, result: &ResultEnvelope) -> std::io::Result<()> {
        match self.format {
            OutputFormat::Text => {
                if result.is_error {
                    self.write_stderr_line(&render_errors(result))
                } else if let Some(text) = &result.result {
                    self.write_stdout_text(text)
                } else {
                    Ok(())
                }
            }
            OutputFormat::Json | OutputFormat::StreamJson => self.write_json_line(result),
        }
    }

    fn write_json_line<T>(&mut self, value: &T) -> std::io::Result<()>
    where
        T: Serialize,
    {
        serde_json::to_writer(&mut self.stdout, value).map_err(std::io::Error::other)?;
        self.stdout.write_all(b"\n")
    }

    fn write_stdout_text(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        self.stdout.write_all(text.as_bytes())?;
        self.stdout.write_all(b"\n")
    }

    fn write_stderr_line(&mut self, message: &str) -> std::io::Result<()> {
        self.stderr.write_all(message.as_bytes())?;
        self.stderr.write_all(b"\n")
    }
}

fn render_errors(result: &ResultEnvelope) -> String {
    if result.errors.is_empty() {
        "unknown error".into()
    } else {
        result.errors.join("\n")
    }
}

fn next_uuid() -> String {
    Uuid::new_v4().to_string()
}
