use anyhow::Result;
use ccc_api::types::StreamEvent;
use ccc_core::types::{ContentBlock, Message, Role};

use crate::Agent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub assistant_text: String,
}

pub struct SessionRunner {
    agent: Agent,
}

impl SessionRunner {
    pub fn new(model: impl Into<String>, system_prompt: Option<String>) -> Result<Self> {
        let agent = match system_prompt {
            Some(prompt) => Agent::new(model)?.with_system_prompt(prompt),
            None => Agent::new(model)?,
        };

        Ok(Self { agent })
    }

    pub fn messages(&self) -> &Vec<Message> {
        self.agent.get_messages()
    }

    pub async fn run_with_events<F>(
        &mut self,
        user_input: String,
        on_event: F,
    ) -> Result<RunSummary>
    where
        F: FnMut(StreamEvent) + Send + Sync + 'static,
    {
        self.agent.run(user_input, on_event).await?;

        Ok(RunSummary {
            assistant_text: latest_assistant_text(self.agent.get_messages()),
        })
    }
}

pub fn latest_assistant_text(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == Role::Assistant)
        .map(|message| {
            message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_assistant_text_ignores_non_text_blocks() {
        let messages = vec![Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "hidden".into(),
                    signature: "sig".into(),
                },
                ContentBlock::Text {
                    text: "hello".into(),
                },
                ContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::Text {
                    text: " world".into(),
                },
            ],
        }];

        assert_eq!(latest_assistant_text(&messages), "hello world");
    }

    #[test]
    fn latest_assistant_text_uses_last_assistant_message() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text { text: "old".into() }],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "question".into(),
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text { text: "new".into() }],
            },
        ];

        assert_eq!(latest_assistant_text(&messages), "new");
    }
}
