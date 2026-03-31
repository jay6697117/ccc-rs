//! Anthropic Messages API client.
//! Corresponds to TS `src/services/api/claude.ts` + `src/services/api/client.ts`.
//!
//! Supports:
//!   - Direct Anthropic API
//!   - AWS Bedrock (env-based detection)
//!   - Google Vertex AI (env-based detection)
//!   - Azure Foundry (env-based detection)
//!   - Streaming (SSE) and non-streaming modes
//!   - Automatic retry on 429 / 529

use anyhow::Context;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response,
};
use serde_json::Value;

use ccc_auth::resolve_api_key;

use crate::{
    error::ApiError,
    provider::Provider,
    retry::{with_retry, RetryConfig},
    stream::{parse_sse, EventStream},
    types::{MessagesRequest, MessagesResponse},
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_USER_AGENT: &str = concat!("ccc-rs/", env!("CARGO_PKG_VERSION"));

/// High-level Anthropic API client.
#[derive(Clone)]
pub struct AnthropicClient {
    http: Client,
    provider: Provider,
    retry: RetryConfig,
}

impl AnthropicClient {
    /// Build a client from environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        let http = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            http,
            provider: Provider::from_env(),
            retry: RetryConfig::default(),
        })
    }

    /// Override retry config (useful for tests).
    pub fn with_retry(mut self, cfg: RetryConfig) -> Self {
        self.retry = cfg;
        self
    }

    // ── Auth helpers ──────────────────────────────────────────────────────────

    /// Build request headers appropriate for the current provider.
    async fn build_headers(
        &self,
        extra_betas: &[String],
    ) -> Result<HeaderMap, ApiError> {
        let mut headers = HeaderMap::new();

        match self.provider {
            Provider::Anthropic => {
                let resolved = resolve_api_key()
                    .map_err(ApiError::Other)?;
                let key_str = resolved.key.ok_or_else(|| {
                    ApiError::Other(anyhow::anyhow!("No API key available"))
                })?;
                headers.insert(
                    "x-api-key",
                    HeaderValue::from_str(&key_str)
                        .map_err(|e| ApiError::Other(anyhow::anyhow!(e)))?,
                );
                headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(ANTHROPIC_VERSION),
                );
            }
            Provider::Bedrock => {
                // Bedrock uses AWS SigV4 — bearer token injected by the SDK layer.
                // For now we just signal that auth must be handled externally.
                headers.insert(
                    "x-amzn-bedrock-accept",
                    HeaderValue::from_static("application/json"),
                );
            }
            Provider::Vertex => {
                // Vertex uses OAuth2 access tokens via GOOGLE_APPLICATION_CREDENTIALS.
                headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(ANTHROPIC_VERSION),
                );
            }
            Provider::Foundry => {
                if let Ok(key) = std::env::var("ANTHROPIC_FOUNDRY_API_KEY") {
                    headers.insert(
                        "api-key",
                        HeaderValue::from_str(&key)
                            .map_err(|e| ApiError::Other(anyhow::anyhow!(e)))?,
                    );
                }
                headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(ANTHROPIC_VERSION),
                );
            }
        }

        // Beta headers
        if !extra_betas.is_empty() {
            let val = extra_betas.join(",");
            headers.insert(
                "anthropic-beta",
                HeaderValue::from_str(&val)
                    .map_err(|e| ApiError::Other(anyhow::anyhow!(e)))?,
            );
        }

        Ok(headers)
    }

    // ── Request building ──────────────────────────────────────────────────────

    fn endpoint_url(&self, model: &str, streaming: bool) -> String {
        let base = self.provider.base_url();
        let path = self.provider.messages_path(model);
        let _ = streaming; // future: non-streaming path may differ
        format!("{base}{path}")
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Send a streaming request; returns an async stream of `StreamEvent`.
    pub async fn stream(
        &self,
        mut req: MessagesRequest,
        betas: &[String],
    ) -> Result<EventStream, ApiError> {
        req.stream = Some(true);
        let model = req.model.clone();
        let url = self.endpoint_url(&model, true);
        let headers = self.build_headers(betas).await?;
        let body = serde_json::to_value(&req).map_err(ApiError::Json)?;

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(ApiError::Http)?;

        let response = check_status(response).await?;
        Ok(parse_sse(response.bytes_stream()))
    }

    /// Send a non-streaming request; returns the complete response.
    pub async fn send(
        &self,
        req: MessagesRequest,
        betas: &[String],
    ) -> Result<MessagesResponse, ApiError> {
        let retry = self.retry.clone();
        let client = self.clone();

        with_retry(&retry, |_attempt| {
            let client = client.clone();
            let req = req.clone();
            let betas = betas.to_vec();
            async move { client.send_once(req, &betas).await }
        })
        .await
    }

    async fn send_once(
        &self,
        req: MessagesRequest,
        betas: &[String],
    ) -> Result<MessagesResponse, ApiError> {
        let model = req.model.clone();
        let url = self.endpoint_url(&model, false);
        let headers = self.build_headers(betas).await?;
        let body = serde_json::to_value(&req).map_err(ApiError::Json)?;

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(ApiError::Http)?;

        let response = check_status(response).await?;
        response.json::<MessagesResponse>().await.map_err(ApiError::Http)
    }
}

// ── Status-code → ApiError mapping ───────────────────────────────────────────

async fn check_status(resp: Response) -> Result<Response, ApiError> {
    let status = resp.status().as_u16();
    if resp.status().is_success() {
        return Ok(resp);
    }
    match status {
        401 => Err(ApiError::Unauthorized),
        429 => {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            Err(ApiError::RateLimited { retry_after_secs: retry_after })
        }
        529 => Err(ApiError::Overloaded),
        _ => {
            // Try to extract a structured error message from the body.
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            Err(ApiError::Api { status, message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_url_anthropic() {
        std::env::remove_var("ANTHROPIC_BASE_URL");
        std::env::remove_var("CLAUDE_CODE_USE_BEDROCK");
        std::env::remove_var("CLAUDE_CODE_USE_VERTEX");
        std::env::remove_var("CLAUDE_CODE_USE_FOUNDRY");
        let client = AnthropicClient {
            http: Client::new(),
            provider: Provider::Anthropic,
            retry: RetryConfig::default(),
        };
        let url = client.endpoint_url("claude-opus-4-6", true);
        assert_eq!(url, "https://api.anthropic.com/messages");
    }
}
