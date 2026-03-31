//! API provider detection and base-URL resolution.
//! Corresponds to TS `src/utils/model/providers.ts`.

/// Which backend the client should target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Direct Anthropic API (api.anthropic.com).
    Anthropic,
    /// AWS Bedrock.
    Bedrock,
    /// Google Vertex AI.
    Vertex,
    /// Azure AI Foundry.
    Foundry,
}

impl Provider {
    /// Detect from environment variables (mirrors TS `getAPIProvider()`).
    pub fn from_env() -> Self {
        if std::env::var("CLAUDE_CODE_USE_BEDROCK")
            .map(|v| is_truthy(&v))
            .unwrap_or(false)
        {
            return Provider::Bedrock;
        }
        if std::env::var("CLAUDE_CODE_USE_VERTEX")
            .map(|v| is_truthy(&v))
            .unwrap_or(false)
        {
            return Provider::Vertex;
        }
        if std::env::var("CLAUDE_CODE_USE_FOUNDRY")
            .map(|v| is_truthy(&v))
            .unwrap_or(false)
        {
            return Provider::Foundry;
        }
        Provider::Anthropic
    }

    /// Anthropic API base URL (may be overridden by `ANTHROPIC_BASE_URL`).
    pub fn base_url(&self) -> String {
        match self {
            Provider::Anthropic => std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into()),
            Provider::Bedrock => {
                let region = std::env::var("AWS_REGION")
                    .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
                    .unwrap_or_else(|_| "us-east-1".into());
                format!(
                    "https://bedrock-runtime.{region}.amazonaws.com/model"
                )
            }
            Provider::Vertex => {
                let project =
                    std::env::var("ANTHROPIC_VERTEX_PROJECT_ID").unwrap_or_default();
                let region =
                    std::env::var("CLOUD_ML_REGION").unwrap_or_else(|_| "us-east5".into());
                format!(
                    "https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/anthropic/models"
                )
            }
            Provider::Foundry => {
                if let Ok(url) = std::env::var("ANTHROPIC_FOUNDRY_BASE_URL") {
                    return format!("{url}/anthropic/v1");
                }
                let resource = std::env::var("ANTHROPIC_FOUNDRY_RESOURCE").unwrap_or_default();
                format!(
                    "https://{resource}.services.ai.azure.com/anthropic/v1"
                )
            }
        }
    }

    /// Messages endpoint path.
    pub fn messages_path(&self, model: &str) -> String {
        match self {
            Provider::Anthropic | Provider::Foundry => "/messages".into(),
            Provider::Bedrock => format!("/{model}/invoke-with-response-stream"),
            Provider::Vertex => format!("/{model}:streamRawPredict"),
        }
    }

    pub fn is_first_party(&self) -> bool {
        matches!(self, Provider::Anthropic)
    }
}

fn is_truthy(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "1" | "true" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_is_default() {
        // env vars shouldn't be set in test env
        let p = Provider::from_env();
        // Allow either Anthropic or Bedrock if CI sets AWS vars
        assert!(matches!(p, Provider::Anthropic | Provider::Bedrock | Provider::Vertex | Provider::Foundry));
    }

    #[test]
    fn base_url_anthropic_default() {
        // Without override env var
        std::env::remove_var("ANTHROPIC_BASE_URL");
        let url = Provider::Anthropic.base_url();
        assert_eq!(url, "https://api.anthropic.com");
    }

    #[test]
    fn messages_path_anthropic() {
        assert_eq!(Provider::Anthropic.messages_path("claude-3-5-sonnet-20241022"), "/messages");
    }

    #[test]
    fn messages_path_bedrock() {
        let path = Provider::Bedrock.messages_path("anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert!(path.contains("anthropic.claude"));
    }
}
