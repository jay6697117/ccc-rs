use crate::cli::ChatArgs;
use crate::error::CliError;
use crate::stdin::{merge_prompt_and_stdin, read_stdin_if_piped};

use ccc_agent::SessionRunner;
use ccc_tui::AppConfig;

const DEFAULT_MODEL: &str = "claude-opus-4-6";

pub async fn run(args: ChatArgs) -> Result<(), CliError> {
    if args.print {
        return run_print(args).await;
    }

    ccc_tui::run_app(AppConfig {
        model: args.model.unwrap_or_else(|| DEFAULT_MODEL.into()),
        system_prompt: args.system_prompt,
    })
    .await?;

    Ok(())
}

async fn run_print(args: ChatArgs) -> Result<(), CliError> {
    let stdin = read_stdin_if_piped()?;
    let input = merge_prompt_and_stdin(args.prompt_text().as_deref(), stdin.as_deref())
        .ok_or_else(|| CliError::new("chat --print requires a prompt or piped stdin", 2))?;

    let mut runner = SessionRunner::new(
        args.model.unwrap_or_else(|| DEFAULT_MODEL.into()),
        args.system_prompt,
    )?;
    let summary = runner.run_with_events(input, |_event| {}).await?;

    if !summary.assistant_text.is_empty() {
        println!("{}", summary.assistant_text);
    }

    Ok(())
}
