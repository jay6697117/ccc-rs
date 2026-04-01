use crate::cli::ChatArgs;
use crate::commands::config::{default_paths, load_config_snapshot};
use crate::error::CliError;
use crate::runtime::{build_chat_runtime, SessionMode};
use crate::stdin::{merge_prompt_and_stdin, read_stdin_if_piped};

use ccc_agent::SessionRunner;
use ccc_tui::AppConfig;

pub async fn run(args: ChatArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()?;
    let snapshot = load_config_snapshot(&default_paths(cwd.clone()))?;
    let runtime = build_chat_runtime(args.clone(), snapshot, cwd)?;

    if runtime.session_mode == SessionMode::Ephemeral {
        return run_print(args, runtime).await;
    }

    ccc_tui::run_app(AppConfig {
        model: runtime.model,
        system_prompt: runtime.system_prompt,
    })
    .await?;

    Ok(())
}

async fn run_print(args: ChatArgs, runtime: crate::runtime::ChatRuntimeConfig) -> Result<(), CliError> {
    let stdin = read_stdin_if_piped()?;
    let input = merge_prompt_and_stdin(args.prompt_text().as_deref(), stdin.as_deref())
        .ok_or_else(|| CliError::new("chat --print requires a prompt or piped stdin", 2))?;

    let mut runner = SessionRunner::new(runtime.model, runtime.system_prompt)?;
    let summary = runner.run_with_events(input, |_event| {}).await?;

    if !summary.assistant_text.is_empty() {
        println!("{}", summary.assistant_text);
    }

    Ok(())
}
