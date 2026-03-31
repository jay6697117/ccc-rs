//! OAuth endpoint configuration.
//! Corresponds to TS `src/constants/oauth.ts` → `getOauthConfig()`.

/// All URLs and identifiers needed to drive the OAuth PKCE flow.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: &'static str,
    pub console_authorize_url: &'static str,
    pub claude_ai_authorize_url: &'static str,
    pub token_url: &'static str,
    pub manual_redirect_url: &'static str,
    pub api_key_url: &'static str,
    pub roles_url: &'static str,
}

pub const CLAUDE_AI_INFERENCE_SCOPE: &str = "claude_ai:inference";
pub const CLAUDE_AI_PROFILE_SCOPE: &str = "claude_ai:profile";

pub const CLAUDE_AI_OAUTH_SCOPES: &[&str] = &[
    CLAUDE_AI_INFERENCE_SCOPE,
    CLAUDE_AI_PROFILE_SCOPE,
    "openid",
    "offline_access",
];

pub const ALL_OAUTH_SCOPES: &[&str] = &[
    CLAUDE_AI_INFERENCE_SCOPE,
    CLAUDE_AI_PROFILE_SCOPE,
    "openid",
    "offline_access",
];

/// 5-minute buffer before treating a token as expired.
pub const EXPIRY_BUFFER_MS: u64 = 5 * 60 * 1_000;

pub const DEFAULT_OAUTH_CONFIG: OAuthConfig = OAuthConfig {
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    console_authorize_url: "https://console.anthropic.com/oauth/authorize",
    claude_ai_authorize_url: "https://claude.ai/oauth/authorize",
    token_url: "https://console.anthropic.com/v1/oauth/token",
    manual_redirect_url: "https://console.anthropic.com/oauth/code",
    api_key_url: "https://api.anthropic.com/v1/oauth/api_key",
    roles_url: "https://api.anthropic.com/v1/oauth/roles",
};

pub fn get_oauth_config() -> &'static OAuthConfig {
    &DEFAULT_OAUTH_CONFIG
}
