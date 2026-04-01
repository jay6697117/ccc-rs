use crate::session::Session;
use anyhow::Result;
use ccc_api::types::{MessagesRequest, RequestMessage};
use ccc_api::AnthropicClient;
use ccc_core::types::{ContentBlock, Message, Role};
use ccc_tools::types::ToolContext;
use ccc_tools::ToolRegistry;
use serde_json::json;

/// The main agent loop engine.
pub struct LoopEngine {
    client: AnthropicClient,
    tools: ToolRegistry,
}

impl LoopEngine {
    pub fn new(client: AnthropicClient, tools: ToolRegistry) -> Self {
        Self { client, tools }
    }

    pub async fn run(&mut self, session: &mut Session) -> Result<()> {
        let ctx = ToolContext::default();

        loop {
            let tool_defs: Vec<serde_json::Value> = self
                .tools
                .list_tools()
                .iter()
                .map(|t| {
                    let meta = t.meta();
                    json!({
                        "name": meta.name,
                        "description": meta.description,
                        "input_schema": meta.input_schema
                    })
                })
                .collect();

            let messages: Vec<RequestMessage> = session
                .history
                .iter()
                .map(|m| RequestMessage {
                    role: match m.role {
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                    },
                    content: serde_json::to_value(&m.content).unwrap(),
                })
                .collect();

            let req = MessagesRequest {
                model: "claude-opus-4-6".to_string(),
                max_tokens: 4096,
                messages,
                system: Some(json!(session.system_prompt)),
                tools: Some(tool_defs),
                stream: None,
                temperature: None,
                thinking: None,
                metadata: None,
                betas: None,
            };

            let resp = self.client.messages(req).await?;

            // Parse assistant message and tool uses
            let mut assistant_content = Vec::new();
            let mut tool_uses = Vec::new();

            for block in resp.content {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    assistant_content.push(ContentBlock::Text {
                        text: text.to_string(),
                    });
                } else if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                    let id = block["id"].as_str().unwrap().to_string();
                    let name = block["name"].as_str().unwrap().to_string();
                    let input = block["input"].clone();

                    assistant_content.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });

                    tool_uses.push((id, name, input));
                } else if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                    let signature = block
                        .get("signature")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    assistant_content.push(ContentBlock::Thinking {
                        thinking: thinking.to_string(),
                        signature,
                    });
                }
            }

            session.add_message(Message {
                role: Role::Assistant,
                content: assistant_content,
            });

            if tool_uses.is_empty() {
                break;
            }

            // Execute tools and add results
            let mut tool_results = Vec::new();
            for (id, name, input) in tool_uses {
                if let Some(tool) = self.tools.get_tool(&name) {
                    let result = tool.call(input, &ctx).await?;
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: vec![ContentBlock::Text {
                            text: result.as_str().to_string(),
                        }],
                        is_error: Some(result.is_error()),
                    });
                } else {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: vec![ContentBlock::Text {
                            text: format!("Tool '{}' not found", name),
                        }],
                        is_error: Some(true),
                    });
                }
            }

            session.add_message(Message {
                role: Role::User,
                content: tool_results,
            });

            if resp.stop_reason == Some("end_turn".to_string()) {
                break;
            }
        }
        Ok(())
    }
}
