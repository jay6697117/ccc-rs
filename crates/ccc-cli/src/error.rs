use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliExit {
    exit_code: u8,
    stderr_message: Option<String>,
}

impl CliExit {
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stderr_message: None,
        }
    }

    pub fn reported(exit_code: u8) -> Self {
        Self {
            exit_code,
            stderr_message: None,
        }
    }

    pub fn error(message: impl Into<String>, exit_code: u8) -> Self {
        Self {
            exit_code,
            stderr_message: Some(message.into()),
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }

    pub fn stderr_message(&self) -> Option<&str> {
        self.stderr_message.as_deref()
    }
}

#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: u8,
}

impl CliError {
    pub fn new(message: impl Into<String>, exit_code: u8) -> Self {
        Self {
            message: message.into(),
            exit_code,
        }
    }

    pub fn unimplemented(subject: &str) -> Self {
        Self::new(format!("{subject} is not implemented yet"), 1)
    }

    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

impl From<CliError> for CliExit {
    fn from(error: CliError) -> Self {
        Self::error(error.message, error.exit_code)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string(), 1)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string(), 1)
    }
}

impl From<anyhow::Error> for CliError {
    fn from(error: anyhow::Error) -> Self {
        Self::new(error.to_string(), 1)
    }
}
