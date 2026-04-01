# Phase 14 Session Persistence & MCP Bootstrap Design

## 背景

Phase 13 已经把 `ccc-cli`、`ccc-tui`、`ccc-agent`、`ccc-telemetry` 接成一条可运行的主链路，但当前运行时仍有两个核心缺口：

1. 配置存在但没有真正驱动 chat 运行时。`ccc chat` 和 `ccc chat --print` 只消费 CLI 参数，不消费项目配置里的 MCP 和 session 相关字段。
2. 会话仍然是纯内存模型。`SessionRunner` 每次从空状态启动，`ProjectConfig.last_session_id` 只是 schema 预留字段，没有被读写。

因此，Phase 14 的目标不是再堆入口 flag，而是把“配置 -> 运行时 -> 会话/MCP”这条链路真正闭合。

## 目标

Phase 14 只做一个子项目：**配置驱动的 session persistence + MCP bootstrap**。

交付完成后应满足：

1. `ccc chat` 默认自动恢复当前项目最近一次会话。
2. `ccc chat --print` 默认启动新会话，不读取也不写入持久化 session。
3. chat 启动时会根据现有配置装配并启动 MCP server。
4. `ProjectConfig.last_session_id` 会在交互会话完成后被正确写回。
5. TUI 与共享 runner 继续走同一条执行链路，只在启动方式和事件消费端不同。

## 非目标

Phase 14 明确不做：

- 不新增 resume/list/fork 等显式 CLI 参数。
- 不做多会话选择器或 transcript 浏览 UI。
- 不做 plugin、remote、policy、enterprise MCP。
- 不做新的配置格式，也不重写完整配置中心。
- 不改变 `chat --print` 的 stdout/stderr 协议；结构化非交互输出留到 Phase 15。

## 范围边界

本轮只执行 Phase 14。Phase 15 和 Phase 16 仅保留为后续 roadmap，不在本 phase 内写实现。

用户已确认的行为决策：

- `ccc chat`：默认自动恢复最近会话。
- `ccc chat --print`：默认新会话。
- 架构边界：`ccc-cli` 负责运行时配置装配，`ccc-agent` 负责持久化会话和 MCP bootstrap。

## 核心设计

### 1. 新增运行时装配层

`ccc-cli` 新增一个内部运行时装配模块，例如 `runtime.rs` 或 `config_runtime.rs`，负责把以下来源合并成一个 `ChatRuntimeConfig`：

- CLI 参数：`--model`、`--system-prompt`
- 全局配置：`GlobalConfig`
- 当前项目视图：`ProjectConfig`

`ChatRuntimeConfig` 至少包含：

- `model: String`
- `system_prompt: Option<String>`
- `project_key: String`
- `session_mode: SessionMode`
- `mcp_servers: Vec<(String, McpServerConfig)>`

其中：

- `SessionMode::ResumeLast` 仅用于交互 `ccc chat`
- `SessionMode::Ephemeral` 仅用于 `ccc chat --print`

### 2. 在 `ccc-agent` 引入持久化会话层

`ccc-agent` 新增 session persistence 抽象，例如：

- `PersistedSession`
- `SessionStore`
- `SessionBootstrap`

职责划分：

- `SessionStore`：负责读写 session transcript 文件
- `SessionBootstrap`：负责根据 `SessionMode`、`last_session_id` 和运行时配置构造 `SessionRunner`
- `SessionRunner`：继续负责真正的消息循环，不承担路径解析和配置合并

`PersistedSession` 建议字段：

- `version: u32`
- `session_id: SessionId`
- `cwd: String`
- `model: String`
- `system_prompt: Option<String>`
- `messages: Vec<Message>`

新会话的 `SessionId` 在 Phase 14 固定使用 UUID v4 生成，并继续通过 `ccc_core::ids::SessionId` 这个包装类型在 crate 间流转。

Phase 14 不需要保存 UI 状态、tool execution cache、file snapshots 或统计指标。

### 3. 统一 session 存储位置

Phase 14 统一复用 `CLAUDE_CONFIG_DIR` / `~/.claude` 作为持久化根目录。

建议目录结构：

- `<claude_config_dir>/sessions/<session_id>.json`

不在本 phase 内引入额外的项目级 session index 文件。最近会话指针直接使用 `GlobalConfig.projects[project_key].last_session_id`。

原因：

- Rust 已经有统一配置根目录约定。
- `ProjectConfig.last_session_id` 已经存在，不需要再发明第二套“最近会话”索引。
- Phase 14 只需要单项目最近会话恢复，不需要多会话浏览器。

### 4. `ccc chat` 的恢复语义

交互模式启动流程固定如下：

1. `ccc-cli` 装配 `ChatRuntimeConfig`
2. 若 `project.last_session_id` 存在，则尝试加载 `<config_dir>/sessions/<id>.json`
3. 若文件存在且可解析：
   - 用已持久化 `messages` 初始化 `SessionRunner`
   - 默认继续使用持久化 session 中的 `model/system_prompt`
   - 若 CLI 显式传了 `--model` 或 `--system-prompt`，CLI 显式值覆盖持久化值
4. 若文件不存在或损坏：
   - 记录一条 warning 级 telemetry/event
   - 回退为新会话，不让启动失败

Phase 14 不做“用户提示是否恢复”，也不做 resume picker。

### 5. `ccc chat --print` 的语义

`--print` 路径固定为 `SessionMode::Ephemeral`：

- 不读取 `last_session_id`
- 不写入 session transcript
- 不更新 `ProjectConfig.last_session_id`

原因：

- 保持脚本行为稳定和可预测
- 避免一次非交互调用隐式污染后续交互会话
- 为 Phase 15 的结构化协议留出清晰边界

### 6. MCP bootstrap 语义

Phase 14 只消费现有配置字段：

- `GlobalConfig.mcp_servers`
- `ProjectConfig.enabled_mcp_json_servers`
- `ProjectConfig.disabled_mcp_json_servers`
- `ProjectConfig.enable_all_project_mcp_servers`

启用规则固定为：

1. 候选集合来自 `GlobalConfig.mcp_servers`
2. 对每个 server name 计算状态：
   - 若命中 `disabled_mcp_json_servers`，状态为 disabled
   - 否则若命中 `enabled_mcp_json_servers`，状态为 enabled
   - 否则若 `enable_all_project_mcp_servers == true`，状态为 enabled
   - 否则状态为 disabled
3. Phase 14 只启动状态为 enabled 的 server

这意味着：

- `disabled` 优先级最高
- `enable_all_project_mcp_servers` 是项目级“默认全开”，但仍可被 `disabled` 覆盖
- 未显式启用且未全开的 server 不启动

Phase 14 不处理：

- project-local `.mcp.json`
- plugin 注入的 MCP server
- enterprise policy gating
- 首次审批对话框

### 7. 运行时写回策略

交互会话在每轮 assistant 完成后持久化最新 transcript；结束时确保最近一次成功状态已落盘。

同时更新 `GlobalConfig.projects[project_key].last_session_id`：

- 新会话：写入新生成的 `SessionId`
- 恢复会话：保持原 `SessionId`

Phase 14 先不写入 cost、duration 等统计字段，只保证 `last_session_id` 正确。

## 模块拆分

建议新增或调整以下边界：

### `ccc-cli`

- 新增 chat runtime 装配模块
- chat 命令先构造 `ChatRuntimeConfig`
- 再根据 `SessionMode` 调用：
  - 交互：`ccc_tui::run_app(config)`
  - 非交互：`SessionRunner` 的 ephemeral 启动

### `ccc-agent`

- 保留 `Agent` 和 `SessionRunner`
- 新增持久化 session store 模块
- 给 `SessionRunner` 增加“从既有 messages 启动”的能力
- 新增 MCP bootstrap 帮助函数，用一组 `(name, config)` 批量调用 `add_mcp_server`

### `ccc-tui`

- `AppConfig` 扩展为接收：
  - `model`
  - `system_prompt`
  - `initial_messages`
  - `session_id`
  - `mcp_servers`
- TUI 不自行决定是否恢复，只消费 CLI 传入的启动状态

### `ccc-core`

- 现有 `GlobalConfig` / `ProjectConfig` 类型不需要新增字段
- 可新增小型辅助函数，但不引入新的重配置框架

## 失败模式

Phase 14 对失败的处理固定如下：

- session 文件不存在：回退新会话
- session 文件 JSON 损坏：记录错误并回退新会话
- session 文件版本不兼容：记录错误并回退新会话
- 某个 MCP server 启动失败：
  - 记录错误
  - 不阻止 chat 主流程启动
  - 继续启动剩余可用 server
- 配置文件不存在：使用默认配置视图

只有以下情况应直接失败：

- CLI 参数非法
- 配置 JSON 无法整体解析
- API/认证链路本身无法启动

## 测试策略

### `ccc-agent`

- session roundtrip：保存后再加载，`messages/model/system_prompt` 保持一致
- `SessionRunner` 可从已有 `messages` 启动
- 批量 MCP bootstrap 会调用 enabled server，并跳过 disabled server

### `ccc-cli`

- 交互 runtime 装配测试：
  - 有 `last_session_id` 时走 `ResumeLast`
  - `--model` / `--system-prompt` 覆盖持久化值
- `--print` 路径始终走 `Ephemeral`
- MCP enable/disable precedence 测试
- 配置写回 `last_session_id` 测试

### `ccc-tui`

- `run_app` 能接收初始消息和 session id
- 恢复启动路径不破坏现有自定义 model 测试

### workspace 验证

- `cargo test`
- 一个 resume smoke：
  - 第一次交互会话写出 session 文件
  - 第二次交互会话能从 `last_session_id` 恢复
- 一个 `--print` smoke：
  - 执行后不生成或不更新 `last_session_id`

## 验收标准

满足以下条件即可认为 Phase 14 完成：

1. `ccc chat` 默认恢复最近一次交互会话。
2. `ccc chat --print` 默认不参与持久化 session。
3. MCP server 会根据现有配置字段在 chat 启动前完成 bootstrap。
4. `GlobalConfig.projects[project_key].last_session_id` 会被正确读写。
5. TUI 和交互 runner 仍然共享同一条消息执行链路。
6. `cargo test` 通过。

## 对 Phase 15/16 的边界预留

Phase 14 完成后，后续阶段边界保持如下：

- Phase 15：在不改变 Phase 14 持久化模型的前提下，增强 `--print` 的输入/输出协议和结构化事件格式
- Phase 16：在不重写 Phase 14 runtime 装配层的前提下，向其追加 policy/plugin/remote/enterprise MCP 规则
