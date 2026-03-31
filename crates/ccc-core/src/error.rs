use thiserror::Error;

#[derive(Debug, Error)]
pub enum CccError {
    #[error("config error: {0}")]
    Config(String),

    #[error("auth error: {0}")]
    Auth(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
