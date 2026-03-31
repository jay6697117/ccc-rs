//! OAuth PKCE flow: token exchange, refresh, expiry.
//! Corresponds to TS `src/services/oauth/client.ts`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::{
    config::{get_oauth_config, CLAUDE_AI_OAUTH_SCOPES, EXPIRY_BUFFER_MS},
    crypto::{generate_code_challenge, generate_code_verifier, generate_state},
    storage::{read_credentials, write_credentials},
    types::OAuthTokens,
};

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_scopes(scope: Option<&str>) -> Vec<String> {
    scope
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns true when the access token is within the 5-minute expiry buffer.
/// Corresponds to TS `isOAuthTokenExpired()`.
pub fn is_oauth_token_expired(expires_at: u64) -> bool {
    now_ms() + EXPIRY_BUFFER_MS >= expires_at
}

/// Build the browser authorization URL with PKCE parameters.
/// Returns `(url, code_verifier, state)` — caller must pass verifier & state
/// to [`exchange_code`].
///
/// `use_claude_ai`: true → claude.ai/oauth, false → console.anthropic.com/oauth
pub fn build_authorize_url(
    port: u16,
    scopes: &[&str],
    use_claude_ai: bool,
) -> (String, String, String) {
    let cfg = get_oauth_config();
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = generate_state();

    let base = if use_claude_ai {
        cfg.claude_ai_authorize_url
    } else {
        cfg.console_authorize_url
    };

    let redirect_uri =
        format!("http://localhost:{port}/callback");
    let scope = scopes.join("%20");

    let url = format!(
        "{base}?response_type=code\
        &client_id={client_id}\
        &redirect_uri={redirect_uri}\
        &scope={scope}\
        &code_challenge={challenge}\
        &code_challenge_method=S256\
        &state={state}",
        client_id = cfg.client_id,
    );

    (url, verifier, state)
}

/// Exchange an authorization code for tokens.
/// Corresponds to TS `exchangeAuthorizationCode()`.
pub async fn exchange_code(
    client: &reqwest::Client,
    authorization_code: &str,
    code_verifier: &str,
    state: &str,
    port: u16,
) -> Result<OAuthTokens> {
    let cfg = get_oauth_config();
    let redirect_uri = format!("http://localhost:{port}/callback");

    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": authorization_code,
        "redirect_uri": redirect_uri,
        "client_id": cfg.client_id,
        "code_verifier": code_verifier,
        "state": state,
    });

    let resp = client
        .post(cfg.token_url)
        .json(&body)
        .send()
        .await
        .context("token exchange request")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Authentication failed: Invalid authorization code");
    }
    if !resp.status().is_success() {
        let status = resp.status();
        bail!("Token exchange failed ({status})");
    }

    let data: TokenExchangeResponse = resp.json().await.context("parse token exchange response")?;
    let expires_at = now_ms() + data.expires_in * 1_000;
    let scopes = parse_scopes(data.scope.as_deref());

    Ok(OAuthTokens {
        access_token: data.access_token,
        refresh_token: data.refresh_token.unwrap_or_default(),
        expires_at,
        scopes,
        subscription_type: None,
        rate_limit_tier: None,
    })
}

/// Refresh an access token using a refresh token.
/// Corresponds to TS `refreshOAuthToken()`.
pub async fn refresh_token(
    client: &reqwest::Client,
    refresh_token: &str,
    scopes: Option<&[&str]>,
) -> Result<OAuthTokens> {
    let cfg = get_oauth_config();
    let scope = scopes.unwrap_or(CLAUDE_AI_OAUTH_SCOPES).join(" ");

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": cfg.client_id,
        "scope": scope,
    });

    let resp = client
        .post(cfg.token_url)
        .json(&body)
        .send()
        .await
        .context("token refresh request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        bail!("Token refresh failed ({status})");
    }

    let data: TokenExchangeResponse = resp.json().await.context("parse token refresh response")?;
    let new_refresh = data
        .refresh_token
        .unwrap_or_else(|| refresh_token.to_owned());
    let expires_at = now_ms() + data.expires_in * 1_000;
    let scopes = parse_scopes(data.scope.as_deref());

    // Preserve existing subscription/tier from storage if not returned by server.
    let existing = read_credentials().ok().and_then(|d| d.oauth_token);
    let subscription_type = existing
        .as_ref()
        .and_then(|t| t.subscription_type.clone());
    let rate_limit_tier = existing
        .as_ref()
        .and_then(|t| t.rate_limit_tier.clone());

    Ok(OAuthTokens {
        access_token: data.access_token,
        refresh_token: new_refresh,
        expires_at,
        scopes,
        subscription_type,
        rate_limit_tier,
    })
}

/// Persist OAuth tokens to secure storage.
pub fn save_oauth_tokens(tokens: &OAuthTokens) -> Result<()> {
    let mut data = read_credentials().unwrap_or_default();
    data.oauth_token = Some(tokens.clone());
    write_credentials(&data)
}

/// Load OAuth tokens from secure storage, refreshing if expired.
/// Returns `None` when no tokens are stored.
pub async fn load_oauth_tokens(client: &reqwest::Client) -> Result<Option<OAuthTokens>> {
    let data = read_credentials().unwrap_or_default();
    let tokens = match data.oauth_token {
        Some(t) => t,
        None => return Ok(None),
    };

    if !is_oauth_token_expired(tokens.expires_at) {
        return Ok(Some(tokens));
    }

    // Expired — refresh.
    let refreshed = refresh_token(client, &tokens.refresh_token, None).await?;
    save_oauth_tokens(&refreshed)?;
    Ok(Some(refreshed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_when_in_past() {
        // expires_at 0 → definitely expired
        assert!(is_oauth_token_expired(0));
    }

    #[test]
    fn not_expired_when_far_future() {
        let far_future = now_ms() + 60 * 60 * 1_000; // +1 hour
        assert!(!is_oauth_token_expired(far_future));
    }

    #[test]
    fn build_authorize_url_contains_required_params() {
        let (url, verifier, state) =
            build_authorize_url(12345, CLAUDE_AI_OAUTH_SCOPES, false);
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&state));
        assert!(!verifier.is_empty());
    }

    #[test]
    fn parse_scopes_splits_on_whitespace() {
        let scopes = parse_scopes(Some("openid offline_access claude_ai:inference"));
        assert_eq!(scopes.len(), 3);
    }
}
