# Phase 16 Runtime Expansion Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 一次性把 `16A/16B/16C` 的 Rust 运行时基础设施落到代码里：统一 MCP config union、policy-aware bootstrap plan、managed/enterprise 注入，以及 remote transport/status lifecycle。

**Architecture:** 先在 `ccc-core` 定义统一的 MCP schema 与 plan/status 类型；再在 `ccc-cli` 重写 runtime 装配层，把 Phase 14 的简单 `Vec<(String, McpServerConfig)>` 提升为 `McpBootstrapPlan`；接着在 `ccc-agent` / `ccc-mcp` 增加连接注册表与 transport adapter，让交互 `chat` 和 Phase 15 headless 协议都消费同一份 plan 和 status vocabulary。企业 / managed 设置在 `16C` 内先落成静态快照与文件/缓存层，plugin 相关能力只实现 provider snapshot/gating，不发明完整 marketplace/installer 系统。

**Tech Stack:** Rust, `serde`, `serde_json`, `tokio`, `reqwest`, `tokio-tungstenite`, `uuid`, existing `ccc-core` / `ccc-cli` / `ccc-agent` / `ccc-mcp` / `ccc-tui`

---

## Chunk 1: Shared MCP Schema and Planning Types

### Task 1: Replace legacy `McpServerConfig` with tagged union and shared enums

**Files:**
- Modify: `crates/ccc-core/src/config.rs`
- Modify: `crates/ccc-core/src/lib.rs`
- Test: `crates/ccc-core/src/config.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn legacy_stdio_mcp_server_config_deserializes_without_type() {
    let json = r#"{"command":"npx","args":["server"],"env":{"A":"1"}}"#;
    let config: McpServerConfig = serde_json::from_str(json).unwrap();
    assert!(matches!(config, McpServerConfig::Stdio { .. }));
}

#[test]
fn remote_mcp_server_config_roundtrips_with_explicit_type() {
    let config = McpServerConfig::Sse {
        url: "https://example.com/sse".into(),
        headers: HashMap::new(),
        headers_helper: None,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"type\":\"sse\""));
    let back: McpServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, back);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-core config::`
Expected: FAIL because `McpServerConfig` is still a flat struct

- [ ] **Step 3: Write minimal implementation**

Add in `ccc-core`:

```rust
pub enum McpServerConfig { Stdio { .. }, Sse { .. }, Http { .. }, Ws { .. }, Sdk { .. }, ClaudeAiProxy { .. } }
pub enum McpTransportKind { Stdio, Sse, Http, Ws, Sdk, ClaudeAiProxy }
pub enum McpSourceScope { Global, Project, Local, BuiltinPlugin, Plugin, Managed, Enterprise, Dynamic, ClaudeAi }
pub enum McpConnectionStatus { Pending, Connected, Failed, NeedsAuth, Disabled }
```

- [ ] **Step 4: Preserve config compatibility**

Keep:
- legacy stdio JSON without `type`
- `GlobalConfig.mcp_servers`

Do not yet remove:
- `enabled_mcp_json_servers`
- `disabled_mcp_json_servers`

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-core`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-core/src/config.rs crates/ccc-core/src/lib.rs
git commit -m "feat: add shared mcp config union types"
```

### Task 2: Add resolved-server, decision, and bootstrap-plan types

**Files:**
- Modify: `crates/ccc-core/src/config.rs`
- Test: `crates/ccc-core/src/config.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn bootstrap_plan_serializes_with_planned_and_blocked_servers() {
    let plan = McpBootstrapPlan {
        planned: vec![],
        blocked: vec![],
        warnings: vec![],
    };
    let json = serde_json::to_string(&plan).unwrap();
    assert!(json.contains("planned"));
    assert!(json.contains("blocked"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-core bootstrap_plan`
Expected: FAIL because `McpBootstrapPlan` does not exist

- [ ] **Step 3: Write minimal implementation**

Add:

```rust
pub struct ResolvedMcpServer { .. }
pub enum McpPolicyDecisionKind { .. }
pub struct McpPolicyDecision { .. }
pub struct PlannedMcpServer { .. }
pub struct BlockedMcpServer { .. }
pub struct McpBootstrapPlan { .. }
pub struct McpConnectionSnapshot { .. }
```

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-core/src/config.rs
git commit -m "feat: add mcp bootstrap planning types"
```

## Chunk 2: CLI Runtime Selector and Policy Pipeline (16A)

### Task 3: Replace simple MCP vector selection with `McpBootstrapPlan`

**Files:**
- Modify: `crates/ccc-cli/src/runtime.rs`
- Modify: `crates/ccc-cli/src/commands/chat.rs`
- Test: `crates/ccc-cli/src/runtime.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn build_chat_runtime_returns_bootstrap_plan() {
    let runtime = build_chat_runtime(args, &snapshot, cwd).unwrap();
    assert!(runtime.mcp_bootstrap.planned.is_empty());
    assert!(runtime.mcp_bootstrap.blocked.is_empty());
}

#[test]
fn disabled_server_becomes_blocked_decision() {
    let runtime = build_chat_runtime(args, &snapshot_with_disabled_server(), cwd).unwrap();
    assert_eq!(runtime.mcp_bootstrap.blocked.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli runtime::`
Expected: FAIL because `ChatRuntimeConfig` still exposes `mcp_servers`

- [ ] **Step 3: Write minimal implementation**

Change:

```rust
pub struct ChatRuntimeConfig {
    ...
    pub mcp_bootstrap: McpBootstrapPlan,
}
```

Add a selector pipeline with stable phases:
- discover manual sources
- merge
- policy gate
- enable/disable gate
- build plan

- [ ] **Step 4: Preserve Phase 14 behavior where still applicable**

No regression when:
- no policy is present
- only global servers exist
- `disabled > enabled > enable_all`

- [ ] **Step 5: Run tests to verify it passes**

Run: `cargo test -p ccc-cli runtime::`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ccc-cli/src/runtime.rs crates/ccc-cli/src/commands/chat.rs
git commit -m "feat: build policy-aware mcp bootstrap plans"
```

### Task 4: Add canonical project enable/disable fields and legacy normalization

**Files:**
- Modify: `crates/ccc-core/src/config.rs`
- Modify: `crates/ccc-cli/src/runtime.rs`
- Modify: `crates/ccc-cli/src/commands/config.rs`
- Test: `crates/ccc-core/src/config.rs`
- Test: `crates/ccc-cli/src/runtime.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn canonical_enabled_disabled_fields_roundtrip() {
    let json = r#"{"enabledMcpServers":["a"],"disabledMcpServers":["b"]}"#;
    let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.enabled_mcp_servers, vec!["a"]);
    assert_eq!(cfg.disabled_mcp_servers, vec!["b"]);
}

#[test]
fn legacy_mcp_json_fields_are_normalized_into_selector_sets() {
    let runtime = build_chat_runtime(args, &legacy_snapshot(), cwd).unwrap();
    assert_eq!(runtime.mcp_bootstrap.blocked.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-core config::`
Expected: FAIL because canonical fields do not exist

- [ ] **Step 3: Write minimal implementation**

Add to `ProjectConfig`:
- `enabled_mcp_servers: Vec<String>`
- `disabled_mcp_servers: Vec<String>`

Normalize selector input by:
- preferring canonical fields when non-empty
- otherwise falling back to legacy `_mcp_json_` fields

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-core && cargo test -p ccc-cli runtime::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-core/src/config.rs crates/ccc-cli/src/runtime.rs crates/ccc-cli/src/commands/config.rs
git commit -m "feat: normalize canonical mcp enable disable fields"
```

### Task 5: Implement policy entries and allow/deny matching

**Files:**
- Modify: `crates/ccc-core/src/config.rs`
- Modify: `crates/ccc-cli/src/runtime.rs`
- Test: `crates/ccc-cli/src/runtime.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn denylist_takes_precedence_over_allowlist() {
    let runtime = build_chat_runtime(args, &snapshot_with_conflicting_policy(), cwd).unwrap();
    assert_eq!(runtime.mcp_bootstrap.blocked[0].decision.kind, McpPolicyDecisionKind::BlockedByDenylist);
}

#[test]
fn remote_server_matches_url_based_allowlist() {
    let runtime = build_chat_runtime(args, &snapshot_with_remote_server(), cwd).unwrap();
    assert_eq!(runtime.mcp_bootstrap.planned.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli runtime::`
Expected: FAIL because policy entries and matching logic do not exist

- [ ] **Step 3: Write minimal implementation**

Add managed-policy-facing entry types:

```rust
pub enum McpPolicyEntry {
    ServerName { server_name: String },
    ServerCommand { server_command: Vec<String> },
    ServerUrl { server_url: String },
}
```

Implement:
- command-array exact match for `stdio`
- wildcard URL pattern match for remote
- denylist absolute precedence
- empty allowlist means block all

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli runtime::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-core/src/config.rs crates/ccc-cli/src/runtime.rs
git commit -m "feat: add mcp allow deny policy matching"
```

## Chunk 3: Managed / Enterprise Injection (16C)

### Task 6: Add managed settings snapshot types and config parsing

**Files:**
- Modify: `crates/ccc-core/src/config.rs`
- Create: `crates/ccc-cli/src/managed.rs`
- Test: `crates/ccc-cli/src/managed.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn managed_file_settings_merge_dropins_in_sorted_order() {
    let snapshot = load_managed_settings(temp.path()).unwrap();
    assert_eq!(snapshot.merged_settings["channelsEnabled"], true);
}

#[test]
fn missing_managed_settings_returns_missing_snapshot() {
    let snapshot = load_managed_settings(temp.path()).unwrap();
    assert!(matches!(snapshot.freshness, ManagedSettingsFreshness::Missing));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli managed::`
Expected: FAIL because module and snapshot types do not exist

- [ ] **Step 3: Write minimal implementation**

Implement file-based:
- `managed-settings.json`
- `managed-settings.d/*.json`
- merged JSON payload
- warnings collection

Do not yet fetch network data; represent remote section as optional cache metadata.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli managed::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-core/src/config.rs crates/ccc-cli/src/managed.rs
git commit -m "feat: load managed settings snapshots"
```

### Task 7: Inject managed policy and enterprise-exclusive MCP into runtime assembly

**Files:**
- Modify: `crates/ccc-cli/src/runtime.rs`
- Modify: `crates/ccc-cli/src/commands/config.rs`
- Test: `crates/ccc-cli/src/runtime.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn enterprise_exclusive_mcp_replaces_manual_sources() {
    let runtime = build_chat_runtime(args, &snapshot_with_enterprise_mcp(), cwd).unwrap();
    assert!(runtime.mcp_bootstrap.planned.iter().all(|s| s.server.source_scope == McpSourceScope::Enterprise));
}

#[test]
fn allow_managed_mcp_servers_only_changes_allowlist_source() {
    let runtime = build_chat_runtime(args, &snapshot_with_managed_allow_only(), cwd).unwrap();
    assert_eq!(runtime.mcp_bootstrap.planned.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli runtime::`
Expected: FAIL because managed injection and enterprise replacement do not exist

- [ ] **Step 3: Write minimal implementation**

Extend runtime assembly to:
- read `ManagedSettingsSnapshot`
- read `managed-mcp.json` if present
- apply enterprise-exclusive source replacement before selector
- apply managed allow/deny/plugin-only settings inside selector

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli runtime::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-cli/src/runtime.rs crates/ccc-cli/src/commands/config.rs
git commit -m "feat: inject managed enterprise mcp policy"
```

### Task 8: Add plugin-source snapshot and marketplace/channel gating

**Files:**
- Create: `crates/ccc-cli/src/plugins.rs`
- Modify: `crates/ccc-cli/src/runtime.rs`
- Test: `crates/ccc-cli/src/runtime.rs`
- Test: `crates/ccc-cli/src/plugins.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn plugin_only_policy_skips_manual_sources_but_keeps_plugin_sources() {
    let runtime = build_chat_runtime(args, &snapshot_with_manual_and_plugin_sources(), cwd).unwrap();
    assert!(runtime.mcp_bootstrap.planned.iter().all(|s| matches!(s.server.source_scope, McpSourceScope::Plugin | McpSourceScope::BuiltinPlugin)));
}

#[test]
fn blocked_marketplace_plugin_is_removed_before_selector() {
    let sources = filter_plugin_sources(plugin_snapshot(), managed_policy()).unwrap();
    assert!(sources.iter().all(|source| source.plugin_source.as_deref() != Some("blocked@market")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli plugins:: runtime::`
Expected: FAIL because plugin provider snapshot and gating do not exist

- [ ] **Step 3: Write minimal implementation**

Add a plugin provider seam:

```rust
pub struct PluginMcpSource { ... }
pub trait PluginMcpSourceLoader { ... }
```

Default runtime behavior:
- built-in providers can be hard-coded or injected by tests
- installed plugin providers come from the loader
- when no loader/provider exists, return empty list

This is intentionally **not** a marketplace installer.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-cli/src/plugins.rs crates/ccc-cli/src/runtime.rs
git commit -m "feat: gate plugin mcp providers with managed policy"
```

## Chunk 4: Agent Registry and Headless/TUI Surface

### Task 9: Add agent-side MCP connection registry

**Files:**
- Modify: `crates/ccc-agent/src/lib.rs`
- Modify: `crates/ccc-agent/src/runner.rs`
- Test: `crates/ccc-agent/src/runner.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn run_summary_carries_connection_snapshots() {
    let summary = build_run_summary(...);
    assert!(summary.mcp_connections.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-agent runner::`
Expected: FAIL because run summary has no MCP connection snapshots

- [ ] **Step 3: Write minimal implementation**

Add:
- `McpConnectionRegistry`
- per-server snapshot storage
- `RunSummary.mcp_connections`

Make `SessionRunner` consume `McpBootstrapPlan` instead of raw tuple vector.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-agent`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-agent/src/lib.rs crates/ccc-agent/src/runner.rs
git commit -m "feat: track mcp connection registry state"
```

### Task 10: Surface MCP planning/connection statuses into headless output and TUI config

**Files:**
- Modify: `crates/ccc-cli/src/commands/chat.rs`
- Modify: `crates/ccc-cli/src/output.rs`
- Modify: `crates/ccc-tui/src/lib.rs`
- Modify: `crates/ccc-tui/src/app.rs`
- Test: `crates/ccc-cli/src/commands/chat.rs`
- Test: `crates/ccc-tui/src/app.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn stream_json_init_emits_pending_connected_failed_disabled_statuses() {
    let output = run_headless_with_fake_backend(...);
    assert!(output.stdout.contains("\"status\":\"pending\""));
}

#[test]
fn tui_app_receives_bootstrap_plan_and_initial_statuses() {
    let app = App::new(AppConfig { ... });
    assert_eq!(app.mcp_statuses().len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli commands::chat::`
Expected: FAIL because headless output still only knows `connected|failed`

- [ ] **Step 3: Write minimal implementation**

Update output protocol types to consume shared status vocabulary:
- `pending`
- `connected`
- `failed`
- `needs-auth`
- `disabled`

Feed `system/init` from `McpBootstrapPlan` + current registry snapshot.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli && cargo test -p ccc-tui`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-cli/src/commands/chat.rs crates/ccc-cli/src/output.rs crates/ccc-tui/src/lib.rs crates/ccc-tui/src/app.rs
git commit -m "feat: surface mcp bootstrap and status snapshots"
```

## Chunk 5: Remote Transport Runtime (16B)

### Task 11: Expand `ccc-mcp` transport config and adapter seams

**Files:**
- Modify: `crates/ccc-mcp/src/types.rs`
- Modify: `crates/ccc-mcp/src/client.rs`
- Create: `crates/ccc-mcp/src/transport.rs`
- Test: `crates/ccc-mcp/src/types.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn mcp_transport_config_supports_http_ws_sdk_and_proxy() {
    let http: McpServerConfig = serde_json::from_str(r#"{"type":"http","url":"https://example.com"}"#).unwrap();
    let ws: McpServerConfig = serde_json::from_str(r#"{"type":"ws","url":"wss://example.com"}"#).unwrap();
    assert!(matches!(http, McpServerConfig::Http { .. }));
    assert!(matches!(ws, McpServerConfig::Ws { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-mcp`
Expected: FAIL because `ccc-mcp` only knows `Stdio` and `Sse`

- [ ] **Step 3: Write minimal implementation**

Unify `ccc-mcp` config with `ccc-core` transport kinds through adapters, not duplicated enums.

Introduce:
- `McpTransportClient`
- `TransportConnector`

Keep `stdio` working first; make remote connectors compile with explicit `todo!`-free placeholder implementations that return typed errors where the actual protocol is not yet available.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-mcp`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-mcp/src/types.rs crates/ccc-mcp/src/client.rs crates/ccc-mcp/src/transport.rs
git commit -m "feat: add mcp transport adapter framework"
```

### Task 12: Implement remote connector status transitions

**Files:**
- Modify: `crates/ccc-mcp/src/client.rs`
- Modify: `crates/ccc-agent/src/lib.rs`
- Modify: `crates/ccc-agent/src/runner.rs`
- Test: `crates/ccc-agent/src/runner.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn failed_remote_server_marks_connection_failed_without_aborting_chat() {
    let failures = runner.bootstrap_mcp_servers(&plan).await.unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(runner.connection_snapshots()[0].status, McpConnectionStatus::Failed);
}

#[tokio::test]
async fn auth_required_remote_server_marks_needs_auth() {
    let snapshots = runner.connection_snapshots();
    assert_eq!(snapshots[0].status, McpConnectionStatus::NeedsAuth);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-agent runner::`
Expected: FAIL because registry does not distinguish `failed` vs `needs-auth`

- [ ] **Step 3: Write minimal implementation**

Map transport-layer outcomes into:
- `Connected`
- `Failed`
- `NeedsAuth`

Leave reconnect conservative:
- remote transports may schedule retry metadata
- `stdio` does not auto-reconnect

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-agent`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-mcp/src/client.rs crates/ccc-agent/src/lib.rs crates/ccc-agent/src/runner.rs
git commit -m "feat: track remote mcp transport statuses"
```

## Chunk 6: Full Regression and Documentation Sync

### Task 13: Reconcile config snapshots, headless protocol, and docs

**Files:**
- Modify: `crates/ccc-cli/src/commands/config.rs`
- Modify: `docs/plans/ARCHITECTURE.md`
- Test: `crates/ccc-cli/src/commands/config.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn config_show_exposes_canonical_mcp_fields_and_managed_summary() {
    let snapshot = load_config_snapshot(&paths).unwrap();
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json["project"].get("enabledMcpServers").is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ccc-cli commands::config::`
Expected: FAIL because config show does not include the new fields

- [ ] **Step 3: Write minimal implementation**

Expose:
- canonical MCP fields
- managed settings summary
- enterprise-exclusive presence

Update `ARCHITECTURE.md` to record Phase 15 landed and Phase 16 runtime expansion status.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test -p ccc-cli commands::config::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ccc-cli/src/commands/config.rs docs/plans/ARCHITECTURE.md
git commit -m "docs: record phase 16 runtime expansion"
```

### Task 14: Final verification on merged feature branch

**Files:**
- No new repo-tracked files

- [ ] **Step 1: Run focused regression suites**

Run:
- `cargo test -p ccc-core`
- `cargo test -p ccc-cli`
- `cargo test -p ccc-agent`
- `cargo test -p ccc-mcp`

Expected: PASS

- [ ] **Step 2: Run workspace verification**

Run:
- `cargo test`

Expected: PASS

- [ ] **Step 3: Run CLI smoke checks**

Run:
- `cargo run -p ccc-cli -- --help`
- `cargo run -p ccc-cli -- config show`

Expected:
- exit code `0`
- config output includes new MCP/managed fields

- [ ] **Step 4: If credentials are available, run optional live smoke**

Run:
- `printf 'hello\\n' | cargo run -p ccc-cli -- chat --print --output-format json`

Expected:
- either a successful result object
- or a clear auth/API-key error result object without protocol corruption

- [ ] **Step 5: Commit any final doc/test-only cleanup**

```bash
git add -A
git commit -m "test: cover phase 16 runtime expansion"
```

## Execution Notes

1. 这份计划虽然覆盖 `16A/16B/16C`，但执行顺序必须严格保持：
   - `16A`
   - `16C`
   - `16B`
2. `plugin` 相关能力只实现到 “provider snapshot + policy gating + selector integration”。
3. 不为完成本计划而凭空引入完整 marketplace/installer/remote session UI。
4. 每个任务都先写失败测试，再写最小实现；如果做不到这一点，说明任务需要进一步拆小。
