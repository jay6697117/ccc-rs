use crate::cli::LoginArgs;
use crate::error::CliError;

use ccc_auth::{
    build_authorize_url, exchange_code, save_oauth_tokens, AuthCodeListener, CLAUDE_AI_OAUTH_SCOPES,
};

pub async fn run(_args: LoginArgs) -> Result<(), CliError> {
    let listener = AuthCodeListener::bind().await?;
    let port = listener.port();
    let (authorize_url, verifier, state) = build_authorize_url(port, CLAUDE_AI_OAUTH_SCOPES, true);

    println!("Open this URL to authorize:");
    println!("{authorize_url}");

    if let Err(error) = open_browser(&authorize_url) {
        eprintln!("Failed to open browser automatically: {error}");
        eprintln!("Continue the login flow manually with the URL above.");
    }

    let code = listener.wait_for_code(&state).await?;
    let client = reqwest::Client::new();
    let tokens = exchange_code(&client, &code, &verifier, &state, port).await?;
    save_oauth_tokens(&tokens)?;

    println!("Login successful.");
    Ok(())
}

fn open_browser(url: &str) -> Result<(), CliError> {
    #[cfg(target_os = "macos")]
    {
        return run_browser_command("open", &[url]);
    }

    #[cfg(target_os = "linux")]
    {
        return run_browser_command("xdg-open", &[url]);
    }

    #[cfg(target_os = "windows")]
    {
        return run_browser_command("cmd", &["/C", "start", "", url]);
    }

    #[allow(unreachable_code)]
    Err(CliError::new(
        "automatic browser launch is not supported on this platform",
        1,
    ))
}

fn run_browser_command(command: &str, args: &[&str]) -> Result<(), CliError> {
    let status = std::process::Command::new(command).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::new(
            format!("browser command exited with status {status}"),
            1,
        ))
    }
}
