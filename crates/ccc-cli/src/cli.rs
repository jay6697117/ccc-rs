use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::error::CliError;

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

#[derive(Debug, Args, PartialEq, Eq, Clone)]
pub struct ChatArgs {
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub system_prompt: Option<String>,
    #[arg(long)]
    pub print: bool,
    #[arg(long, default_value = "text")]
    pub output_format: OutputFormat,
    #[arg(long)]
    pub include_partial_messages: bool,
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Text,
    Json,
    #[value(name = "stream-json")]
    StreamJson,
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

    pub fn validate_headless_flags(&self) -> Result<(), CliError> {
        if !self.print && self.output_format != OutputFormat::Text {
            return Err(CliError::new(
                "--output-format can only be used with --print",
                2,
            ));
        }

        if self.include_partial_messages
            && (!self.print || self.output_format != OutputFormat::StreamJson)
        {
            return Err(CliError::new(
                "--include-partial-messages requires --print --output-format=stream-json",
                2,
            ));
        }

        Ok(())
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
    fn chat_output_format_defaults_to_text() {
        let cli = Cli::try_parse_from(["ccc", "chat", "--print", "hello"]).unwrap();

        match cli.command {
            Commands::Chat(args) => {
                assert_eq!(args.output_format, OutputFormat::Text);
                assert!(!args.include_partial_messages);
            }
            other => panic!("expected chat command, got {other:?}"),
        }
    }

    #[test]
    fn parses_stream_json_output_format() {
        let cli = Cli::try_parse_from([
            "ccc",
            "chat",
            "--print",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "hello",
        ])
        .unwrap();

        match cli.command {
            Commands::Chat(args) => {
                assert_eq!(args.output_format, OutputFormat::StreamJson);
                assert!(args.include_partial_messages);
            }
            other => panic!("expected chat command, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_text_output_format_without_print() {
        let cli = Cli::try_parse_from(["ccc", "chat", "--output-format", "json", "hello"]).unwrap();

        match cli.command {
            Commands::Chat(args) => {
                let error = args.validate_headless_flags().unwrap_err();
                assert_eq!(error.exit_code(), 2);
                assert!(error
                    .to_string()
                    .contains("--output-format can only be used with --print"));
            }
            other => panic!("expected chat command, got {other:?}"),
        }
    }

    #[test]
    fn rejects_partial_messages_without_stream_json_print() {
        let cli = Cli::try_parse_from([
            "ccc",
            "chat",
            "--print",
            "--include-partial-messages",
            "hello",
        ])
        .unwrap();

        match cli.command {
            Commands::Chat(args) => {
                let error = args.validate_headless_flags().unwrap_err();
                assert_eq!(error.exit_code(), 2);
                assert!(error.to_string().contains(
                    "--include-partial-messages requires --print --output-format=stream-json"
                ));
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
