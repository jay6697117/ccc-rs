use std::{future::Future, io::Write, pin::Pin};

use crate::cli::{ChatArgs, OutputFormat};
use crate::commands::config::{
    default_paths, load_config_snapshot, write_last_session_id, ConfigPaths, ConfigSnapshot,
};
use crate::error::{CliError, CliExit};
use crate::output::{ProtocolWriter, ResultContext, ResultEnvelope, SystemInitEvent};
use crate::runtime::{build_chat_runtime, ChatRuntimeConfig, SessionMode};
use crate::stdin::{merge_prompt_and_stdin, read_stdin_if_piped};

use ccc_agent::{
    session_store::{PersistedSession, SessionStore},
    McpBootstrapReport, RunSummary, SessionRunner,
};
use ccc_api::types::StreamEvent;
use ccc_core::{McpBootstrapPlan, claude_config_dir, SessionId};
use ccc_tui::AppConfig;
use tracing::warn;

#[derive(Debug)]
enum ChatLaunch {
    Interactive(AppConfig),
    Print(ChatRuntimeConfig),
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug)]
struct HeadlessRunOutput {
    summary: RunSummary,
    stream_events: Vec<StreamEvent>,
}

trait HeadlessChatBackend {
    fn session_id(&self) -> &SessionId;
    fn cwd(&self) -> &str;
    fn model(&self) -> &str;
    fn bootstrap_mcp_plan<'a>(
        &'a mut self,
        plan: &'a McpBootstrapPlan,
    ) -> BoxFuture<'a, Result<McpBootstrapReport, CliError>>;
    fn run<'a>(&'a mut self, input: String) -> BoxFuture<'a, Result<HeadlessRunOutput, CliError>>;
}

struct RunnerBackend {
    runner: SessionRunner,
}

impl RunnerBackend {
    fn new(runtime: &ChatRuntimeConfig) -> Result<Self, CliError> {
        Ok(Self {
            runner: SessionRunner::new(runtime.model.clone(), runtime.system_prompt.clone())?,
        })
    }
}

impl HeadlessChatBackend for RunnerBackend {
    fn session_id(&self) -> &SessionId {
        self.runner
            .session_id()
            .expect("session runner should always have a session id")
    }

    fn cwd(&self) -> &str {
        self.runner.cwd()
    }

    fn model(&self) -> &str {
        self.runner.model()
    }

    fn bootstrap_mcp_plan<'a>(
        &'a mut self,
        plan: &'a McpBootstrapPlan,
    ) -> BoxFuture<'a, Result<McpBootstrapReport, CliError>> {
        Box::pin(async move {
            self.runner
                .bootstrap_mcp_plan(plan)
                .await
                .map_err(Into::into)
        })
    }

    fn run<'a>(&'a mut self, input: String) -> BoxFuture<'a, Result<HeadlessRunOutput, CliError>> {
        Box::pin(async move {
            let mut stream_events = Vec::new();
            let summary = self
                .runner
                .run_with_events(input, |event| stream_events.push(event))
                .await
                .map_err(CliError::from)?;

            Ok(HeadlessRunOutput {
                summary,
                stream_events,
            })
        })
    }
}

pub async fn run(args: ChatArgs) -> CliExit {
    match run_inner(args).await {
        Ok(exit) => exit,
        Err(error) => error.into(),
    }
}

async fn run_inner(args: ChatArgs) -> Result<CliExit, CliError> {
    args.validate_headless_flags()?;

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
        ChatLaunch::Interactive(app) => {
            ccc_tui::run_app(app).await?;
            Ok(CliExit::success())
        }
        ChatLaunch::Print(runtime) => run_print(&args, runtime).await,
    }
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
        mcp_bootstrap: runtime.mcp_bootstrap.clone(),
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

async fn run_print(args: &ChatArgs, runtime: ChatRuntimeConfig) -> Result<CliExit, CliError> {
    let stdin = read_stdin_if_piped()?;
    let input = merge_prompt_and_stdin(args.prompt_text().as_deref(), stdin.as_deref())
        .ok_or_else(|| CliError::new("chat --print requires a prompt or piped stdin", 2))?;
    let mut backend = RunnerBackend::new(&runtime)?;
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();

    run_headless_with_backend(
        args,
        &runtime,
        input,
        &mut backend,
        &mut stdout,
        &mut stderr,
    )
    .await
}

async fn run_headless_with_backend<B, Stdout, Stderr>(
    args: &ChatArgs,
    runtime: &ChatRuntimeConfig,
    input: String,
    backend: &mut B,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> Result<CliExit, CliError>
where
    B: HeadlessChatBackend,
    Stdout: Write,
    Stderr: Write,
{
    let mut protocol = ProtocolWriter::new(args.output_format, stdout, stderr);
    let bootstrap_report = backend.bootstrap_mcp_plan(&runtime.mcp_bootstrap).await?;
    let warnings = bootstrap_report.warnings.clone();

    if args.output_format == OutputFormat::StreamJson {
        protocol.emit_init(&SystemInitEvent::new(
            backend.session_id().clone(),
            backend.cwd(),
            backend.model(),
            args.output_format,
            bootstrap_report.snapshots.clone(),
        ))?;
    }

    for warning in &warnings {
        protocol.emit_warning(backend.session_id(), warning)?;
    }

    match backend.run(input).await {
        Ok(output) => {
            if args.output_format == OutputFormat::StreamJson {
                if args.include_partial_messages {
                    for event in &output.stream_events {
                        protocol.emit_stream_event(backend.session_id(), event)?;
                    }
                }

                for message in &output.summary.assistant_messages {
                    protocol.emit_assistant(backend.session_id(), message)?;
                }
            }

            let mut result_warnings = warnings;
            result_warnings.extend(output.summary.warnings.clone());
            let result = ResultEnvelope::success(
                summary_context(&output.summary),
                output.summary.assistant_text.clone(),
                result_warnings,
            );
            protocol.emit_result(&result)?;
            Ok(CliExit::reported(0))
        }
        Err(error) => {
            let result = ResultEnvelope::error(
                ResultContext::failed(backend.session_id().clone(), backend.model()),
                warnings,
                vec![error.to_string()],
            );
            protocol.emit_result(&result)?;
            Ok(CliExit::reported(1))
        }
    }
}

fn summary_context(summary: &RunSummary) -> ResultContext {
    ResultContext {
        session_id: summary.session_id.clone(),
        model: summary.model.clone(),
        duration_ms: summary.duration_ms,
        num_turns: summary.num_turns,
        stop_reason: summary.stop_reason.clone(),
        usage: summary.usage.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, io::Cursor, pin::Pin};

    use std::fs;

    use ccc_agent::session_store::{PersistedSession, SessionStore};
    use ccc_agent::{McpBootstrapReport, RunSummary};
    use ccc_api::types::{MessageDeltaPayload, StreamEvent, Usage, UsageDelta};
    use ccc_core::{
        ContentBlock, GlobalConfig, McpBootstrapPlan, McpConnectionSnapshot, McpConnectionStatus,
        McpSourceScope, Message, ProjectConfig, Role, SessionId,
    };

    use crate::{
        cli::{ChatArgs, OutputFormat},
        commands::config::{load_config_snapshot, ConfigPaths},
        error::CliError,
        runtime::{ChatRuntimeConfig, SessionMode},
    };

    use super::{
        prepare_chat_launch, run_headless_with_backend, ChatLaunch, HeadlessChatBackend,
        HeadlessRunOutput,
    };

    fn config_paths(root: &std::path::Path) -> ConfigPaths {
        ConfigPaths {
            cwd: root.to_path_buf(),
            global_candidates: vec![root.join("settings.json")],
            project_settings_path: root.join(".claude/settings.json"),
            project_local_settings_path: root.join(".claude/settings.local.json"),
            managed_root: root.join("managed"),
        }
    }

    fn assistant_message(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    fn sample_summary() -> RunSummary {
        RunSummary {
            session_id: SessionId::new("sess-print"),
            assistant_text: "final answer".into(),
            assistant_messages: vec![assistant_message("final answer")],
            model: "claude-opus-4-6".into(),
            duration_ms: 42,
            num_turns: 1,
            stop_reason: Some("end_turn".into()),
            usage: Usage::default(),
            warnings: Vec::new(),
        }
    }

    fn sample_bootstrap_report(
        status: McpConnectionStatus,
        warnings: Vec<String>,
    ) -> McpBootstrapReport {
        McpBootstrapReport {
            snapshots: vec![McpConnectionSnapshot {
                name: "ok".into(),
                transport: ccc_core::McpTransportKind::Stdio,
                status,
                reconnect_attempt: None,
                max_reconnect_attempts: None,
                error: None,
                source_scope: McpSourceScope::Global,
            }],
            warnings,
        }
    }

    fn runtime(
        output_format: OutputFormat,
        include_partial_messages: bool,
    ) -> (ChatArgs, ChatRuntimeConfig) {
        (
            ChatArgs {
                model: None,
                system_prompt: None,
                print: true,
                output_format,
                include_partial_messages,
                prompt: vec!["hello".into()],
            },
            ChatRuntimeConfig {
                model: "claude-opus-4-6".into(),
                system_prompt: None,
                project_key: "project".into(),
                session_mode: SessionMode::Ephemeral,
                mcp_servers: vec![],
                mcp_bootstrap: McpBootstrapPlan::default(),
            },
        )
    }

    struct FakeBackend {
        session_id: SessionId,
        cwd: String,
        model: String,
        bootstrap_report: McpBootstrapReport,
        run_result: Option<Result<HeadlessRunOutput, CliError>>,
    }

    impl HeadlessChatBackend for FakeBackend {
        fn session_id(&self) -> &SessionId {
            &self.session_id
        }

        fn cwd(&self) -> &str {
            &self.cwd
        }

        fn model(&self) -> &str {
            &self.model
        }

        fn bootstrap_mcp_plan<'a>(
            &'a mut self,
            _plan: &'a McpBootstrapPlan,
        ) -> Pin<Box<dyn Future<Output = Result<McpBootstrapReport, CliError>> + Send + 'a>>
        {
            let report = self.bootstrap_report.clone();
            Box::pin(async move { Ok(report) })
        }

        fn run<'a>(
            &'a mut self,
            _input: String,
        ) -> Pin<Box<dyn Future<Output = Result<HeadlessRunOutput, CliError>> + Send + 'a>>
        {
            let result = self
                .run_result
                .take()
                .expect("fake backend run result should be present");
            Box::pin(async move { result })
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
                output_format: OutputFormat::Text,
                include_partial_messages: false,
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
                output_format: OutputFormat::Text,
                include_partial_messages: false,
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
                output_format: OutputFormat::Text,
                include_partial_messages: false,
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
                output_format: OutputFormat::Text,
                include_partial_messages: false,
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

    #[tokio::test]
    async fn text_print_success_writes_only_stdout() {
        let (args, runtime) = runtime(OutputFormat::Text, false);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(McpConnectionStatus::Connected, Vec::new()),
            run_result: Some(Ok(HeadlessRunOutput {
                summary: sample_summary(),
                stream_events: vec![StreamEvent::MessageDelta {
                    delta: MessageDeltaPayload {
                        stop_reason: Some("end_turn".into()),
                        stop_sequence: None,
                    },
                    usage: Some(UsageDelta {
                        output_tokens: Some(4),
                    }),
                }],
            })),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        assert_eq!(exit.exit_code(), 0);
        assert_eq!(
            String::from_utf8(stdout.into_inner()).unwrap(),
            "final answer\n"
        );
        assert!(String::from_utf8(stderr.into_inner()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn text_print_failure_writes_only_stderr() {
        let (args, runtime) = runtime(OutputFormat::Text, false);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(McpConnectionStatus::Connected, Vec::new()),
            run_result: Some(Err(CliError::new("boom", 1))),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        assert_eq!(exit.exit_code(), 1);
        assert!(String::from_utf8(stdout.into_inner()).unwrap().is_empty());
        assert_eq!(String::from_utf8(stderr.into_inner()).unwrap(), "boom\n");
    }

    #[tokio::test]
    async fn json_print_success_writes_single_result_object() {
        let (args, runtime) = runtime(OutputFormat::Json, false);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(
                McpConnectionStatus::Failed,
                vec!["failed to bootstrap MCP server: ok: boom".into()],
            ),
            run_result: Some(Ok(HeadlessRunOutput {
                summary: sample_summary(),
                stream_events: Vec::new(),
            })),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        let output = String::from_utf8(stdout.into_inner()).unwrap();
        let lines: Vec<_> = output.lines().collect();
        let value: serde_json::Value = serde_json::from_str(lines[0]).unwrap();

        assert_eq!(exit.exit_code(), 0);
        assert_eq!(lines.len(), 1);
        assert_eq!(value["type"], "result");
        assert_eq!(value["subtype"], "success");
        assert_eq!(value["result"], "final answer");
        assert_eq!(
            value["warnings"][0],
            "failed to bootstrap MCP server: ok: boom"
        );
        assert!(String::from_utf8(stderr.into_inner()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn json_print_failure_writes_single_error_result_object() {
        let (args, runtime) = runtime(OutputFormat::Json, false);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(McpConnectionStatus::Connected, Vec::new()),
            run_result: Some(Err(CliError::new("boom", 1))),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        let output = String::from_utf8(stdout.into_inner()).unwrap();
        let lines: Vec<_> = output.lines().collect();
        let value: serde_json::Value = serde_json::from_str(lines[0]).unwrap();

        assert_eq!(exit.exit_code(), 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(value["type"], "result");
        assert_eq!(value["subtype"], "error_during_execution");
        assert_eq!(value["errors"][0], "boom");
        assert!(String::from_utf8(stderr.into_inner()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn stream_json_emits_init_stream_event_assistant_and_result() {
        let (args, runtime) = runtime(OutputFormat::StreamJson, true);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(
                McpConnectionStatus::Failed,
                vec!["failed to bootstrap MCP server: ok: boom".into()],
            ),
            run_result: Some(Ok(HeadlessRunOutput {
                summary: sample_summary(),
                stream_events: vec![StreamEvent::MessageDelta {
                    delta: MessageDeltaPayload {
                        stop_reason: Some("end_turn".into()),
                        stop_sequence: None,
                    },
                    usage: Some(UsageDelta {
                        output_tokens: Some(4),
                    }),
                }],
            })),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        let output = String::from_utf8(stdout.into_inner()).unwrap();
        let lines: Vec<_> = output.lines().collect();

        assert_eq!(exit.exit_code(), 0);
        assert_eq!(lines.len(), 5);
        assert!(lines[0].contains("\"type\":\"system\""));
        assert!(lines[0].contains("\"subtype\":\"init\""));
        assert!(lines[1].contains("\"subtype\":\"warning\""));
        assert!(lines[2].contains("\"type\":\"stream_event\""));
        assert!(lines[3].contains("\"type\":\"assistant\""));
        assert!(lines[4].contains("\"type\":\"result\""));
        assert!(String::from_utf8(stderr.into_inner()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn stream_json_omits_partial_messages_when_flag_disabled() {
        let (args, runtime) = runtime(OutputFormat::StreamJson, false);
        let mut stdout = Cursor::new(Vec::new());
        let mut stderr = Cursor::new(Vec::new());
        let mut backend = FakeBackend {
            session_id: SessionId::new("sess-print"),
            cwd: "/tmp/project".into(),
            model: "claude-opus-4-6".into(),
            bootstrap_report: sample_bootstrap_report(McpConnectionStatus::Connected, Vec::new()),
            run_result: Some(Ok(HeadlessRunOutput {
                summary: sample_summary(),
                stream_events: vec![StreamEvent::MessageDelta {
                    delta: MessageDeltaPayload {
                        stop_reason: Some("end_turn".into()),
                        stop_sequence: None,
                    },
                    usage: Some(UsageDelta {
                        output_tokens: Some(4),
                    }),
                }],
            })),
        };

        let exit = run_headless_with_backend(
            &args,
            &runtime,
            "hello".into(),
            &mut backend,
            &mut stdout,
            &mut stderr,
        )
        .await
        .unwrap();

        let output = String::from_utf8(stdout.into_inner()).unwrap();
        let lines: Vec<_> = output.lines().collect();

        assert_eq!(exit.exit_code(), 0);
        assert_eq!(lines.len(), 3);
        assert!(lines
            .iter()
            .all(|line| !line.contains("\"type\":\"stream_event\"")));
        assert!(String::from_utf8(stderr.into_inner()).unwrap().is_empty());
    }
}
