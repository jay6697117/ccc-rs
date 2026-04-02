pub mod mcp_registry;
pub mod runner;
pub mod session_store;

use anyhow::Result;
use ccc_api::types::{MessagesRequest, RequestMessage, StreamEvent};
use ccc_api::AnthropicClient;
use ccc_core::{McpBootstrapPlan, McpConnectionSnapshot, types::{ContentBlock, Message, Role}};
use ccc_mcp::{client::McpClient, connector::connect_server};
use ccc_tools::types::ToolContext;
use ccc_tools::ToolRegistry;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;

pub use mcp_registry::{McpBootstrapReport, McpConnectionRegistry};
pub use runner::{latest_assistant_text, RunSummary, SessionRunner};

/// Core agent for managing conversations and model interaction.
pub struct Agent {
    client: AnthropicClient,
    model: String,
    messages: Vec<Message>,
    system_prompt: Option<String>,
    registry: Arc<ToolRegistry>,
    mcp_clients: Arc<Mutex<std::collections::HashMap<String, McpClient>>>,
    mcp_connections: Arc<Mutex<McpConnectionRegistry>>,
}

impl Agent {
    pub fn new(model: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: AnthropicClient::from_env()?,
            model: model.into(),
            messages: Vec::new(),
            system_prompt: None,
            registry: Arc::new(ToolRegistry::new()),
            mcp_clients: Arc::new(Mutex::new(std::collections::HashMap::new())),
            mcp_connections: Arc::new(Mutex::new(McpConnectionRegistry::default())),
        })
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub async fn add_mcp_server(
        &mut self,
        name: &str,
        config: &ccc_core::config::McpServerConfig,
    ) -> Result<()> {
        let (command, args, env) = config.stdio_parts().ok_or_else(|| {
            anyhow::anyhow!(
                "transport {:?} is not supported yet",
                config.transport_kind()
            )
        })?;
        let mut client = McpClient::spawn(command, args, env).await?;
        client.initialize().await?;
        self.mcp_clients
            .lock()
            .await
            .insert(name.to_string(), client);
        self.mcp_connections.lock().await.upsert(McpConnectionSnapshot {
            name: name.into(),
            transport: config.transport_kind(),
            status: ccc_core::McpConnectionStatus::Connected,
            reconnect_attempt: None,
            max_reconnect_attempts: None,
            error: None,
            source_scope: ccc_core::McpSourceScope::Dynamic,
        });
        Ok(())
    }

    pub async fn bootstrap_mcp_servers(
        &mut self,
        servers: &[(String, ccc_core::config::McpServerConfig)],
    ) -> Result<Vec<(String, anyhow::Error)>> {
        let mut failures = Vec::new();

        for (name, config) in servers {
            if let Err(err) = self.add_mcp_server(name, config).await {
                failures.push((name.clone(), err));
            }
        }

        Ok(failures)
    }

    pub async fn bootstrap_mcp_plan(&mut self, plan: &McpBootstrapPlan) -> Result<McpBootstrapReport> {
        *self.mcp_connections.lock().await = McpConnectionRegistry::from_plan(plan);
        self.mcp_clients.lock().await.clear();

        for planned in &plan.planned {
            let result = connect_server(
                &planned.server.name,
                planned.server.source_scope,
                &planned.server.config,
            )
            .await;
            if let Some(client) = result.client {
                self.mcp_clients
                    .lock()
                    .await
                    .insert(planned.server.name.clone(), client);
            }
            self.mcp_connections.lock().await.upsert(result.snapshot);
        }

        Ok(self.mcp_connections.lock().await.bootstrap_report(plan))
    }

    pub async fn mcp_connection_snapshots(&self) -> Vec<McpConnectionSnapshot> {
        self.mcp_connections.lock().await.snapshots()
    }

    pub fn get_messages(&self) -> &Vec<Message> {
        &self.messages
    }

    pub async fn chat_stream(&self) -> Result<ccc_api::EventStream> {
        let request_messages: Vec<RequestMessage> = self
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                let content = match serde_json::to_value(&m.content) {
                    Ok(v) => v,
                    Err(_) => serde_json::Value::Array(vec![]),
                };
                RequestMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect();

        let tools: Vec<serde_json::Value> = self
            .registry
            .list_tools()
            .iter()
            .map(|t| {
                let meta = t.meta();
                serde_json::json!({
                    "name": meta.name,
                    "description": meta.description,
                    "input_schema": meta.input_schema,
                })
            })
            .collect();

        // Add MCP tools
        let _clients = self.mcp_clients.lock().await;
        // Note: In a real implementation, we would call list_tools() and cache them.
        // For now, we assume tools are added to the registry or handled dynamically.

        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: request_messages,
            system: self
                .system_prompt
                .as_ref()
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
            stream: Some(true),
            tools: Some(tools),
            temperature: None,
            thinking: None,
            metadata: None,
            betas: None,
        };

        self.client.stream(request, &[]).await.map_err(Into::into)
    }

    pub async fn handle_tool_call(
        &self,
        tool_use_id: String,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ContentBlock> {
        // Try internal tools first
        if let Some(tool) = self.registry.get_tool(name) {
            let ctx = ToolContext {
                cwd: std::env::current_dir()?,
            };

            let output = tool.call(input, &ctx).await?;

            return Ok(ContentBlock::ToolResult {
                tool_use_id,
                content: vec![ContentBlock::Text {
                    text: output.as_str().to_string(),
                }],
                is_error: Some(output.is_error()),
            });
        }

        // Try MCP tools
        let mut clients = self.mcp_clients.lock().await;
        for (_server_name, client) in clients.iter_mut() {
            if let Ok(result) = client.call_tool(name, input.clone()).await {
                return Ok(ContentBlock::ToolResult {
                    tool_use_id,
                    content: vec![ContentBlock::Text {
                        text: result.to_string(),
                    }],
                    is_error: Some(false),
                });
            }
        }

        anyhow::bail!("Tool not found: {}", name)
    }

    /// Main loop to process a request and its subsequent tool-call iterations.
    pub async fn run<F>(&mut self, user_input: String, on_event: F) -> Result<()>
    where
        F: FnMut(StreamEvent),
    {
        self.add_message(Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: user_input }],
        });

        self.process_loop(on_event).await
    }

    async fn process_loop<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(StreamEvent),
    {
        loop {
            let mut stream = self.chat_stream().await?;
            let mut current_response_blocks = Vec::new();
            let mut tool_calls = Vec::new();

            let mut partial_tool_inputs: std::collections::HashMap<u32, String> =
                std::collections::HashMap::new();

            while let Some(event) = stream.next().await {
                let event = event?;
                on_event(event.clone());

                match event {
                    StreamEvent::ContentBlockStart {
                        index,
                        content_block,
                    } => match content_block {
                        ccc_api::types::ContentBlockStart::Text { text } => {
                            current_response_blocks.push(ContentBlock::Text { text });
                        }
                        ccc_api::types::ContentBlockStart::Thinking { thinking } => {
                            current_response_blocks.push(ContentBlock::Thinking {
                                thinking,
                                signature: String::new(),
                            });
                        }
                        ccc_api::types::ContentBlockStart::ToolUse { id, name } => {
                            tool_calls.push((id, name, index));
                            partial_tool_inputs.insert(index, String::new());
                        }
                    },
                    StreamEvent::ContentBlockDelta { index, delta } => {
                        let idx = index as usize;
                        match delta {
                            ccc_api::types::Delta::TextDelta { text: delta_text } => {
                                if let Some(ContentBlock::Text { text }) =
                                    current_response_blocks.get_mut(idx)
                                {
                                    text.push_str(&delta_text);
                                }
                            }
                            ccc_api::types::Delta::ThinkingDelta {
                                thinking: delta_thinking,
                            } => {
                                if let Some(ContentBlock::Thinking { thinking, .. }) =
                                    current_response_blocks.get_mut(idx)
                                {
                                    thinking.push_str(&delta_thinking);
                                }
                            }
                            ccc_api::types::Delta::InputJsonDelta { partial_json } => {
                                if let Some(input) = partial_tool_inputs.get_mut(&index) {
                                    input.push_str(&partial_json);
                                }
                            }
                            _ => {}
                        }
                    }
                    StreamEvent::MessageStop => break,
                    _ => {}
                }
            }

            // Add assistant's response to history
            self.add_message(Message {
                role: Role::Assistant,
                content: current_response_blocks,
            });

            // Execute tool calls if any
            if tool_calls.is_empty() {
                break; // Final response reached
            }

            let mut tool_results = Vec::new();
            for (id, name, index) in tool_calls {
                let input_str = partial_tool_inputs.remove(&index).unwrap_or_default();
                let input: serde_json::Value = serde_json::from_str(&input_str)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                let result = self.handle_tool_call(id, &name, input).await?;
                tool_results.push(result);
            }

            // Add tool results to history for next iteration
            self.add_message(Message {
                role: Role::User, // Tool results are sent back as a 'user' (system) turn in the loop
                content: tool_results,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bootstrap_mcp_servers_skips_empty_input() {
        let mut agent = Agent::new("claude-opus-4-6").unwrap();
        let failures = agent.bootstrap_mcp_servers(&[]).await.unwrap();

        assert!(failures.is_empty());
    }

    #[tokio::test]
    async fn bootstrap_mcp_plan_tracks_disabled_servers_in_registry() {
        let mut agent = Agent::new("claude-opus-4-6").unwrap();
        let report = agent
            .bootstrap_mcp_plan(&McpBootstrapPlan {
                planned: Vec::new(),
                blocked: vec![ccc_core::BlockedMcpServer {
                    server: ccc_core::ResolvedMcpServer {
                        name: "blocked".into(),
                        config: ccc_core::config::McpServerConfig::Stdio {
                            command: "echo".into(),
                            args: Vec::new(),
                            env: std::collections::HashMap::new(),
                        },
                        source_scope: ccc_core::McpSourceScope::Global,
                        source_label: "/tmp/settings.json".into(),
                        plugin_source: None,
                        dedup_signature: None,
                        default_enabled: true,
                    },
                    decision: ccc_core::McpPolicyDecision {
                        name: "blocked".into(),
                        kind: ccc_core::McpPolicyDecisionKind::BlockedByAllowlist,
                        message: "blocked by policy".into(),
                    },
                }],
                warnings: Vec::new(),
            })
            .await
            .unwrap();

        assert!(report
            .snapshots
            .iter()
            .any(|snapshot| snapshot.status == ccc_core::McpConnectionStatus::Disabled));
    }
}
