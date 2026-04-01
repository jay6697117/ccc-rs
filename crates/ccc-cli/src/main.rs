use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = ccc_cli::cli::Cli::parse();

    match ccc_cli::run(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            std::process::ExitCode::from(err.exit_code())
        }
    }
}
