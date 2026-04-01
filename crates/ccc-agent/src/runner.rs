use anyhow::Result;
use ccc_api::types::StreamEvent;
use ccc_core::{
    types::{ContentBlock, Message, Role},
    SessionId,
};

use crate::{session_store::PersistedSession, Agent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub assistant_text: String,
}

pub struct SessionRunner {
    agent: Agent,
    session_id: Option<SessionId>,
    cwd: String,
    model: String,
    system_prompt: Option<String>,
}

impl SessionRunner {
    pub fn new(model: impl Into<String>, system_prompt: Option<String>) -> Result<Self> {
        let model = model.into();
        let agent = match system_prompt.as_ref() {
            Some(prompt) => Agent::new(model.clone())?.with_system_prompt(prompt),
            None => Agent::new(model.clone())?,
        };

        Ok(Self {
            agent,
            session_id: None,
            cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
            model,
            system_prompt,
        })
    }

    pub fn from_persisted_session(session: PersistedSession) -> Result<Self> {
        let mut runner = Self::new(session.model.clone(), session.system_prompt.clone())?;
        runner.session_id = Some(session.session_id);
        runner.cwd = session.cwd;
        runner.model = session.model;
        runner.system_prompt = session.system_prompt;

        for message in session.messages {
            runner.agent.add_message(message);
        }

        Ok(runner)
    }

    pub fn messages(&self) -> &Vec<Message> {
        self.agent.get_messages()
    }

    pub fn session_id(&self) -> Option<&SessionId> {
        self.session_id.as_ref()
    }

    pub fn snapshot(&self) -> PersistedSession {
        PersistedSession::new(
            self.session_id
                .clone()
                .unwrap_or_else(|| SessionId::new(uuid::Uuid::new_v4().to_string())),
            self.cwd.clone(),
            self.model.clone(),
            self.system_prompt.clone(),
            self.agent.get_messages().clone(),
        )
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
    use crate::session_store::PersistedSession;
    use ccc_core::SessionId;

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

    #[test]
    fn restores_runner_from_persisted_session() {
        let session = PersistedSession::new(
            SessionId::new("sess-1"),
            "/tmp/project".into(),
            "claude-opus-4-6".into(),
            Some("system".into()),
            vec![Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            }],
        );

        let runner = SessionRunner::from_persisted_session(session).unwrap();

        assert_eq!(runner.session_id().map(|id| id.as_str()), Some("sess-1"));
        assert_eq!(latest_assistant_text(runner.messages()), "hello");
    }
}
