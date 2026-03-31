//! Auth-layer types.  Corresponds to TS `src/services/oauth/types.ts`.

use serde::{Deserialize, Serialize};

/// OAuth token bundle persisted to secure storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix-millisecond timestamp when the access token expires.
    pub expires_at: u64,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<SubscriptionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionType {
    Free,
    Pro,
    Max,
    #[serde(other)]
    Unknown,
}

/// What is stored under a single key in `.credentials.json` / keychain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecureStorageData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_token: Option<OAuthTokens>,
    /// Direct API key stored in secure storage (not env).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Where the API key ultimately came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeySource {
    AnthropicApiKeyEnv,
    FileDescriptor,
    ApiKeyHelper,
    Keychain,
    PlainTextStorage,
    None,
}

impl std::fmt::Display for ApiKeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiKeySource::AnthropicApiKeyEnv => write!(f, "ANTHROPIC_API_KEY"),
            ApiKeySource::FileDescriptor => write!(f, "file-descriptor"),
            ApiKeySource::ApiKeyHelper => write!(f, "apiKeyHelper"),
            ApiKeySource::Keychain => write!(f, "keychain"),
            ApiKeySource::PlainTextStorage => write!(f, "plaintext"),
            ApiKeySource::None => write!(f, "none"),
        }
    }
}

/// Resolved API key + where it came from.
#[derive(Debug, Clone)]
pub struct ResolvedApiKey {
    pub key: Option<String>,
    pub source: ApiKeySource,
}
