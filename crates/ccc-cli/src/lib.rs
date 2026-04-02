pub mod cli;
pub mod commands;
pub mod error;
pub mod managed;
pub mod output;
pub mod plugins;
pub mod runtime;
pub mod stdin;

use ccc_telemetry::{init_telemetry, TelemetryConfig, TelemetryFormat};
use cli::{Cli, Commands};
use error::{CliError, CliExit};

pub async fn run(cli: Cli) -> CliExit {
    let telemetry_format = match cli.telemetry_format.parse::<TelemetryFormat>() {
        Ok(format) => format,
        Err(err) => return CliExit::error(err, 2),
    };
    let telemetry = TelemetryConfig {
        format: telemetry_format,
        filter: cli.telemetry_filter.clone(),
    };
    if let Err(err) = init_telemetry(&telemetry) {
        return CliExit::error(err, 1);
    }

    match cli.command {
        Commands::Login(args) => finish(commands::login::run(args).await),
        Commands::Chat(args) => commands::chat::run(args).await,
        Commands::Config(args) => finish(commands::config::run(args).await),
    }
}

fn finish(result: Result<(), CliError>) -> CliExit {
    match result {
        Ok(()) => CliExit::success(),
        Err(error) => error.into(),
    }
}
