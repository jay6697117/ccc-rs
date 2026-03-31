//! API error types for ccc-api.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Non-2xx status from the Anthropic API.
    #[error("Anthropic API error {status}: {message}")]
    Api { status: u16, message: String },

    /// Rate-limited (429).
    #[error("Rate limited (429): retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    /// Overloaded (529).
    #[error("Service overloaded (529)")]
    Overloaded,

    /// Auth error (401).
    #[error("Authentication error (401)")]
    Unauthorized,

    /// SSE parse error.
    #[error("SSE parse error: {0}")]
    SseParse(String),

    /// JSON deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// User aborted the stream.
    #[error("Aborted")]
    Aborted,

    /// Any other error.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl ApiError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, ApiError::RateLimited { .. } | ApiError::Overloaded)
    }

    pub fn status_code(&self) -> Option<u16> {
        match self {
            ApiError::Api { status, .. } => Some(*status),
            ApiError::RateLimited { .. } => Some(429),
            ApiError::Overloaded => Some(529),
            ApiError::Unauthorized => Some(401),
            _ => None,
        }
    }
}
