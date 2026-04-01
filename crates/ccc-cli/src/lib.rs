pub mod cli;
pub mod commands;
pub mod error;
pub mod stdin;

use ccc_telemetry::{init_telemetry, TelemetryConfig, TelemetryFormat};
use cli::{Cli, Commands};
use error::CliError;

pub async fn run(cli: Cli) -> Result<(), CliError> {
    let telemetry_format = cli
        .telemetry_format
        .parse::<TelemetryFormat>()
        .map_err(|err| CliError::new(err, 2))?;
    let telemetry = TelemetryConfig {
        format: telemetry_format,
        filter: cli.telemetry_filter.clone(),
    };
    init_telemetry(&telemetry).map_err(|err| CliError::new(err, 1))?;

    match cli.command {
        Commands::Login(args) => commands::login::run(args).await,
        Commands::Chat(args) => commands::chat::run(args).await,
        Commands::Config(args) => commands::config::run(args).await,
    }
}
