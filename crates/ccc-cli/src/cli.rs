use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(name = "ccc")]
pub struct Cli {
    #[arg(long, global = true, default_value = "noop")]
    pub telemetry_format: String,
    #[arg(long, global = true)]
    pub telemetry_filter: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Commands {
    Login(LoginArgs),
    Chat(ChatArgs),
    Config(ConfigArgs),
}

#[derive(Debug, Args, PartialEq, Eq, Default)]
pub struct LoginArgs {}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ChatArgs {
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub system_prompt: Option<String>,
    #[arg(long)]
    pub print: bool,
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    Show,
}

impl ChatArgs {
    pub fn prompt_text(&self) -> Option<String> {
        if self.prompt.is_empty() {
            None
        } else {
            Some(self.prompt.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat_print_with_prompt() {
        let cli = Cli::try_parse_from([
            "ccc",
            "--telemetry-format",
            "json",
            "chat",
            "--print",
            "explain",
            "this",
        ])
        .unwrap();

        assert_eq!(cli.telemetry_format, "json");
        match cli.command {
            Commands::Chat(args) => {
                assert!(args.print);
                assert_eq!(args.prompt_text().as_deref(), Some("explain this"));
            }
            other => panic!("expected chat command, got {other:?}"),
        }
    }

    #[test]
    fn parses_config_show_command() {
        let cli = Cli::try_parse_from(["ccc", "config", "show"]).unwrap();

        match cli.command {
            Commands::Config(args) => assert_eq!(args.command, ConfigCommand::Show),
            other => panic!("expected config command, got {other:?}"),
        }
    }
}
