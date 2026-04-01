use crate::cli::ChatArgs;
use crate::commands::config::{
    default_paths, load_config_snapshot, write_last_session_id, ConfigPaths, ConfigSnapshot,
};
use crate::error::CliError;
use crate::runtime::{build_chat_runtime, ChatRuntimeConfig, SessionMode};
use crate::stdin::{merge_prompt_and_stdin, read_stdin_if_piped};

use ccc_agent::{
    session_store::{PersistedSession, SessionStore},
    SessionRunner,
};
use ccc_core::{claude_config_dir, SessionId};
use ccc_tui::AppConfig;
use tracing::warn;

#[derive(Debug)]
enum ChatLaunch {
    Interactive(AppConfig),
    Print(ChatRuntimeConfig),
}

pub async fn run(args: ChatArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()?;
    let paths = default_paths(cwd.clone());
    let snapshot = load_config_snapshot(&paths)?;
    let launch = prepare_chat_launch(
        &args,
        cwd,
        &paths,
        &snapshot,
        claude_config_dir().join("sessions"),
    )
    .await?;

    match launch {
        ChatLaunch::Interactive(app) => ccc_tui::run_app(app).await?,
        ChatLaunch::Print(runtime) => return run_print(args, runtime).await,
    }

    Ok(())
}

async fn prepare_chat_launch(
    args: &ChatArgs,
    cwd: std::path::PathBuf,
    paths: &ConfigPaths,
    snapshot: &ConfigSnapshot,
    session_store_root: std::path::PathBuf,
) -> Result<ChatLaunch, CliError> {
    let runtime = build_chat_runtime(args.clone(), snapshot, cwd.clone())?;

    if runtime.session_mode == SessionMode::Ephemeral {
        return Ok(ChatLaunch::Print(runtime));
    }

    Ok(ChatLaunch::Interactive(
        build_interactive_app_config(args, &runtime, paths, snapshot, session_store_root).await?,
    ))
}

async fn build_interactive_app_config(
    args: &ChatArgs,
    runtime: &ChatRuntimeConfig,
    paths: &ConfigPaths,
    snapshot: &ConfigSnapshot,
    session_store_root: std::path::PathBuf,
) -> Result<AppConfig, CliError> {
    let store = SessionStore::new(session_store_root);
    let session = resolve_interactive_session(args, runtime, snapshot, &paths.cwd, &store).await;
    write_last_session_id(paths, &runtime.project_key, &session.session_id)?;

    Ok(AppConfig {
        model: session.model.clone(),
        system_prompt: session.system_prompt.clone(),
        initial_messages: session.messages.clone(),
        session_id: Some(session.session_id),
        cwd: session.cwd,
        mcp_servers: runtime.mcp_servers.clone(),
        session_store: Some(store),
    })
}

async fn resolve_interactive_session(
    args: &ChatArgs,
    runtime: &ChatRuntimeConfig,
    snapshot: &ConfigSnapshot,
    cwd: &std::path::Path,
    store: &SessionStore,
) -> PersistedSession {
    if let Some(last_session_id) = snapshot.project.last_session_id.as_deref() {
        match store.load(&SessionId::new(last_session_id)).await {
            Ok(Some(mut session)) => {
                if let Some(model) = args.model.clone() {
                    session.model = model;
                }

                if let Some(system_prompt) = args.system_prompt.clone() {
                    session.system_prompt = Some(system_prompt);
                }

                return session;
            }
            Ok(None) => {
                warn!(
                    session_id = last_session_id,
                    "persisted session missing; starting a new interactive session"
                );
            }
            Err(error) => {
                warn!(
                    session_id = last_session_id,
                    error = %error,
                    "failed to load persisted session; starting a new interactive session"
                );
            }
        }
    }

    PersistedSession::fresh(
        cwd.display().to_string(),
        runtime.model.clone(),
        runtime.system_prompt.clone(),
        Vec::new(),
    )
}

async fn run_print(args: ChatArgs, runtime: ChatRuntimeConfig) -> Result<(), CliError> {
    let stdin = read_stdin_if_piped()?;
    let input = merge_prompt_and_stdin(args.prompt_text().as_deref(), stdin.as_deref())
        .ok_or_else(|| CliError::new("chat --print requires a prompt or piped stdin", 2))?;

    let mut runner = SessionRunner::new(runtime.model, runtime.system_prompt)?;
    let failures = runner.bootstrap_mcp_servers(&runtime.mcp_servers).await?;
    for (name, error) in failures {
        warn!(server = %name, error = %error, "failed to bootstrap MCP server");
    }
    let summary = runner.run_with_events(input, |_event| {}).await?;

    if !summary.assistant_text.is_empty() {
        println!("{}", summary.assistant_text);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ccc_agent::session_store::{PersistedSession, SessionStore};
    use ccc_core::{ContentBlock, GlobalConfig, Message, ProjectConfig, Role, SessionId};

    use crate::{
        cli::ChatArgs,
        commands::config::{load_config_snapshot, ConfigPaths},
        runtime::SessionMode,
    };

    use super::{prepare_chat_launch, ChatLaunch};

    fn config_paths(root: &std::path::Path) -> ConfigPaths {
        ConfigPaths {
            cwd: root.to_path_buf(),
            global_candidates: vec![root.join("settings.json")],
            project_settings_path: root.join(".claude/settings.json"),
            project_local_settings_path: root.join(".claude/settings.local.json"),
        }
    }

    fn assistant_message(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[tokio::test]
    async fn interactive_chat_uses_last_session_id_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let project_key = ccc_core::normalize_project_key(temp.path());
        let paths = config_paths(temp.path());
        let session_store = SessionStore::new(temp.path().join("sessions"));
        let session = PersistedSession::new(
            SessionId::new("sess-existing"),
            temp.path().display().to_string(),
            "claude-opus-4-6".into(),
            Some("saved prompt".into()),
            vec![assistant_message("hello again")],
        );

        session_store.save(&session).await.unwrap();
        fs::write(
            temp.path().join("settings.json"),
            serde_json::json!({
                "projects": {
                    project_key: {
                        "lastSessionId": "sess-existing"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_config_snapshot(&paths).unwrap();
        let launch = prepare_chat_launch(
            &ChatArgs {
                model: None,
                system_prompt: None,
                print: false,
                prompt: vec![],
            },
            temp.path().to_path_buf(),
            &paths,
            &snapshot,
            temp.path().join("sessions"),
        )
        .await
        .unwrap();

        match launch {
            ChatLaunch::Interactive(app) => {
                assert_eq!(
                    app.session_id.as_ref().map(|id| id.as_str()),
                    Some("sess-existing")
                );
                assert_eq!(app.model, "claude-opus-4-6");
                assert_eq!(app.system_prompt.as_deref(), Some("saved prompt"));
                assert_eq!(app.initial_messages, vec![assistant_message("hello again")]);
            }
            other => panic!("expected interactive launch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn interactive_chat_cli_flags_override_persisted_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let project_key = ccc_core::normalize_project_key(temp.path());
        let paths = config_paths(temp.path());
        let session_store = SessionStore::new(temp.path().join("sessions"));
        let session = PersistedSession::new(
            SessionId::new("sess-existing"),
            temp.path().display().to_string(),
            "claude-opus-4-6".into(),
            Some("saved prompt".into()),
            vec![assistant_message("hello again")],
        );

        session_store.save(&session).await.unwrap();
        fs::write(
            temp.path().join("settings.json"),
            serde_json::json!({
                "projects": {
                    project_key: {
                        "lastSessionId": "sess-existing"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_config_snapshot(&paths).unwrap();
        let launch = prepare_chat_launch(
            &ChatArgs {
                model: Some("claude-sonnet-4-5".into()),
                system_prompt: Some("override prompt".into()),
                print: false,
                prompt: vec![],
            },
            temp.path().to_path_buf(),
            &paths,
            &snapshot,
            temp.path().join("sessions"),
        )
        .await
        .unwrap();

        match launch {
            ChatLaunch::Interactive(app) => {
                assert_eq!(
                    app.session_id.as_ref().map(|id| id.as_str()),
                    Some("sess-existing")
                );
                assert_eq!(app.model, "claude-sonnet-4-5");
                assert_eq!(app.system_prompt.as_deref(), Some("override prompt"));
                assert_eq!(app.initial_messages, vec![assistant_message("hello again")]);
            }
            other => panic!("expected interactive launch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn interactive_chat_writes_back_last_session_id() {
        let temp = tempfile::tempdir().unwrap();
        let project_key = ccc_core::normalize_project_key(temp.path());
        let paths = config_paths(temp.path());

        fs::write(temp.path().join("settings.json"), "{}").unwrap();

        let snapshot = load_config_snapshot(&paths).unwrap();
        let launch = prepare_chat_launch(
            &ChatArgs {
                model: None,
                system_prompt: None,
                print: false,
                prompt: vec![],
            },
            temp.path().to_path_buf(),
            &paths,
            &snapshot,
            temp.path().join("sessions"),
        )
        .await
        .unwrap();

        let written: GlobalConfig =
            serde_json::from_str(&fs::read_to_string(temp.path().join("settings.json")).unwrap())
                .unwrap();

        let session_id = match launch {
            ChatLaunch::Interactive(app) => app.session_id.unwrap(),
            other => panic!("expected interactive launch, got {other:?}"),
        };

        assert_eq!(
            written
                .projects
                .get(&project_key)
                .and_then(|project| project.last_session_id.as_deref()),
            Some(session_id.as_str())
        );
    }

    #[tokio::test]
    async fn print_chat_does_not_load_or_write_last_session_id() {
        let temp = tempfile::tempdir().unwrap();
        let project_key = ccc_core::normalize_project_key(temp.path());
        let paths = config_paths(temp.path());
        let session_store = SessionStore::new(temp.path().join("sessions"));
        let session = PersistedSession::new(
            SessionId::new("sess-existing"),
            temp.path().display().to_string(),
            "claude-opus-4-6".into(),
            Some("saved prompt".into()),
            vec![assistant_message("hello again")],
        );

        session_store.save(&session).await.unwrap();
        let config_json = serde_json::json!({
            "projects": {
                project_key: ProjectConfig {
                    last_session_id: Some("sess-existing".into()),
                    ..ProjectConfig::default()
                }
            }
        })
        .to_string();
        fs::write(temp.path().join("settings.json"), &config_json).unwrap();

        let snapshot = load_config_snapshot(&paths).unwrap();
        let launch = prepare_chat_launch(
            &ChatArgs {
                model: None,
                system_prompt: None,
                print: true,
                prompt: vec!["hello".into()],
            },
            temp.path().to_path_buf(),
            &paths,
            &snapshot,
            temp.path().join("sessions"),
        )
        .await
        .unwrap();

        match launch {
            ChatLaunch::Print(runtime) => {
                assert_eq!(runtime.session_mode, SessionMode::Ephemeral);
            }
            other => panic!("expected print launch, got {other:?}"),
        }

        let after = fs::read_to_string(temp.path().join("settings.json")).unwrap();
        assert_eq!(after, config_json);
        assert_eq!(
            session_store
                .load(&SessionId::new("sess-existing"))
                .await
                .unwrap()
                .unwrap()
                .messages,
            vec![assistant_message("hello again")]
        );
    }
}
