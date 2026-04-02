# Phase 16A Policy-Aware MCP Bootstrap Design

## 背景

截至 `865ff55`，Rust 侧的 MCP bootstrap 仍然停留在 Phase 14 模型：

- `McpServerConfig` 只有 `stdio` 形态。
- `ChatRuntimeConfig.mcp_servers` 只是 `Vec<(String, McpServerConfig)>`。
- `ccc-cli::runtime::select_mcp_servers(...)` 只理解：
  - `enabled_mcp_json_servers`
  - `disabled_mcp_json_servers`
  - `enable_all_project_mcp_servers`

这个模型足够支撑 Phase 14 的最小闭环，但已经无法覆盖未来需求：

- plugin 与 builtin plugin 提供的 MCP 无法纳入同一候选集。
- managed allowlist / denylist 没有统一注入点。
- `strictPluginOnlyCustomization` 无法在 MCP surface 生效。
- Phase 15 的 `system/init` / `system/warning` 已经有稳定输出协议，但当前 bootstrap 没有稳定的“被拦截/被禁用/被计划启动”中间层。

因此，Phase 16A 的目标是先把 **本地 bootstrap 选择器** 写死：把“哪些 server 允许进入启动计划”这件事做成统一、可观测、可扩展的装配层。

## Goals

Phase 16A 必须完成以下设计目标：

1. 把 `McpServerConfig` 从纯 `stdio` 扩展为 tagged union，并为后续 remote transport 预留所有必要 variant。
2. 把以下来源统一收敛到一个 source graph：
   - 全局 settings
   - 项目 settings
   - 本地 settings
   - builtin plugin 提供的 MCP
   - 已启用 plugin 提供的 MCP
3. 把以下 policy 收敛到一个统一的 gating pipeline：
   - managed allowlist / denylist
   - `allowManagedMcpServersOnly`
   - `strictPluginOnlyCustomization` 对 `mcp` surface 的限制
   - built-in MCP default-disabled 语义
4. 产出一个可复用的 `McpBootstrapPlan`，并保证交互 `chat` 与 Phase 15 headless 路径消费同一份 plan。
5. 被 policy 拦截、被用户禁用、被 builtin 默认关闭的 server 不能静默消失，必须可转化为 warning / status。

## Non-goals

Phase 16A 明确不做：

- 不实现 `sse/http/ws/sdk/claudeai-proxy` 的真实连接逻辑。
- 不实现 remote managed settings 拉取。
- 不实现 enterprise-exclusive MCP 文件覆盖。
- 不设计新的 CLI 交互命令或 control plane 操作。
- 不重写通用 plugin system，只消费“已启用 plugin 提供的 MCP 定义”。

## 配置模型演进

### 1. `McpServerConfig` 升级为 tagged union

Rust 侧的 canonical 结构改为：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum McpServerConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        headers_helper: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        headers_helper: Option<String>,
    },
    Ws {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        headers_helper: Option<String>,
    },
    Sdk {
        name: String,
    },
    ClaudeAiProxy {
        url: String,
        id: String,
    },
}
```

兼容性约束：

1. 反序列化仍然接受“无 `type` 字段”的旧结构，并视为 `Stdio`。
2. 序列化时统一输出显式 `type`，避免后续 transport 扩展时出现歧义。
3. `16A` 不要求 `ccc-mcp` 已经能连接所有 variant，但 selector 必须能够识别并保留 transport 信息。

### 2. 配置字段演进

为支持非 `.mcp.json` 来源与 builtin 默认关闭语义，Rust 配置需要从 Phase 14 的 legacy 字段逐步演进到 canonical 字段：

- 现有 legacy：
  - `enabled_mcp_json_servers`
  - `disabled_mcp_json_servers`
  - `enable_all_project_mcp_servers`
- 新的 canonical：
  - `enabled_mcp_servers`
  - `disabled_mcp_servers`

设计决策：

1. `16A` 引入 canonical 字段。
2. legacy 字段在读取时仍然接受，并归一化到 canonical set。
3. 如果两套字段同时存在，以 canonical 字段为准，legacy 只作为向后兼容输入。
4. `enable_all_project_mcp_servers` 保留，作为 Phase 14 行为的兼容开关。

这样做的原因是：未来 server 已经不再等价于 `.mcp.json` 的 `stdio` 条目，继续把语义绑死在 `mcp_json` 命名上会越来越误导。

## 统一 source graph

### `ResolvedMcpServer`

selector 的基础输入不再是裸 `(name, config)`，而是带来源元数据的 resolved 节点：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpSourceScope {
    Global,
    Project,
    Local,
    BuiltinPlugin,
    Plugin,
    Managed,
    Enterprise,
    Dynamic,
    ClaudeAi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedMcpServer {
    pub name: String,
    pub config: McpServerConfig,
    pub source_scope: McpSourceScope,
    pub source_label: String,
    pub plugin_source: Option<String>,
    pub dedup_signature: Option<String>,
    pub default_enabled: bool,
}
```

字段说明：

- `source_scope`：给 precedence 和 policy gate 使用。
- `source_label`：给 warning / status 展示用，例如 `~/.claude/settings.json`、`plugin:slack@anthropic`。
- `plugin_source`：只有 plugin 提供的 MCP 才有值。
- `dedup_signature`：用于 manual-vs-plugin 的内容去重，而不是只靠 server 名。
- `default_enabled`：builtin default-disabled 语义通过它表达。

### Source load order

`16A` 固定 source load order 为：

| 顺序 | 来源 | 目的 |
|---|---|---|
| 1 | `global` | 作为最低优先级的手工配置基线 |
| 2 | `project` | 覆盖全局配置 |
| 3 | `local` | 提供当前工作目录的最高优先级手工覆盖 |
| 4 | `builtin-plugin` | 纳入内建 plugin 提供的 MCP |
| 5 | `plugin` | 纳入已启用 plugin 提供的 MCP |

这里的 “load order” 只决定发现顺序，不等价于最终覆盖顺序。最终的 merge 规则见后文。

## Precedence 与过滤顺序

Phase 16A 的 gating pipeline 固定为下面 5 个阶段，顺序不可交换：

1. source load order
2. plugin-only gate
3. allowlist / denylist gate
4. user/project disable / enable gate
5. final bootstrap set

### 1. Source merge

merge 分两类处理：

#### 手工配置来源

`global/project/local` 的 server 以名称为主键合并，优先级固定为：

```text
local > project > global
```

也就是说：

- 同名 server 在 `local` 中存在时，完全覆盖 `project/global`。
- 同名 server 只在 `project` 中存在时，覆盖 `global`。

#### Plugin 来源

plugin server 不与手工配置直接做“按名称覆盖”，而是先 namespaced，再按 `dedup_signature` 去重：

1. 手工配置永远优先于 plugin。
2. builtin plugin 在 plugin 世界中先于已启用 plugin。
3. 已启用 plugin 之间遵循稳定加载顺序，先到先赢。

这样可以避免：

- 用户手工配置一个 server，却被同名 plugin 条目覆盖。
- 两个 plugin 提供的是同一底层命令或 URL，却因为命名不同而重复启动。

### 2. Plugin-only gate

若 managed policy 对 `mcp` surface 启用了 `strictPluginOnlyCustomization`，则：

- `global/project/local` 全部跳过，不进入后续候选集。
- `builtin-plugin` 与 `plugin` 仍然允许进入候选集。

这是“来源级别”的 gate，而不是 per-server 名称匹配。也就是说，一旦该策略开启，用户与项目侧的 MCP 自定义整体失效，而不是逐条判断。

### 3. Allowlist / denylist gate

policy 规则统一产出 `McpPolicyDecision`：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpPolicyDecisionKind {
    Allowed,
    BlockedByPluginOnlyPolicy,
    BlockedByDenylist,
    BlockedByAllowlist,
    DisabledByProject,
    DisabledByBuiltinDefault,
    SuppressedDuplicate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpPolicyDecision {
    pub name: String,
    pub kind: McpPolicyDecisionKind,
    pub message: String,
}
```

allow / deny 规则必须支持三类匹配：

- `server name`
- `stdio command array`
- `remote server url pattern`

优先级固定如下：

1. denylist 绝对优先
2. 若存在 allowlist：
   - `stdio` 服务器优先按 `command` 规则匹配
   - remote 服务器优先按 `url` 规则匹配
   - 若对应维度没有专门规则，则回退到 name 规则
3. 若 allowlist 未定义，则默认允许
4. 若 allowlist 定义为空数组，则默认全部拒绝

`allowManagedMcpServersOnly` 的语义固定为：

- allowlist 只从 managed settings 读取
- denylist 仍然从全部来源合并

这样既保留“管理员决定什么允许启动”，又允许用户继续“为自己禁用某台 server”。

### 4. User / project enable / disable gate

在通过 policy 之后，才应用项目级开关。语义固定为：

#### 普通 server

- 如果名称命中 `disabled_mcp_servers`，则最终状态为 disabled
- 否则如果名称命中 `enabled_mcp_servers`，则最终状态为 enabled
- 否则如果 `enable_all_project_mcp_servers == true`，则最终状态为 enabled
- 否则：
  - 手工配置 server 默认 enabled
  - builtin default-disabled server 默认 disabled

#### Builtin default-disabled server

builtin 默认关闭的 server 采用显式 opt-in：

- 不受 `enable_all_project_mcp_servers` 自动放开
- 只有名称命中 `enabled_mcp_servers` 才进入 enabled
- 否则记为 `DisabledByBuiltinDefault`

这样可以保留“某些高风险或重量级 builtin MCP 必须用户主动启用”的语义。

### 5. Final bootstrap set

最终 selector 产出：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedMcpServer {
    pub server: ResolvedMcpServer,
    pub initial_status: McpConnectionStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockedMcpServer {
    pub server: ResolvedMcpServer,
    pub decision: McpPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpBootstrapPlan {
    pub planned: Vec<PlannedMcpServer>,
    pub blocked: Vec<BlockedMcpServer>,
    pub warnings: Vec<String>,
}
```

`initial_status` 在 `16A` 中只允许是：

- `pending`
- `disabled`

真正的 `connected/failed/needs-auth` 由 `16B` 的连接层产生。

## 交互与非交互输出行为

Phase 16A 强制要求：

1. 交互 `ccc chat` 与 `ccc chat --print` 使用同一份 `McpBootstrapPlan`。
2. 被挡下的 server 必须进入可见输出，而不是静默消失。

对外行为固定为：

- 交互路径：
  - TUI MCP 状态列表显示 `planned` 与 `disabled`
  - 被 policy 拦截的 server 在状态列表中显示为 `disabled`，并带 reason
- Phase 15 headless 路径：
  - `system/init.mcp_servers` 包含进入 plan 的 server
  - 被 policy 拦下或被 builtin 默认禁用的 server 通过 `system/warning` 和最终 `result.warnings` 暴露

这里特意不在 `system/init.mcp_servers` 里扩展另一套 planning-only 状态，是为了保持 Phase 15 协议简单。详细原因由 warning 文本承载。

## Failure modes 与降级行为

| 场景 | 行为 |
|---|---|
| 某个配置来源文件不存在 | 视为该来源为空，继续启动 |
| 某个配置来源 JSON 解析失败 | 记录 warning，跳过该来源，不阻断主流程 |
| 某个 plugin 的 MCP 清单无效 | 记录 warning，跳过该 plugin 的 MCP |
| 某个 plugin 提供的 server 与手工配置重复 | plugin server 被 `SuppressedDuplicate`，并记录 warning |
| allowlist / denylist 规则无法解析 | 视为该规则源无效，记录 warning |
| 出现未知 `type` | 视为该 server 无效，记录 warning |
| legacy 与 canonical enable/disable 字段同时存在 | 以 canonical 为准，记录一次兼容 warning |

只有以下情况应直接失败：

- 整体配置快照本身无法构造
- `ccc-cli` 无法确定当前项目 key

## 测试矩阵

### Source merge

- `global/project/local` 同名 server 的覆盖顺序正确
- builtin plugin 与 installed plugin 的稳定去重正确
- 手工配置和 plugin 的 signature 重复时，手工配置获胜

### Policy gating

- `strictPluginOnlyCustomization` 生效时，`global/project/local` 来源被整体跳过，但 plugin 来源仍可进入候选集
- denylist 同时覆盖 `name / command / url` 三类规则
- denylist 与 allowlist 冲突时 denylist 获胜
- `allowManagedMcpServersOnly` 开启时 allowlist 只读取 managed 来源，denylist 仍读取全部来源

### Enable / disable

- builtin default-disabled server 在未显式启用时保持 disabled
- builtin default-disabled server 不受 `enable_all_project_mcp_servers` 影响
- Phase 14 的 `disabled > enabled > enable_all` 行为在无 policy 时不回归
- legacy `enabled_mcp_json_servers` / `disabled_mcp_json_servers` 仍能被正确归一化

### Output behavior

- 交互与非交互路径看到的 planned server 集合一致
- 被 policy 拦下的 server 会进入 warning/status，而不是直接丢失

## 结果

Phase 16A 完成后，Rust MCP bootstrap 将首次具备一个明确的“选择器层”：

- 它不再只是一个 `Vec<(String, McpServerConfig)>`
- 它可以表达来源、策略决策、默认启用状态和兼容字段
- 它为 `16C` 的 enterprise / managed 注入和 `16B` 的 transport lifecycle 提供稳定输入

这也是整个 Phase 16 spec pack 的基础。如果没有这一层，后续所有 enterprise、plugin、remote transport 逻辑都只能继续堆在 `ccc-cli::runtime` 的临时判断里，技术债会快速恶化。
