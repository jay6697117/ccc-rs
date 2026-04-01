use std::time::Instant;

use anyhow::Result;
use ccc_api::types::{StreamEvent, Usage};
use ccc_core::{
    config::McpServerConfig,
    types::{ContentBlock, Message, Role},
    SessionId,
};

use crate::{session_store::PersistedSession, Agent};

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub session_id: SessionId,
    pub assistant_text: String,
    pub assistant_messages: Vec<Message>,
    pub model: String,
    pub duration_ms: u64,
    pub num_turns: usize,
    pub stop_reason: Option<String>,
    pub usage: Usage,
    pub warnings: Vec<String>,
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
            session_id: Some(SessionId::new(uuid::Uuid::new_v4().to_string())),
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

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn snapshot(&self) -> PersistedSession {
        PersistedSession::new(
            self.session_id
                .clone()
                .expect("session runner should always have a session id"),
            self.cwd.clone(),
            self.model.clone(),
            self.system_prompt.clone(),
            self.agent.get_messages().clone(),
        )
    }

    pub async fn bootstrap_mcp_servers(
        &mut self,
        servers: &[(String, McpServerConfig)],
    ) -> Result<Vec<(String, anyhow::Error)>> {
        self.agent.bootstrap_mcp_servers(servers).await
    }

    pub async fn run_with_events<F>(
        &mut self,
        user_input: String,
        mut on_event: F,
    ) -> Result<RunSummary>
    where
        F: FnMut(StreamEvent),
    {
        let started_at = Instant::now();
        let messages_before = self.agent.get_messages().clone();
        let mut metrics = RunMetrics::default();

        self.agent
            .run(user_input, |event| {
                metrics.record_event(&event);
                on_event(event);
            })
            .await?;

        Ok(build_run_summary(
            self.session_id
                .clone()
                .expect("session runner should always have a session id"),
            self.model.clone(),
            &messages_before,
            self.agent.get_messages(),
            elapsed_millis(started_at.elapsed()),
            metrics,
        ))
    }
}

#[derive(Debug, Clone, Default)]
struct RunMetrics {
    stop_reason: Option<String>,
    usage: Usage,
}

impl RunMetrics {
    fn record_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart { message } => {
                self.usage = message.usage.clone();
                if let Some(stop_reason) = message.stop_reason.clone() {
                    self.stop_reason = Some(stop_reason);
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                if let Some(stop_reason) = delta.stop_reason.clone() {
                    self.stop_reason = Some(stop_reason);
                }

                if let Some(output_tokens) = usage
                    .as_ref()
                    .and_then(|delta_usage| delta_usage.output_tokens)
                {
                    self.usage.output_tokens += output_tokens;
                }
            }
            _ => {}
        }
    }
}

fn build_run_summary(
    session_id: SessionId,
    model: String,
    before_messages: &[Message],
    after_messages: &[Message],
    duration_ms: u64,
    metrics: RunMetrics,
) -> RunSummary {
    let assistant_messages: Vec<Message> = after_messages
        .iter()
        .skip(before_messages.len())
        .filter(|message| message.role == Role::Assistant)
        .cloned()
        .collect();

    RunSummary {
        session_id,
        assistant_text: latest_assistant_text(&assistant_messages),
        num_turns: assistant_messages.len(),
        assistant_messages,
        model,
        duration_ms,
        stop_reason: metrics.stop_reason,
        usage: metrics.usage,
        warnings: Vec::new(),
    }
}

fn elapsed_millis(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
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

    #[test]
    fn generated_session_id_is_stable_for_snapshot() {
        let runner = SessionRunner::new("claude-opus-4-6", None).unwrap();

        let session_id = runner.session_id().unwrap().clone();
        let snapshot = runner.snapshot();

        assert_eq!(snapshot.session_id, session_id);
    }

    #[test]
    fn aggregates_usage_stop_reason_and_new_assistant_messages() {
        let before_messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
        }];
        let after_messages = vec![
            before_messages[0].clone(),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Thinking {
                    thinking: "hidden".into(),
                    signature: "sig".into(),
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "final answer".into(),
                }],
            },
        ];
        let mut metrics = RunMetrics::default();

        metrics.record_event(&StreamEvent::MessageStart {
            message: ccc_api::types::MessageStartPayload {
                id: "msg_1".into(),
                model: "claude-opus-4-6".into(),
                usage: ccc_api::types::Usage {
                    input_tokens: 11,
                    output_tokens: 3,
                    cache_creation_input_tokens: 2,
                    cache_read_input_tokens: 1,
                },
                stop_reason: None,
            },
        });
        metrics.record_event(&StreamEvent::MessageDelta {
            delta: ccc_api::types::MessageDeltaPayload {
                stop_reason: Some("end_turn".into()),
                stop_sequence: None,
            },
            usage: Some(ccc_api::types::UsageDelta {
                output_tokens: Some(7),
            }),
        });

        let summary = build_run_summary(
            SessionId::new("sess-test"),
            "claude-opus-4-6".into(),
            &before_messages,
            &after_messages,
            1234,
            metrics,
        );

        assert_eq!(summary.session_id.as_str(), "sess-test");
        assert_eq!(summary.assistant_text, "final answer");
        assert_eq!(summary.assistant_messages.len(), 2);
        assert_eq!(summary.num_turns, 2);
        assert_eq!(summary.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(summary.usage.input_tokens, 11);
        assert_eq!(summary.usage.output_tokens, 10);
        assert_eq!(summary.duration_ms, 1234);
        assert!(summary.warnings.is_empty());
    }
}
