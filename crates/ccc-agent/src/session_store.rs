use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ccc_core::{Message, SessionId};
use serde::{Deserialize, Serialize};

const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedSession {
    pub version: u32,
    pub session_id: SessionId,
    pub cwd: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
}

impl PersistedSession {
    pub fn new(
        session_id: SessionId,
        cwd: String,
        model: String,
        system_prompt: Option<String>,
        messages: Vec<Message>,
    ) -> Self {
        Self {
            version: SESSION_VERSION,
            session_id,
            cwd,
            model,
            system_prompt,
            messages,
        }
    }

    pub fn fresh(
        cwd: impl Into<String>,
        model: impl Into<String>,
        system_prompt: Option<String>,
        messages: Vec<Message>,
    ) -> Self {
        Self::new(
            SessionId::new(uuid::Uuid::new_v4().to_string()),
            cwd.into(),
            model.into(),
            system_prompt,
            messages,
        )
    }
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn save(&self, session: &PersistedSession) -> Result<()> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .with_context(|| format!("create session store dir {}", self.root.display()))?;

        let json = serde_json::to_string_pretty(session).context("serialize persisted session")?;
        tokio::fs::write(self.session_path(&session.session_id), json)
            .await
            .context("write persisted session")?;
        Ok(())
    }

    pub async fn load(&self, session_id: &SessionId) -> Result<Option<PersistedSession>> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Ok(None);
        }

        let json = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("read persisted session {}", path.display()))?;
        let session: PersistedSession =
            serde_json::from_str(&json).context("parse persisted session JSON")?;
        anyhow::ensure!(
            session.version == SESSION_VERSION,
            "unsupported persisted session version {}",
            session.version
        );
        Ok(Some(session))
    }

    pub fn session_path(&self, session_id: &SessionId) -> PathBuf {
        self.root.join(format!("{}.json", session_id.as_str()))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use ccc_core::{ContentBlock, Message, Role, SessionId};

    use super::{PersistedSession, SessionStore};

    #[tokio::test]
    async fn saves_and_loads_session_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path().into());
        let session = PersistedSession::new(
            SessionId::new("sess-1"),
            "/tmp/project".into(),
            "claude-opus-4-6".into(),
            Some("system".into()),
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            }],
        );

        store.save(&session).await.unwrap();
        let loaded = store.load(&session.session_id).await.unwrap().unwrap();

        assert_eq!(loaded.messages, session.messages);
        assert_eq!(loaded.model, session.model);
        assert_eq!(loaded.system_prompt, session.system_prompt);
    }

    #[tokio::test]
    async fn missing_session_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path().into());

        assert!(store
            .load(&SessionId::new("missing"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn incompatible_session_version_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path().into());
        let path = store.session_path(&SessionId::new("sess-1"));

        tokio::fs::create_dir_all(store.root()).await.unwrap();
        tokio::fs::write(
            &path,
            serde_json::json!({
                "version": 999,
                "session_id": "sess-1",
                "cwd": "/tmp/project",
                "model": "claude-opus-4-6",
                "system_prompt": null,
                "messages": []
            })
            .to_string(),
        )
        .await
        .unwrap();

        let error = store.load(&SessionId::new("sess-1")).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported persisted session version"));
    }
}
