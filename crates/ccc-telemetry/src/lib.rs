use std::str::FromStr;
use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryFormat {
    Noop,
    Pretty,
    Json,
}

impl Default for TelemetryFormat {
    fn default() -> Self {
        Self::Noop
    }
}

impl FromStr for TelemetryFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "noop" => Ok(Self::Noop),
            "pretty" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            other => Err(format!("unsupported telemetry format: {other}")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TelemetryConfig {
    pub format: TelemetryFormat,
    pub filter: Option<String>,
}

static TELEMETRY_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn init_telemetry(config: &TelemetryConfig) -> Result<(), String> {
    TELEMETRY_INIT
        .get_or_init(|| initialize_subscriber(config))
        .clone()
}

fn initialize_subscriber(config: &TelemetryConfig) -> Result<(), String> {
    match config.format {
        TelemetryFormat::Noop => Ok(()),
        TelemetryFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(build_filter(config)?)
            .with_writer(std::io::stderr)
            .pretty()
            .try_init()
            .map_err(|err| err.to_string()),
        TelemetryFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(build_filter(config)?)
            .with_writer(std::io::stderr)
            .json()
            .try_init()
            .map_err(|err| err.to_string()),
    }
}

fn build_filter(config: &TelemetryConfig) -> Result<EnvFilter, String> {
    EnvFilter::try_new(config.filter.as_deref().unwrap_or("info")).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_formats() {
        assert_eq!(
            "noop".parse::<TelemetryFormat>().unwrap(),
            TelemetryFormat::Noop
        );
        assert_eq!(
            "pretty".parse::<TelemetryFormat>().unwrap(),
            TelemetryFormat::Pretty
        );
        assert_eq!(
            "json".parse::<TelemetryFormat>().unwrap(),
            TelemetryFormat::Json
        );
    }

    #[test]
    fn noop_initialization_is_idempotent() {
        let config = TelemetryConfig::default();
        init_telemetry(&config).unwrap();
        init_telemetry(&config).unwrap();
    }

    #[test]
    fn pretty_initialization_is_accepted() {
        let config = TelemetryConfig {
            format: TelemetryFormat::Pretty,
            filter: Some("info".into()),
        };

        init_telemetry(&config).unwrap();
    }

    #[test]
    fn json_initialization_is_accepted() {
        let config = TelemetryConfig {
            format: TelemetryFormat::Json,
            filter: Some("debug".into()),
        };

        init_telemetry(&config).unwrap();
    }
}
