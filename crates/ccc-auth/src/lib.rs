//! `ccc-auth` — Authentication layer for ccc-rs.
//!
//! Covers:
//!   - PKCE OAuth flow (browser-based, local callback server)
//!   - Token exchange & refresh
//!   - API-key resolution (env → file-descriptor → helper → secure storage)
//!   - Cross-platform credential storage (macOS keychain / plaintext fallback)

pub mod api_key;
pub mod auth_code_listener;
pub mod config;
pub mod crypto;
pub mod oauth;
pub mod storage;
pub mod types;

// Convenient re-exports
pub use api_key::{clear_api_key_helper_cache, resolve_api_key, run_api_key_helper};
pub use auth_code_listener::AuthCodeListener;
pub use config::{get_oauth_config, OAuthConfig, CLAUDE_AI_OAUTH_SCOPES, EXPIRY_BUFFER_MS};
pub use crypto::{generate_code_challenge, generate_code_verifier, generate_state};
pub use oauth::{
    build_authorize_url, exchange_code, is_oauth_token_expired, load_oauth_tokens,
    refresh_token, save_oauth_tokens,
};
pub use storage::{delete_credentials, read_credentials, write_credentials};
pub use types::{ApiKeySource, OAuthTokens, ResolvedApiKey, SecureStorageData, SubscriptionType};
