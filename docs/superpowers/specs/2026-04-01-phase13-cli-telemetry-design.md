# Phase 13 CLI & Telemetry Design

## 背景

当前 workspace 已经具备以下基础能力：

- `ccc-auth`：OAuth PKCE、token refresh、secure storage
- `ccc-api`：Anthropic streaming client、SSE parser、retry
- `ccc-tools`：本地工具注册与执行
- `ccc-mcp`：MCP client 基础能力
- `ccc-agent`：消息循环、工具调用、MCP 接入
- `ccc-tui`：基础终端界面与输入循环

但仓库仍缺少两个直接阻塞可用性的入口层：

1. 没有统一的二进制入口，用户无法通过单一 `ccc` 命令访问登录、聊天和配置能力。
2. 没有统一的 telemetry bootstrap，后续 CLI、Agent、TUI 的启动、错误和流式执行都缺乏一致的观测点。

因此，Phase 13 定义为：在不复制 TS 巨型入口文件的前提下，建立一个可工作的 Rust 入口层，把交互、非交互和基础观测链路真正接通。

## 目标

Phase 13 需要一次性交付三条主线，但范围必须收敛：

1. 新增 `ccc-cli` crate，作为 workspace 唯一推荐入口。
2. 支持非交互 `chat --print` 路径，打通 prompt、stdin、streaming text 输出和退出码。
3. 新增 `ccc-telemetry` crate，提供统一的 tracing/bootstrap，允许后续 crate 复用。

交付完成后的最小可用命令集为：

- `ccc login`
- `ccc chat`
- `ccc chat --print [prompt]`
- `ccc config show`

## 非目标

本 phase 明确不做以下内容：

- 不追平 TS `main.tsx` 的全部 flags、remote mode、plugin system、policy gating、resume/fork-session、deep link、enterprise MCP 规则。
- 不实现完整配置热加载或全量配置分层引擎。
- 不在本 phase 内重写 `ccc-agent` 的所有内部结构，只抽出 CLI/TUI 都需要的最小共享执行边界。
- 不承诺输出格式与原 TS CLI 100% 等价，只保证 Rust 版本内部自洽且后续可演进。

## 用户可见行为

### `ccc login`

职责：

- 复用 `ccc-auth` 的 PKCE/OAuth 流程
- 在本机启动 callback listener
- 打开浏览器前打印授权 URL
- 成功后持久化 token

用户可见结果：

- 成功时输出简明确认信息
- 失败时返回非零退出码，并输出明确错误上下文

### `ccc chat`

职责：

- 初始化 telemetry
- 解析 CLI 覆盖参数
- 启动 TUI 主循环

约束：

- `ccc-cli` 不直接持有 UI 细节，TUI 生命周期由 `ccc-tui` 暴露的库入口承接

### `ccc chat --print [prompt]`

职责：

- 支持命令行 prompt
- 支持 stdin 管道输入
- 支持 prompt + stdin 合并
- 用共享 agent runner 处理流式事件
- 将 assistant 的文本内容打印到 stdout

输出策略：

- 默认仅输出 assistant 的文本内容
- thinking/tool events 先不暴露为稳定公共格式
- 错误信息写 stderr，并返回非零退出码

### `ccc config show`

职责：

- 展示当前可解析到的全局/项目配置快照
- 初始版本只做只读展示，不做交互修改

## 架构决策

### 方案选择

最终选择“纵向切片式入口整合”，而不是“TS 全量对齐式迁移”。

原因：

1. 当前 Rust crate 已具备核心能力，缺的是组装层，不是业务能力空白。
2. 如果直接搬运 TS 巨型入口，会把未实现的远程、插件、策略、恢复等次级系统一并拖入本 phase，风险失控。
3. CLI、非交互和 telemetry 可以通过一条共享执行链路闭环，这是当前最有性价比的增量。

### 核心边界

#### `ccc-cli`

职责：

- 使用 `clap` 定义命令树
- 负责参数解析、stdin 采集、退出码映射
- 调用 `ccc-auth`、`ccc-tui`、`ccc-agent`、`ccc-telemetry`

不负责：

- 不直接实现 OAuth 协议细节
- 不直接渲染 TUI
- 不直接持有 API/tool loop 细节

#### `ccc-telemetry`

职责：

- 提供统一初始化入口，例如 `init_telemetry(...)`
- 负责 tracing subscriber 生命周期
- 允许 `noop`、`stderr pretty`、`stderr json` 这三类基础模式

不负责：

- 不在本 phase 内引入复杂 exporter、collector、批量上报或账号绑定逻辑

#### `ccc-tui`

职责调整：

- 从“独立 bin”转成“可被 CLI 调用的库入口 + 可选薄 bin”
- 暴露一个明确的 `run_app(...)` 或等价函数，接收模型/配置等启动参数

#### `ccc-agent`

职责调整：

- 保留当前 `Agent` 作为状态持有者
- 新增共享执行层，供 TUI 与 `--print` 共用
- 把“事件如何消费”从“消息如何生成”中分离

建议新增抽象：

- `SessionRunner`：负责驱动用户输入到最终 assistant 输出的完整循环
- `EventSink` 或回调接口：负责消费 `StreamEvent`，用于 TUI 实时渲染和非交互 stdout 输出

## 文件与模块布局

Phase 13 完成后，推荐形成以下新增/改造边界：

### 新增 crate

- `crates/ccc-cli/Cargo.toml`
- `crates/ccc-cli/src/main.rs`
- `crates/ccc-cli/src/lib.rs`
- `crates/ccc-cli/src/commands/mod.rs`
- `crates/ccc-cli/src/commands/login.rs`
- `crates/ccc-cli/src/commands/chat.rs`
- `crates/ccc-cli/src/commands/config.rs`
- `crates/ccc-cli/src/cli.rs`
- `crates/ccc-cli/src/stdin.rs`
- `crates/ccc-cli/src/error.rs`

- `crates/ccc-telemetry/Cargo.toml`
- `crates/ccc-telemetry/src/lib.rs`
- `crates/ccc-telemetry/src/config.rs`

### 改造现有 crate

- `Cargo.toml`
  - 增加 workspace member：`ccc-cli`、`ccc-telemetry`

- `crates/ccc-tui/src/lib.rs`
  - 暴露公共运行入口

- `crates/ccc-tui/src/main.rs`
  - 退化为薄包装，或删除 bin-only 逻辑

- `crates/ccc-tui/src/app.rs`
  - 接受 CLI 注入的模型、初始 prompt、配置对象

- `crates/ccc-agent/src/lib.rs`
  - 收敛共享执行逻辑或转调新 runner

- `crates/ccc-agent/src/session.rs`
  - 扩充成 CLI/TUI 都能复用的会话状态容器

- `crates/ccc-agent/src/loop_engine.rs`
  - 要么并入共享 runner，要么继续作为非流式备用路径

- `docs/plans/ARCHITECTURE.md`
  - 更新 phase 编排与 crate 实际状态

## 关键流程

### 交互路径

1. `ccc-cli` 解析 `ccc chat`
2. 初始化 telemetry
3. 解析模型与基础配置
4. 调用 `ccc-tui` 公开入口
5. `ccc-tui` 持有 UI 状态并委托 `ccc-agent` 执行消息循环

### 非交互路径

1. `ccc-cli` 解析 `ccc chat --print [prompt]`
2. 聚合 argv prompt 与 stdin 内容
3. 初始化 telemetry
4. 创建 agent/session
5. 调用共享 runner
6. 流式事件映射到 stdout/stderr
7. 正常结束返回 `0`，失败返回非零

### 登录路径

1. `ccc-cli` 解析 `ccc login`
2. 初始化 telemetry
3. 启动本地 listener
4. 构造 authorize URL
5. 调用默认浏览器，若失败则至少打印 URL
6. 交换 token 并写入 storage

## 配置与参数策略

Phase 13 不做完整配置中心，但至少支持以下优先级：

1. CLI 显式参数
2. 环境变量
3. `ccc-core` 当前可表达的配置结构

初始建议支持的 CLI 参数：

- `--model`
- `--system-prompt`
- `--print`
- `--telemetry-format`
- `--telemetry-filter`

这样既能满足最小实用性，也不会提前把配置系统做成巨石。

## Telemetry 设计

### 初始化时机

在 `ccc-cli` 中尽早初始化，但必须发生在命令分发之后、实际副作用之前。

原因：

- 需要让 login/chat/config 共用同一套 subscriber
- 避免每个子命令重复初始化
- 避免测试环境里因全局 subscriber 重复安装导致脆弱行为

### 初始输出模式

- 默认：`noop`
- 调试模式：`stderr pretty`
- 机器消费模式：`stderr json`

### 事件范围

Phase 13 先覆盖以下事件：

- CLI command start/stop
- login flow start/success/failure
- chat session start/stop
- stream turn failure
- tool execution failure

## 测试策略

### `ccc-cli`

- 命令解析单元测试
- prompt/ststdin 聚合测试
- 退出码映射测试

### `ccc-auth`

- 对 `login` orchestration 做 mock 驱动测试
- 不在测试中打开真实浏览器

### `ccc-agent`

- 为共享 runner 补充非交互回调/事件汇聚测试
- 验证最终 stdout 只包含 assistant text

### `ccc-tui`

- 入口适配测试至少验证编译通过与公共 API 可调用

### `ccc-telemetry`

- 初始化幂等性测试
- 各输出模式的 smoke test

### workspace 级验证

- `cargo test`
- `cargo run -p ccc-cli -- --help`
- `cargo run -p ccc-cli -- config show`

## 风险与缓解

### 风险 1：入口逻辑再次膨胀成单文件巨石

缓解：

- 命令分发、stdin 采集、telemetry 初始化、业务执行分文件拆分

### 风险 2：TUI 与 `--print` 走两套 agent 流程，后续行为分叉

缓解：

- 强制抽共享 runner
- TUI 和非交互只在“事件消费端”不同

### 风险 3：telemetry 初始化污染测试

缓解：

- 使用显式初始化函数
- 提供 no-op 配置
- 测试中避免重复安装全局 subscriber

### 风险 4：登录流程在无图形环境下体验脆弱

缓解：

- 浏览器打开失败时保底打印 URL
- 始终输出下一步操作提示

## 验收标准

满足以下条件即可认为 Phase 13 完成：

1. workspace 新增 `ccc-cli` 与 `ccc-telemetry` 两个 crate。
2. `ccc chat` 能通过 CLI 启动现有 TUI。
3. `ccc chat --print` 能在非交互模式下消费 prompt/ststdin 并输出 assistant 文本。
4. `ccc login` 能跑通到 token 持久化的主链路。
5. `ccc config show` 能输出当前配置快照。
6. telemetry bootstrap 被 CLI 统一调用。
7. `cargo test` 通过。

## 后续阶段预留

Phase 13 完成后，下一步最自然的扩展方向是：

- 完整配置分层与热加载
- 更丰富的非交互输出格式
- telemetry exporter/OTel pipeline
- session resume / persistence
- 更完整的 MCP config 和 policy 处理
