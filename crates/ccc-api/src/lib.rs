//! `ccc-api` — Anthropic Messages API client for ccc-rs.
//!
//! Supports streaming (SSE) and non-streaming requests across four providers:
//! - Direct Anthropic API
//! - AWS Bedrock
//! - Google Vertex AI
//! - Azure AI Foundry

pub mod client;
pub mod error;
pub mod provider;
pub mod retry;
pub mod stream;
pub mod types;

pub use client::AnthropicClient;
pub use error::ApiError;
pub use provider::Provider;
pub use retry::{backoff_delay, with_retry, RetryConfig};
pub use stream::{parse_sse, EventStream};
pub use types::{MessagesRequest, MessagesResponse, StreamEvent, Usage};
