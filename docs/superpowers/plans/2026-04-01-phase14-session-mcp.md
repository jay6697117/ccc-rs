# Phase 14 Session Persistence & MCP Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `ccc chat` 真正由配置驱动，支持交互式最近会话恢复，并在启动时按现有配置 bootstrap MCP servers，同时保持 `ccc chat --print` 默认新会话。

**Architecture:** `ccc-cli` 先把 CLI 参数、全局配置和项目配置装配成 `ChatRuntimeConfig`，再把交互与非交互分流到共享的 `ccc-agent` 执行层。`ccc-agent` 新增持久化 session store 和 MCP bootstrap 辅助层；`ccc-tui` 只消费 CLI 传入的初始 messages/session 元信息，不自行决定恢复逻辑。

**Tech Stack:** Rust, `serde`, `serde_json`, `tokio`, `uuid`, 现有 `ccc-cli` / `ccc-agent` / `ccc-tui` / `ccc-core`

---

## Chunk 1: Shared Runtime Paths and Config Assembly

### Task 1: 抽共享配置根目录与项目 key 帮助函数

**Files:**
- Create: `crates/ccc-core/src/paths.rs`
- Modify: `crates/ccc-core/src/lib.rs`
- Modify: `crates/ccc-cli/src/commands/config.rs`
- Modify: `crates/ccc-auth/src/storage.rs`
- Test: `crates/ccc-core/src/paths.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn claude_config_dir_prefers_env_var() {
    std::env::set_var("CLAUDE_CONFIG_DIR", "/tmp/claude-config");
    assert_eq!(claude_config_dir(), PathBuf::from("/tmp/claude-config"));
}

#[test]
fn normalize_project_key_canonicalizes_and_normalizes_separators() {
    let temp = tempfile::tempdir().unwrap();
    assert!(normalize_project_key(temp.path()).contains('/'));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-core paths::`
Expected: FAIL with missing module or missing functions

- [ ] **Step 3: Write minimal implementation**

```rust
pub fn claude_config_dir() -> PathBuf { /* env -> HOME/.claude */ }
pub fn normalize_project_key(path: &Path) -> String { /* canonicalize + slash normalize */ }
```

- [ ] **Step 4: Rewire existing callers**

Use `ccc_core::paths::claude_config_dir()` in:
- `ccc-auth/src/storage.rs`
- `ccc-cli/src/commands/config.rs`

Use `ccc_core::paths::normalize_project_key()` in:
- `ccc-cli/src/commands/config.rs`

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-core`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-core/src/paths.rs crates/ccc-core/src/lib.rs crates/ccc-cli/src/commands/config.rs crates/ccc-auth/src/storage.rs
git commit -m "refactor: share claude config path helpers"
```

### Task 2: 新增 `ChatRuntimeConfig` 装配层

**Files:**
- Create: `crates/ccc-cli/src/runtime.rs`
- Modify: `crates/ccc-cli/src/lib.rs`
- Modify: `crates/ccc-cli/src/commands/chat.rs`
- Test: `crates/ccc-cli/src/runtime.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn interactive_chat_uses_resume_last_mode() {
    let runtime = build_chat_runtime(/* chat args without --print */).unwrap();
    assert_eq!(runtime.session_mode, SessionMode::ResumeLast);
}

#[test]
fn print_chat_uses_ephemeral_mode() {
    let runtime = build_chat_runtime(/* chat args with --print */).unwrap();
    assert_eq!(runtime.session_mode, SessionMode::Ephemeral);
}

#[test]
fn cli_model_overrides_project_session_defaults() {
    let runtime = build_chat_runtime(/* saved session + --model */).unwrap();
    assert_eq!(runtime.model, "claude-opus-4-6");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli runtime::`
Expected: FAIL with missing `runtime` module or missing `SessionMode`

- [ ] **Step 3: Write minimal implementation**

```rust
pub enum SessionMode {
    ResumeLast,
    Ephemeral,
}

pub struct ChatRuntimeConfig {
    pub model: String,
    pub system_prompt: Option<String>,
    pub project_key: String,
    pub session_mode: SessionMode,
    pub mcp_servers: Vec<(String, McpServerConfig)>,
}
```

- [ ] **Step 4: Implement MCP selection precedence**

Inside `build_chat_runtime(...)`, filter `GlobalConfig.mcp_servers` with:
- `disabled_mcp_json_servers` first
- then `enabled_mcp_json_servers`
- then `enable_all_project_mcp_servers`

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-cli/src/runtime.rs crates/ccc-cli/src/lib.rs crates/ccc-cli/src/commands/chat.rs
git commit -m "feat: assemble chat runtime config"
```

## Chunk 2: Session Persistence in `ccc-agent`

### Task 3: 新增持久化 session store

**Files:**
- Create: `crates/ccc-agent/src/session_store.rs`
- Modify: `crates/ccc-agent/src/lib.rs`
- Modify: `crates/ccc-agent/src/session.rs`
- Modify: `crates/ccc-agent/Cargo.toml`
- Test: `crates/ccc-agent/src/session_store.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn saves_and_loads_session_roundtrip() {
    let store = SessionStore::new(temp.path().into());
    let session = PersistedSession::new(/* id, cwd, model, prompt, messages */);
    store.save(&session).await.unwrap();
    let loaded = store.load(&session.session_id).await.unwrap().unwrap();
    assert_eq!(loaded.messages, session.messages);
}

#[tokio::test]
async fn missing_session_returns_none() {
    let store = SessionStore::new(temp.path().into());
    assert!(store.load(&SessionId::new("missing")).await.unwrap().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-agent session_store::`
Expected: FAIL with missing module/types

- [ ] **Step 3: Add dependencies and minimal types**

Use `uuid` to generate new session ids:

```rust
let session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
```

Persist format:

```rust
pub struct PersistedSession {
    pub version: u32,
    pub session_id: SessionId,
    pub cwd: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
}
```

- [ ] **Step 4: Implement save/load**

Store transcript at:
- `<claude_config_dir>/sessions/<session_id>.json`

Behavior:
- create parent directories if missing
- pretty JSON write is acceptable
- on missing file: `Ok(None)`
- on parse failure: return typed error to caller

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-agent`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-agent/src/session_store.rs crates/ccc-agent/src/session.rs crates/ccc-agent/src/lib.rs crates/ccc-agent/Cargo.toml Cargo.toml Cargo.lock
git commit -m "feat: add persistent session store"
```

### Task 4: 让 `SessionRunner` 支持恢复已有消息

**Files:**
- Modify: `crates/ccc-agent/src/runner.rs`
- Modify: `crates/ccc-agent/src/session.rs`
- Test: `crates/ccc-agent/src/runner.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn latest_assistant_text_works_with_restored_history() {
    let runner = SessionRunner::from_messages(
        "claude-opus-4-6",
        Some("system".into()),
        vec![Message { /* assistant text */ }],
        Some(SessionId::new("sess-1")),
    ).unwrap();
    assert_eq!(latest_assistant_text(runner.messages()), "hello");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-agent runner::`
Expected: FAIL with missing constructor or missing session id support

- [ ] **Step 3: Write minimal implementation**

Add:

```rust
pub fn from_persisted_session(session: PersistedSession) -> Result<Self>
pub fn session_id(&self) -> Option<&SessionId>
pub fn snapshot(&self) -> PersistedSession
```

- [ ] **Step 4: Preserve existing behavior**

`SessionRunner::new(...)` must still create a fresh runner for non-persistent paths.

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-agent`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-agent/src/runner.rs crates/ccc-agent/src/session.rs
git commit -m "feat: restore session runner from persisted history"
```

## Chunk 3: MCP Bootstrap and CLI/TUI Wiring

### Task 5: 在 `ccc-agent` 增加批量 MCP bootstrap

**Files:**
- Modify: `crates/ccc-agent/src/lib.rs`
- Test: `crates/ccc-agent/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn bootstrap_mcp_servers_skips_empty_input() {
    let mut agent = Agent::new("claude-opus-4-6").unwrap();
    agent.bootstrap_mcp_servers(vec![]).await.unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-agent bootstrap_mcp`
Expected: FAIL with missing method

- [ ] **Step 3: Write minimal implementation**

```rust
pub async fn bootstrap_mcp_servers(
    &mut self,
    servers: &[(String, McpServerConfig)],
) -> Result<Vec<(String, anyhow::Error)>>
```

Rules:
- iterate enabled servers in order
- success: server remains registered
- failure: collect error and continue

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-agent`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-agent/src/lib.rs
git commit -m "feat: add batch mcp bootstrap"
```

### Task 6: 把交互 `chat` 改成恢复最近会话并写回 `last_session_id`

**Files:**
- Modify: `crates/ccc-cli/src/commands/chat.rs`
- Modify: `crates/ccc-cli/src/commands/config.rs`
- Modify: `crates/ccc-tui/src/lib.rs`
- Modify: `crates/ccc-tui/src/app.rs`
- Test: `crates/ccc-cli/src/commands/chat.rs`
- Test: `crates/ccc-tui/src/app.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn interactive_chat_uses_last_session_id_when_present() {
    let runtime = /* config with last_session_id + saved session */;
    let app = build_app_config_from_runtime(runtime).await.unwrap();
    assert_eq!(app.session_id.as_deref(), Some("..."));
    assert!(!app.initial_messages.is_empty());
}

#[tokio::test]
async fn interactive_chat_writes_back_last_session_id() {
    /* start fresh session, complete one turn, verify settings.json project entry updated */
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli commands::chat::`
Expected: FAIL because runtime write-back/resume is not implemented

- [ ] **Step 3: Extend TUI startup config**

Add to `AppConfig`:

```rust
pub initial_messages: Vec<Message>,
pub session_id: Option<SessionId>,
pub mcp_servers: Vec<(String, McpServerConfig)>,
```

`run_app(...)` should accept restored messages and bootstrap MCP before the first user turn.

- [ ] **Step 4: Implement config write-back**

Add a helper in `ccc-cli/src/commands/config.rs` (or nearby module) to:
- load global config
- update `projects[project_key].last_session_id`
- write pretty JSON back to the selected global config path

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-cli/src/commands/chat.rs crates/ccc-cli/src/commands/config.rs crates/ccc-tui/src/lib.rs crates/ccc-tui/src/app.rs
git commit -m "feat: resume interactive chat sessions from config"
```

### Task 7: 保持 `chat --print` 为完全 ephemeral

**Files:**
- Modify: `crates/ccc-cli/src/commands/chat.rs`
- Test: `crates/ccc-cli/src/commands/chat.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn print_chat_does_not_load_or_write_last_session_id() {
    /* seed config + session file, run print path, assert no load/write side effect */
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli commands::chat::print_chat_does_not_load_or_write_last_session_id`
Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

Keep `SessionMode::Ephemeral` for `--print` and guard all persistence/write-back behind interactive mode only.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-cli/src/commands/chat.rs
git commit -m "fix: keep print chat sessions ephemeral"
```

## Chunk 4: Final Verification and Docs

### Task 8: 补文档并完成全量验证

**Files:**
- Modify: `docs/plans/ARCHITECTURE.md`
- Test: workspace commands only

- [ ] **Step 1: Update architecture doc**

Document:
- Phase 14 status
- shared path helper
- session persistence + MCP bootstrap runtime flow

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo test -p ccc-core
cargo test -p ccc-agent
cargo test -p ccc-cli
cargo test -p ccc-tui
```

Expected: PASS

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo test
```

Expected: PASS

- [ ] **Step 4: Run smoke checks**

Run:

```bash
cargo run -p ccc-cli -- config show
cargo run -p ccc-cli -- --help
```

Expected: both exit `0`

- [ ] **Step 5: Commit**

```bash
git add docs/plans/ARCHITECTURE.md
git commit -m "docs: record phase 14 runtime architecture"
```

## Acceptance Checklist

- [ ] `ccc chat` 能自动恢复最近会话
- [ ] `ccc chat --print` 默认不读写持久化 session
- [ ] MCP bootstrap 遵守 `disabled > enabled > enable_all` precedence
- [ ] 新会话会生成 UUID v4 `SessionId`
- [ ] `ProjectConfig.last_session_id` 会被正确写回
- [ ] `cargo test` 通过
