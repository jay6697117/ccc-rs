/// Branded session ID. Corresponds to TS `SessionId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Branded agent ID. Corresponds to TS `AgentId`.
/// Format: `a` + optional `<label>-` + 16 hex chars.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

const AGENT_ID_PREFIX: char = 'a';

impl AgentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Validate format produced by createAgentId(): `a[<label>-]<16 hex chars>`.
    pub fn parse(s: &str) -> Option<Self> {
        if !s.starts_with(AGENT_ID_PREFIX) {
            return None;
        }
        let rest = &s[1..];
        // last 16 chars must be hex
        if rest.len() < 16 {
            return None;
        }
        let hex_part = &rest[rest.len() - 16..];
        if hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(Self(s.to_owned()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_roundtrip() {
        let id = SessionId::new("sess-abc");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"sess-abc\"");
        let back: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn agent_id_valid() {
        let id = AgentId::parse("a1234567890abcdef");
        assert!(id.is_some());
    }

    #[test]
    fn agent_id_with_label() {
        let id = AgentId::parse("asome-label-1234567890abcdef");
        assert!(id.is_some());
    }

    #[test]
    fn agent_id_invalid_no_prefix() {
        assert!(AgentId::parse("1234567890abcdef").is_none());
    }

    #[test]
    fn agent_id_invalid_short() {
        assert!(AgentId::parse("a123").is_none());
    }
}
