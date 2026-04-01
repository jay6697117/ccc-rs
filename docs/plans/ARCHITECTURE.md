# Claude Code CLI — Rust 重写架构总览

> 源项目：`claude-copy-code/`（1884 TS/TSX 文件，v2.1.88）
> 目标：单一静态二进制，功能等价，高级 Rust 工程师主导

---

## Workspace 结构

```
ccc-rs/
├── Cargo.toml                  # workspace root，统一依赖版本
├── crates/
│   ├── ccc-core/               # 核心类型、配置 schema、错误、trait
│   ├── ccc-platform/           # 平台检测（macOS/Linux/WSL/Windows）
│   ├── ccc-vim/                # Vim 状态机（纯逻辑，无 I/O）
│   ├── ccc-auth/               # 认证（OAuth / API key / AWS STS / GCP）
│   ├── ccc-api/                # Anthropic 多 provider 流式 HTTP 客户端
│   ├── ccc-tools/              # 40 个 Tool 实现
│   ├── ccc-mcp/                # MCP 协议 client + server（JSON-RPC over stdio/SSE）
│   ├── ccc-agent/              # Agent 主循环与共享 SessionRunner
│   ├── ccc-tui/                # TUI 渲染与交互入口（ratatui + crossterm）
│   ├── ccc-telemetry/          # tracing/bootstrap 初始化层
│   └── ccc-cli/                # 统一二进制入口，clap 命令树
├── docs/
│   └── plans/                  # 实施计划文档
└── claude-copy-code/           # 原 TS 源码（参考用）
```

## 依赖关系图

```
ccc-cli
  └─ ccc-agent, ccc-tui, ccc-auth, ccc-core, ccc-telemetry
       └─ ccc-agent
            └─ ccc-api, ccc-tools, ccc-mcp, ccc-core
       └─ ccc-tui
            └─ ccc-agent, ccc-vim, ccc-core
       └─ ccc-auth
            └─ ccc-platform, ccc-core
       └─ ccc-api
            └─ ccc-auth, ccc-core
       └─ ccc-mcp
            └─ ccc-core

ccc-platform  ──► ccc-core
ccc-vim       ──► ccc-core
```

## 关键 crate 选型

| 功能 | crate | 对应 TS 依赖 |
|---|---|---|
| CLI 解析 | `clap` v4 (derive) | `@commander-js/extra-typings` |
| 异步运行时 | `tokio` (full features) | Node.js event loop |
| HTTP 客户端 | `reqwest` (stream + rustls) | `axios` + Anthropic SDK |
| JSON | `serde_json` | `jsonc-parser` |
| YAML | `serde_yaml` | `yaml` |
| JSONC 解析 | `jsonc-to-json`（预处理去注释）| `jsonc-parser` |
| 配置分层 | `figment` | 手写合并逻辑 |
| TUI | `ratatui` + `crossterm` | `ink` + React + Yoga |
| WebSocket | `tokio-tungstenite` | `ws` |
| keychain | `keyring` v3 | macOS Security.framework |
| 进程执行 | `tokio::process` | `execa` |
| 文件 glob | `globset` + `ignore` | `picomatch` + `ignore` |
| Unicode 宽度 | `unicode-width` | `get-east-asian-width` |
| ANSI 解析 | `anstyle-parse` | `@alcalzone/ansi-tokenize` |
| 差量算法 | `similar` | `diff` |
| Telemetry bootstrap | `tracing` + `tracing-subscriber` | OTel / internal logging bootstrap |
| 错误处理（bin）| `anyhow` | — |
| 错误处理（lib）| `thiserror` | — |
| JSON Schema | `schemars` | `zod` |
| fuzzy 搜索 | `nucleo` | `fuse.js` |
| QR 码 | `qrcode` | `qrcode` |
| bidi 文本 | `unicode-bidi` | `bidi-js` |

## 实施阶段

| Phase | crate(s) | 计划文档 | 验证方式 |
|---|---|---|---|
| 1 | `ccc-core`, `ccc-platform`, `ccc-vim` | `2026-04-01-phase1-foundation.md` | `cargo test` |
| 2 | `ccc-auth` | `2026-04-01-phase2-auth.md` | mock OAuth server |
| 3 | `ccc-api` | `2026-04-01-phase3-api.md` | wiremock HTTP |
| 4 | `ccc-tools` | `2026-04-01-phase4-tools.md` | 单元 + 集成 |
| 5 | `ccc-mcp` | `2026-04-01-phase5-mcp.md` | MCP mock server |
| 6 | `ccc-agent` | `2026-04-01-phase6-agent.md` | 端到端 mock |
| 7 | `ccc-tui` | `2026-04-01-phase7-tui.md` | headless 渲染 |
| 8 | `ccc-vim` advanced | `2026-04-01-phase8-vim-advanced.md` | motion/operator tests |
| 9 | `ccc-telemetry` | `2026-04-01-phase9-telemetry.md` | tracing bootstrap tests |
| 10 | `ccc-cli` | `2026-04-01-phase10-cli.md` | clap parsing tests |
| 11 | `ccc-core` refinement | `2026-04-01-phase11-core-refinement.md` | config loading tests |
| 12 | TUI + Agent integration | `2026-04-01-phase12-integration.md` | workspace integration tests |
| 13 | `ccc-cli`, `ccc-telemetry`, `ccc-agent`, `ccc-tui` | `docs/superpowers/specs/2026-04-01-phase13-cli-telemetry-design.md` | `cargo test`, `ccc --help`, `ccc config show` |

## TS → Rust 文件映射（关键路径）

| Rust 文件 | TS 源文件 |
|---|---|
| `ccc-core/src/types.rs` | `src/types/`, `src/schemas/` |
| `ccc-core/src/config.rs` | `src/utils/config.ts` |
| `ccc-core/src/tool_trait.rs` | `src/Tool.ts` |
| `ccc-platform/src/lib.rs` | `src/utils/platform.ts` |
| `ccc-platform/src/vcs.rs` | `src/utils/vcs.ts` |
| `ccc-platform/src/keychain.rs` | `src/utils/secureStorage/` |
| `ccc-vim/src/types.rs` | `src/vim/types.ts` |
| `ccc-vim/src/transitions.rs` | `src/vim/transitions.ts` |
| `ccc-vim/src/motions.rs` | `src/vim/motions.ts` |
| `ccc-vim/src/operators.rs` | `src/vim/operators.ts` |
| `ccc-vim/src/text_objects.rs` | `src/vim/textObjects.ts` |
| `ccc-auth/src/oauth.rs` | `src/utils/auth.ts` |
| `ccc-api/src/client.rs` | `src/services/api/claude.ts` |
| `ccc-api/src/stream.rs` | `src/utils/stream.ts` |
| `ccc-tools/src/bash.rs` | `src/tools/BashTool/` |
| `ccc-tools/src/file_read.rs` | `src/tools/FileReadTool/` |
| `ccc-tools/src/file_edit.rs` | `src/tools/FileEditTool/` |
| `ccc-mcp/src/client.rs` | `src/services/mcp/client.ts` |
| `ccc-agent/src/lib.rs` | `src/coordinator/`, `src/QueryEngine.ts` |
| `ccc-agent/src/runner.rs` | `src/query.ts`, `src/screens/REPL.tsx` |
| `ccc-tui/src/app.rs` | `src/ink/ink.tsx`, `src/ink/root.ts` |
| `ccc-tui/src/events.rs` | `src/ink/hooks/use-input.ts` |
| `ccc-cli/src/main.rs` | `src/main.tsx` |
