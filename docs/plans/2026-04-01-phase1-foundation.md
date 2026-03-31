# Phase 1 — 基础层实施计划

> crate：`ccc-core`, `ccc-platform`, `ccc-vim`
> 验证：`cargo test -p ccc-core -p ccc-platform -p ccc-vim` 全绿
> 原则：TDD，每个任务以失败测试开始，先最小实现，再提交

---

## 文件结构

```
ccc-rs/
├── Cargo.toml                          # workspace root
├── crates/
│   ├── ccc-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # pub use 入口
│   │       ├── types.rs                # Message, ContentBlock, Role, Tool trait
│   │       ├── config.rs               # GlobalConfig, ProjectConfig schema
│   │       ├── permissions.rs          # PermissionMode, ToolPermission
│   │       ├── ids.rs                  # SessionId, AgentId（newtype wrapper）
│   │       └── error.rs               # CccError（thiserror）
│   ├── ccc-platform/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── detect.rs               # Platform enum, get_platform(), get_wsl_version()
│   │       ├── vcs.rs                  # detect_vcs(), VcsType enum
│   │       └── keychain.rs             # SecureStorage trait, platform impls
│   └── ccc-vim/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── types.rs                # VimState, CommandState, Operator, FindType
│           ├── transitions.rs          # transition() 纯函数
│           ├── motions.rs              # resolve_motion() 纯函数
│           ├── operators.rs            # execute_operator()
│           └── text_objects.rs        # resolve_text_object()
```

---

## Chunk 1: Cargo Workspace

### 任务 1.1 — 写失败的 workspace smoke test

**目标：** 确认 workspace 结构正确后续可 build。

在 `/Users/zhangjinhui/Desktop/ccc-rs/` 创建根 `Cargo.toml`：

```toml
[workspace]
resolver = "2"
members = [
    "crates/ccc-core",
    "crates/ccc-platform",
    "crates/ccc-vim",
]

[workspace.dependencies]
# error handling
thiserror = "2"
anyhow = "1"
# serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
# async
tokio = { version = "1", features = ["full"] }
# platform
keyring = "3"
# testing
pretty_assertions = "1"
```

**验证：** `cargo check` 无报错（crate 目录尚未创建会报错，先跳过）。

---

### 任务 1.2 — 创建三个空 crate

```bash
cargo new --lib crates/ccc-core
cargo new --lib crates/ccc-platform
cargo new --lib crates/ccc-vim
```

每个 `Cargo.toml` 开头加：
```toml
[package]
name = "ccc-core"   # 各自名称
version = "0.1.0"
edition = "2021"
```

**验证：** `cargo build --workspace` 成功。提交：`chore: init cargo workspace with three empty crates`

---

## Chunk 2: ccc-core

参考源文件：
- `src/types/ids.ts` → `ids.rs`
- `src/types/permissions.ts` → `permissions.rs`
- `src/utils/config.ts`（前 120 行）→ `config.rs`
- `src/Tool.ts` → `types.rs`（Tool trait 定义）

### 任务 2.1 — ids.rs（TDD）

**先写测试：**

```rust
// crates/ccc-core/src/ids.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_from_str() {
        let id = SessionId::new("ses_abc123".to_string());
        assert_eq!(id.as_str(), "ses_abc123");
    }

    #[test]
    fn agent_id_valid_format() {
        assert!(AgentId::try_from("a-deadbeefcafe0123").is_ok());
        assert!(AgentId::try_from("alabel-deadbeefcafe0123").is_ok());
    }

    #[test]
    fn agent_id_invalid_format() {
        assert!(AgentId::try_from("not-an-agent").is_err());
        assert!(AgentId::try_from("").is_err());
    }
}
```

运行 `cargo test -p ccc-core` 确认失败，然后实现：

```rust
use std::str::FromStr;
use crate::error::CccError;

/// 对应 TS SessionId = string & { __brand: 'SessionId' }
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(s: String) -> Self { Self(s) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// 对应 TS AgentId，验证 `a(?:.+-)?[0-9a-f]{16}` 格式
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AgentId(String);

impl AgentId {
    fn is_valid(s: &str) -> bool {
        // a + optional "label-" + exactly 16 hex chars
        let s = s.strip_prefix('a').unwrap_or("");
        let hex_part = match s.rfind('-') {
            Some(pos) => &s[pos + 1..],
            None => s,
        };
        hex_part.len() == 16 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
    }
}

impl TryFrom<&str> for AgentId {
    type Error = CccError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if Self::is_valid(s) {
            Ok(Self(s.to_string()))
        } else {
            Err(CccError::InvalidId(s.to_string()))
        }
    }
}
```

**验证：** `cargo test -p ccc-core ids` 全绿。

---

### 任务 2.2 — error.rs

```rust
// crates/ccc-core/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum CccError {
    #[error("invalid ID format: {0}")]
    InvalidId(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("permission denied: {tool} requires {permission}")]
    PermissionDenied { tool: String, permission: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
```

`Cargo.toml` 加依赖：
```toml
[dependencies]
thiserror.workspace = true
serde = { workspace = true }
serde_json = { workspace = true }
```

**验证：** `cargo build -p ccc-core`。

---

### 任务 2.3 — permissions.rs（TDD）

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_serde_roundtrip() {
        let modes = [
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::BypassPermissions,
            PermissionMode::Plan,
        ];
        for mode in modes {
            let s = serde_json::to_string(&mode).unwrap();
            let back: PermissionMode = serde_json::from_str(&s).unwrap();
            assert_eq!(mode, back);
        }
    }
}
```

实现：

```rust
/// 对应 TS ExternalPermissionMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
    Default,
    DontAsk,
    Plan,
}

/// 内部模式（含 Auto / Bubble，不暴露给用户）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalPermissionMode {
    External(PermissionMode),
    Auto,
    Bubble,
}
```

**验证：** `cargo test -p ccc-core permissions` 全绿。

---

### 任务 2.4 — types.rs（Message/ContentBlock/Tool trait）

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_block_text_serde() {
        let block = ContentBlock::Text { text: "hello".into() };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn message_role_serde() {
        assert_eq!(
            serde_json::to_string(&Role::User).unwrap(),
            "\"user\""
        );
    }
}
```

实现（对应 Anthropic Messages API schema）：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: Vec<ContentBlock>, is_error: Option<bool> },
    Thinking { thinking: String, signature: String },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// 对应 TS Tool trait（src/Tool.ts）
/// 每个工具实现此 trait
pub trait ToolDef: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
}
```

**验证：** `cargo test -p ccc-core types` 全绿。

---

### 任务 2.5 — config.rs（GlobalConfig + ProjectConfig）

参考 `src/utils/config.ts` 中的 `GlobalConfig` 和 `ProjectConfig` 类型。只实现 schema（serde），不实现读写 IO（IO 在 ccc-platform）。

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_config_defaults() {
        let cfg = GlobalConfig::default();
        assert!(!cfg.has_completed_onboarding);
        assert_eq!(cfg.theme, Theme::Dark);
    }

    #[test]
    fn project_config_serde_roundtrip() {
        let cfg = ProjectConfig { allowed_tools: vec!["bash".into()], ..Default::default() };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProjectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.allowed_tools, back.allowed_tools);
    }
}
```

实现（仅关键字段，完整字段后续补全）：

```rust
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalConfig {
    pub has_completed_onboarding: bool,
    pub theme: Theme,
    pub preferred_notify_sound: bool,
    pub custom_api_key_responses: std::collections::HashMap<String, String>,
    pub mcps_agreed_to_terms: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectConfig {
    pub allowed_tools: Vec<String>,
    pub mcp_context_uris: Vec<String>,
    pub has_trust_dialog_accepted: Option<bool>,
    pub last_session_id: Option<String>,
    pub last_cost: Option<f64>,
}
```

**验证：** `cargo test -p ccc-core config` 全绿。提交：`feat(ccc-core): types, config, permissions, ids, error`

---

## Chunk 3: ccc-platform

参考源文件：
- `src/utils/platform.ts` → `detect.rs`
- VCS 检测逻辑（memoize 等价 → `std::sync::OnceLock`）→ `vcs.rs`
- `src/utils/secureStorage/` → `keychain.rs`

`Cargo.toml` 依赖：
```toml
[dependencies]
ccc-core = { path = "../ccc-core" }
thiserror.workspace = true
serde.workspace = true

[target.'cfg(target_os = "macos")'.dependencies]
keyring.workspace = true

[target.'cfg(target_os = "linux")'.dependencies]
keyring.workspace = true
```

### 任务 3.1 — detect.rs（TDD）

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_platform_returns_known_value() {
        let p = get_platform();
        // 在 CI 中必然是 mac/linux/windows/wsl 之一
        assert!(matches!(p,
            Platform::Mac | Platform::Linux | Platform::Wsl |
            Platform::Windows | Platform::Unknown
        ));
    }

    #[test]
    fn platform_is_memoized() {
        // 两次调用结果相同（也验证 OnceLock 不 panic）
        assert_eq!(get_platform(), get_platform());
    }
}
```

实现：

```rust
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
    Wsl,
    Windows,
    Unknown,
}

static PLATFORM: OnceLock<Platform> = OnceLock::new();

pub fn get_platform() -> Platform {
    *PLATFORM.get_or_init(detect_platform)
}

fn detect_platform() -> Platform {
    match std::env::consts::OS {
        "macos" => Platform::Mac,
        "windows" => Platform::Windows,
        "linux" => {
            // WSL 检测：读取 /proc/version
            if let Ok(v) = std::fs::read_to_string("/proc/version") {
                let lower = v.to_lowercase();
                if lower.contains("microsoft") || lower.contains("wsl") {
                    return Platform::Wsl;
                }
            }
            Platform::Linux
        }
        _ => Platform::Unknown,
    }
}

/// 返回 WSL 版本号字符串（"1", "2" 等），非 WSL 返回 None
pub fn get_wsl_version() -> Option<String> {
    static WSL_VERSION: OnceLock<Option<String>> = OnceLock::new();
    WSL_VERSION.get_or_init(|| {
        if std::env::consts::OS != "linux" { return None; }
        let v = std::fs::read_to_string("/proc/version").ok()?;
        // 优先查找 "WSL2" 等显式标记
        if let Some(cap) = regex_lite::Regex::new(r"WSL(\d+)").ok()
            .and_then(|re| re.captures(&v)) {
            return Some(cap[1].to_string());
        }
        if v.to_lowercase().contains("microsoft") { return Some("1".into()); }
        None
    }).clone()
}
```

注意：`regex_lite` 需加入 workspace 依赖（`regex-lite = "0.1"`）。

**验证：** `cargo test -p ccc-platform detect` 全绿。

---

### 任务 3.2 — vcs.rs（TDD）

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_git_repo() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        assert_eq!(detect_vcs(dir.path()), Some(VcsType::Git));
    }

    #[test]
    fn detects_no_vcs() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_vcs(dir.path()), None);
    }

    #[test]
    fn detects_mercurial() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".hg")).unwrap();
        assert_eq!(detect_vcs(dir.path()), Some(VcsType::Mercurial));
    }
}
```

`Cargo.toml` 加 `tempfile = "3"` 到 `[dev-dependencies]`（workspace 层也加）。

实现：

```rust
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsType {
    Git,
    Mercurial,
    Svn,
    Perforce,
    Tfs,
    Jujutsu,
    Sapling,
}

/// 对应 TS detectVcs()：在指定目录查找 VCS 标记文件/目录
pub fn detect_vcs(dir: &Path) -> Option<VcsType> {
    let markers: &[(&str, VcsType)] = &[
        (".git",      VcsType::Git),
        (".hg",       VcsType::Mercurial),
        (".svn",      VcsType::Svn),
        (".p4config", VcsType::Perforce),
        (".jj",       VcsType::Jujutsu),
        (".sl",       VcsType::Sapling),
    ];
    for (marker, vcs) in markers {
        if dir.join(marker).exists() { return Some(*vcs); }
    }
    // Perforce 环境变量备用
    if std::env::var("P4PORT").is_ok() { return Some(VcsType::Perforce); }
    None
}
```

**验证：** `cargo test -p ccc-platform vcs` 全绿。

---

### 任务 3.3 — keychain.rs（trait + 平台 impl）

对应 `src/utils/secureStorage/`。定义统一 trait，平台特化实现。

```rust
use ccc_core::error::CccError;

/// 安全存储抽象，对应 TS SecureStorage 接口
pub trait SecureStorage: Send + Sync {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, CccError>;
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), CccError>;
    fn delete(&self, service: &str, key: &str) -> Result<(), CccError>;
}

/// 基于 `keyring` crate 的实现（macOS Keychain / Linux Secret Service / Windows DPAPI）
pub struct KeyringStorage;

impl SecureStorage for KeyringStorage {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, CccError> {
        match keyring::Entry::new(service, key).and_then(|e| e.get_password()) {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CccError::Config(e.to_string())),
        }
    }

    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), CccError> {
        keyring::Entry::new(service, key)
            .and_then(|e| e.set_password(value))
            .map_err(|e| CccError::Config(e.to_string()))
    }

    fn delete(&self, service: &str, key: &str) -> Result<(), CccError> {
        match keyring::Entry::new(service, key).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CccError::Config(e.to_string())),
        }
    }
}
```

**验证：** `cargo build -p ccc-platform`（keychain 单测需真实 keychain，跳过 CI）。提交：`feat(ccc-platform): detect, vcs, keychain`

---

## Chunk 4: ccc-vim

参考源文件：
- `src/vim/types.ts` → `types.rs`
- `src/vim/transitions.ts` → `transitions.rs`
- `src/vim/motions.ts` → `motions.rs`
- `src/vim/operators.ts` → `operators.rs`
- `src/vim/textObjects.ts` → `text_objects.rs`

`Cargo.toml`：
```toml
[dependencies]
ccc-core = { path = "../ccc-core" }
```

这是纯逻辑 crate，无 I/O，全部可以完整单测。

### 任务 4.1 — types.rs（直译 TS 类型）

```rust
/// 对应 TS Operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator { Delete, Change, Yank }

/// 对应 TS FindType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindType { F, BigF, T, BigT }

/// 对应 TS TextObjScope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjScope { Inner, Around }

/// 对应 TS CommandState（命令状态机节点）
#[derive(Debug, Clone, PartialEq)]
pub enum CommandState {
    Idle,
    Count          { count: u32 },
    Operator       { op: Operator, count: u32 },
    OperatorCount  { op: Operator, count: u32, op_count: u32 },
    OperatorFind   { op: Operator, count: u32, find_type: FindType },
    OperatorTextObj{ op: Operator, count: u32 },
    Find           { find_type: FindType, count: u32 },
    G              { count: u32 },
    OperatorG      { op: Operator, count: u32 },
    Replace,
    Indent         { dir: IndentDir, count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentDir { In, Out }

/// 对应 TS VimState（顶层模式）
#[derive(Debug, Clone, PartialEq)]
pub enum VimState {
    Insert { inserted_text: String },
    Normal { command: CommandState },
}

/// 跨模式持久化状态（对应 TS PersistentState）
#[derive(Debug, Clone, Default)]
pub struct PersistentState {
    pub last_change: Option<RecordedChange>,
    pub last_find: Option<LastFind>,
    pub register: String,
    pub register_is_linewise: bool,
}

#[derive(Debug, Clone)]
pub struct RecordedChange {
    pub keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LastFind {
    pub find_type: FindType,
    pub ch: char,
}
```

**验证：** `cargo build -p ccc-vim`。

---

### 任务 4.2 — transitions.rs（TDD，核心状态机）

**先写测试（覆盖主要转移路径）：**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn idle() -> CommandState { CommandState::Idle }

    #[test]
    fn idle_digit_goes_to_count() {
        let next = transition_command(idle(), "3");
        assert_eq!(next, CommandState::Count { count: 3 });
    }

    #[test]
    fn idle_d_goes_to_operator() {
        let next = transition_command(idle(), "d");
        assert_eq!(next, CommandState::Operator { op: Operator::Delete, count: 1 });
    }

    #[test]
    fn operator_motion_resets_to_idle() {
        // d + w：执行后回到 Idle
        let state = CommandState::Operator { op: Operator::Delete, count: 1 };
        let result = transition_command_with_execute(state, "w");
        assert!(matches!(result, TransitionResult::Execute { .. }));
    }

    #[test]
    fn dd_executes_line_op() {
        let state = CommandState::Operator { op: Operator::Delete, count: 1 };
        let result = transition_command_with_execute(state, "d");
        assert!(matches!(result, TransitionResult::ExecuteLine { .. }));
    }

    #[test]
    fn count_accumulates() {
        let state = CommandState::Count { count: 1 };
        let next = transition_command(state, "2");
        assert_eq!(next, CommandState::Count { count: 12 });
    }
}
```

实现（核心 transition 函数签名）：

```rust
use crate::types::*;

pub enum TransitionResult {
    /// 状态转移到下一个 CommandState
    Next(CommandState),
    /// 执行 motion（含 op、motion 类型、count）
    Execute { op: Option<Operator>, motion: Motion, count: u32 },
    /// 执行整行操作（dd/cc/yy）
    ExecuteLine { op: Operator, count: u32 },
    /// 无效输入，保持原状态
    Ignore,
}

/// 纯函数：给定当前命令状态和一个键，返回转移结果
/// 对应 TS transition() + fromIdle() + fromOperator() 等
pub fn transition_command(state: CommandState, key: &str) -> CommandState {
    match transition_command_with_execute(state, key) {
        TransitionResult::Next(s) => s,
        TransitionResult::Execute { .. } | TransitionResult::ExecuteLine { .. } => CommandState::Idle,
        TransitionResult::Ignore => CommandState::Idle,
    }
}

pub fn transition_command_with_execute(state: CommandState, key: &str) -> TransitionResult {
    match state {
        CommandState::Idle => from_idle(key),
        CommandState::Count { count } => from_count(count, key),
        CommandState::Operator { op, count } => from_operator(op, count, key),
        // ... 其余分支参照 src/vim/transitions.ts 逐一实现
        _ => TransitionResult::Ignore,
    }
}

fn from_idle(key: &str) -> TransitionResult {
    match key {
        "d" => TransitionResult::Next(CommandState::Operator { op: Operator::Delete, count: 1 }),
        "c" => TransitionResult::Next(CommandState::Operator { op: Operator::Change, count: 1 }),
        "y" => TransitionResult::Next(CommandState::Operator { op: Operator::Yank, count: 1 }),
        "g" => TransitionResult::Next(CommandState::G { count: 1 }),
        "r" => TransitionResult::Next(CommandState::Replace),
        k if k.len() == 1 && k.chars().next().map_or(false, |c| c.is_ascii_digit() && c != '0') => {
            let digit = k.parse::<u32>().unwrap();
            TransitionResult::Next(CommandState::Count { count: digit })
        }
        // motion 键直接执行
        "w" | "b" | "e" | "0" | "$" | "h" | "l" | "j" | "k" | "^" | "G" => {
            TransitionResult::Execute { op: None, motion: key_to_motion(key), count: 1 }
        }
        _ => TransitionResult::Ignore,
    }
}

fn from_count(count: u32, key: &str) -> TransitionResult {
    if let Ok(d) = key.parse::<u32>() {
        return TransitionResult::Next(CommandState::Count { count: count * 10 + d });
    }
    // count + operator
    match key {
        "d" => TransitionResult::Next(CommandState::Operator { op: Operator::Delete, count }),
        "c" => TransitionResult::Next(CommandState::Operator { op: Operator::Change, count }),
        "y" => TransitionResult::Next(CommandState::Operator { op: Operator::Yank, count }),
        _ => TransitionResult::Ignore,
    }
}

fn from_operator(op: Operator, count: u32, key: &str) -> TransitionResult {
    // dd/cc/yy → 整行
    let op_char = match op {
        Operator::Delete => "d",
        Operator::Change => "c",
        Operator::Yank   => "y",
    };
    if key == op_char {
        return TransitionResult::ExecuteLine { op, count };
    }
    match key {
        "i" | "a" => TransitionResult::Next(CommandState::OperatorTextObj { op, count }),
        "f" | "F" | "t" | "T" => TransitionResult::Next(
            CommandState::OperatorFind { op, count, find_type: char_to_find_type(key) }
        ),
        k if k.parse::<u32>().is_ok() => TransitionResult::Next(
            CommandState::OperatorCount { op, count, op_count: k.parse().unwrap() }
        ),
        // motion → execute
        _ => TransitionResult::Execute { op: Some(op), motion: key_to_motion(key), count },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Word, Back, End, StartOfLine, EndOfLine,
    Left, Right, Up, Down, FirstNonBlank,
    GoToEnd,
    Unknown,
}

fn key_to_motion(key: &str) -> Motion {
    match key {
        "w" => Motion::Word, "b" => Motion::Back, "e" => Motion::End,
        "0" => Motion::StartOfLine, "$" => Motion::EndOfLine,
        "h" => Motion::Left, "l" => Motion::Right,
        "j" => Motion::Down, "k" => Motion::Up,
        "^" => Motion::FirstNonBlank, "G" => Motion::GoToEnd,
        _ => Motion::Unknown,
    }
}

fn char_to_find_type(key: &str) -> FindType {
    match key {
        "f" => FindType::F, "F" => FindType::BigF,
        "t" => FindType::T, _ => FindType::BigT,
    }
}
```

**验证：** `cargo test -p ccc-vim transitions` 全绿。

---

### 任务 4.3 — motions.rs（TDD）

**先写测试：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_right_clamps_to_len() {
        // cursor 在末尾，向右不越界
        let result = resolve_motion(Motion::Right, 5, 5, "hello");
        assert_eq!(result, 5);
    }

    #[test]
    fn move_to_end_of_line() {
        let result = resolve_motion(Motion::EndOfLine, 0, 1, "hello world");
        assert_eq!(result, 10); // 最后一个字符索引
    }

    #[test]
    fn word_motion_skips_to_next_word() {
        let result = resolve_motion(Motion::Word, 0, 1, "hello world");
        assert_eq!(result, 6);
    }
}
```

实现（`resolve_motion` 为纯函数，接受 cursor 位置、count 和文本，返回新位置）：

```rust
use crate::transitions::Motion;

/// 纯函数：给定 motion、当前 offset、count、文本内容，返回新 offset
/// 对应 TS resolveMotion()
pub fn resolve_motion(motion: Motion, offset: usize, count: u32, text: &str) -> usize {
    let len = text.len();
    if len == 0 { return 0; }
    match motion {
        Motion::Right => (offset + count as usize).min(len.saturating_sub(1)),
        Motion::Left  => offset.saturating_sub(count as usize),
        Motion::EndOfLine => len.saturating_sub(1),
        Motion::StartOfLine => 0,
        Motion::Word  => word_forward(text, offset, count),
        Motion::Back  => word_backward(text, offset, count),
        Motion::GoToEnd => len.saturating_sub(1),
        Motion::FirstNonBlank => text.find(|c: char| !c.is_whitespace()).unwrap_or(0),
        _ => offset,
    }
}

fn word_forward(text: &str, mut pos: usize, count: u32) -> usize {
    let chars: Vec<char> = text.chars().collect();
    for _ in 0..count {
        // 跳过当前 word
        while pos < chars.len() && !chars[pos].is_whitespace() { pos += 1; }
        // 跳过空白
        while pos < chars.len() && chars[pos].is_whitespace() { pos += 1; }
    }
    pos.min(chars.len().saturating_sub(1))
}

fn word_backward(text: &str, mut pos: usize, count: u32) -> usize {
    let chars: Vec<char> = text.chars().collect();
    for _ in 0..count {
        if pos > 0 { pos -= 1; }
        while pos > 0 && chars[pos].is_whitespace() { pos -= 1; }
        while pos > 0 && !chars[pos - 1].is_whitespace() { pos -= 1; }
    }
    pos
}
```

**验证：** `cargo test -p ccc-vim motions` 全绿。提交：`feat(ccc-vim): state machine, transitions, motions`

---

### 任务 4.4 — operators.rs + text_objects.rs

对应 `src/vim/operators.ts` 和 `src/vim/textObjects.ts`。

这两个文件操作文本字符串（纯函数，无 I/O）：
- `execute_operator(op, text, start, end) -> String`：执行 delete/change/yank
- `resolve_text_object(scope, obj_char, text, cursor) -> (usize, usize)`：返回文本对象的 `(start, end)`

**先写测试：**

```rust
// operators.rs tests
#[test]
fn delete_range() {
    let result = execute_delete("hello world", 0, 5);
    assert_eq!(result, "world");
}

// text_objects.rs tests
#[test]
fn inner_word() {
    // "hello world"，cursor 在 'e'(1)，iw → (0, 5)
    let (s, e) = resolve_text_object(TextObjScope::Inner, 'w', "hello world", 1);
    assert_eq!((s, e), (0, 5));
}

#[test]
fn inner_quotes() {
    // cursor 在引号内
    let (s, e) = resolve_text_object(TextObjScope::Inner, '"', "say \"hi\" there", 6);
    assert_eq!(&"say \"hi\" there"[s..e], "hi");
}
```

**验证：** `cargo test -p ccc-vim operators text_objects` 全绿。提交：`feat(ccc-vim): operators, text_objects`

---

## Chunk 5: 最终验证与收尾

### 任务 5.1 — 整体测试

```bash
cargo test -p ccc-core -p ccc-platform -p ccc-vim
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

所有测试绿，clippy 无 warning，fmt 格式正确。

### 任务 5.2 — 补全 ccc-core/src/lib.rs

```rust
pub mod config;
pub mod error;
pub mod ids;
pub mod permissions;
pub mod types;

pub use config::{GlobalConfig, ProjectConfig, Theme};
pub use error::CccError;
pub use ids::{AgentId, SessionId};
pub use permissions::{InternalPermissionMode, PermissionMode};
pub use types::{ContentBlock, ImageSource, Message, Role, ToolDef};
```

### 任务 5.3 — 提交

```bash
git add -A
git commit -m "feat: Phase 1 foundation — ccc-core, ccc-platform, ccc-vim"
```

---

## 进入 Phase 2

Phase 1 完成后，下一步见 `docs/plans/2026-04-01-phase2-auth.md`。

Phase 2 依赖：`ccc-core` + `ccc-platform`（keychain），新增 `ccc-auth`（OAuth、API key、AWS STS、GCP）。