use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = ccc_cli::cli::Cli::parse();
    let outcome = ccc_cli::run(cli).await;

    if let Some(message) = outcome.stderr_message() {
        eprintln!("{message}");
    }

    std::process::ExitCode::from(outcome.exit_code())
}
