# Phase 16C Enterprise / Managed MCP Design

## 背景

`16A` 定义了本地 bootstrap selector，`16B` 定义了 transport lifecycle，但还有一层更高优先级的现实约束尚未进入 Rust：**企业/托管配置会在 MCP 启动前重写候选集、限制 plugin 来源，并在某些情况下直接声明 enterprise-exclusive 的 MCP 集合。**

在 TS 参考实现中，这类能力至少覆盖：

- `managed-settings.json` 与 drop-in 文件
- remote managed settings 的缓存、资格判定和刷新
- `allowManagedMcpServersOnly`
- `strictPluginOnlyCustomization`
- `strictKnownMarketplaces` / `blockedMarketplaces`
- `allowedChannelPlugins` / `channelsEnabled`
- `managed-mcp.json` 一类 enterprise-exclusive MCP 配置

Rust 侧如果没有一个独立的 enterprise / managed spec，就会出现两个问题：

1. `16A` 被迫把“managed settings 从哪里来”也一起设计，边界过大。
2. `16B` 先实现 transport 后，再发现 enterprise policy 其实不允许某些 provider 进入候选集，只能返工。

因此，`16C` 专门负责 **managed policy 的形成与注入**，并把 enterprise-exclusive MCP 语义固定下来。

## Goals

Phase 16C 必须完成以下设计目标：

1. 定义 managed settings 文件层与 merge order。
2. 定义 remote managed settings 的 eligibility、cache、refresh 与降级路径。
3. 定义 enterprise-exclusive MCP 配置的排他语义。
4. 定义哪些 policy 在 `16A` 前注入，哪些在 `16A` 里执行。
5. 定义 plugin / marketplace / channel policy 如何影响 MCP provider 进入 `16A` 候选集。

## Non-goals

Phase 16C 明确不做：

- 不重写通用 plugin 安装、升级或 marketplace UI。
- 不实现 remote session UI。
- 不覆盖非 MCP 的 enterprise 策略面。
- 不实现 transport lifecycle；该职责仍然属于 `16B`。

## Managed settings 模型

### `ManagedSettingsSnapshot`

`16C` 的核心产物不是直接连 MCP，而是生成一份统一的 managed policy 快照，供 `16A` 使用：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ManagedSettingsFreshness {
    Fresh,
    Stale,
    Missing,
    Ineligible,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteManagedSettingsCache {
    pub uuid: String,
    pub checksum: String,
    pub fetched_at_unix_ms: i64,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedSettingsSnapshot {
    pub merged_settings: serde_json::Value,
    pub freshness: ManagedSettingsFreshness,
    pub warnings: Vec<String>,
    pub remote_cache: Option<RemoteManagedSettingsCache>,
}
```

注意：

- `16C` 只要求 Rust 先把 managed settings 做成稳定输入，不要求一开始就把每个非 MCP 字段都转成强类型。
- 与 MCP 直接相关的字段，后续再由 `ccc-core` 逐步抽成 typed schema。

## Managed settings merge order

### 文件层

Phase 16C 固定 managed settings 文件层 merge order 为：

1. `managed-settings.json`
2. `managed-settings.d/*.json`，按文件名升序依次覆盖

这与常见的 base-file + drop-in 约定一致：

- 基础文件给默认值
- drop-in 给局部团队或环境覆盖

### 远程层

若 remote managed settings 可用，则它在文件层之上覆盖：

```text
managed-settings.json
  < managed-settings.d/*.json
  < remote managed settings cache
```

设计理由：

1. enterprise 后台应拥有更高优先级的最终裁决权。
2. 本地 managed 文件仍然是无网络时的安全基线。
3. 当远程不可用时，可以自然退回文件层，而不是完全失去 enterprise policy。

### 未来平台特定来源

如后续引入 macOS MDM、Windows 注册表等平台特定 managed source，它们必须先被归一化成“文件层等价输入”，再参与同一套 merge 顺序，不能另起一套 precedence。

## Remote managed settings

### Eligibility

remote managed settings 的拉取必须先经过 eligibility gate。Rust 侧把它建模成一个独立接口，而不是把资格判断写死在 MCP 逻辑里：

```rust
pub enum RemoteManagedEligibility {
    Eligible,
    Ineligible,
    Unknown,
}
```

语义：

- `Eligible`：允许尝试远程拉取
- `Ineligible`：明确不应拉取，不产生 warning
- `Unknown`：当前环境无法确认，允许保守降级为本地 managed 文件

### Refresh 语义

远程 managed settings 刷新策略固定为：

1. 启动时优先读取本地 cache
2. 若 `Eligible`，异步尝试刷新远程数据
3. 若远程返回 `304`，保留 cache，freshness 记为 `Fresh`
4. 若远程返回新设置，更新 cache 并覆盖快照
5. 若远程失败：
   - 有 cache：使用 stale cache，freshness 记为 `Stale`
   - 无 cache：退回本地文件层，freshness 记为 `Error`

### Failure 与降级

| 场景 | 行为 |
|---|---|
| 用户无资格 | 使用文件层，不记 warning |
| 远程返回 304 | 继续使用 cache，视为 fresh |
| 远程请求失败但有 cache | 使用 stale cache，并记 warning |
| 远程请求失败且无 cache | 使用文件层，并记 warning |
| 远程返回无效 schema | 丢弃该响应，保留旧 cache 或退回文件层 |
| 认证失败 | 本次会话停止远程重试，使用 cache 或文件层 |

## Enterprise-exclusive MCP

### 配置来源

Phase 16C 预留 enterprise-exclusive MCP 文件，例如：

```text
<managed-config-root>/managed-mcp.json
```

### 排他语义

如果 enterprise-exclusive MCP 文件存在且解析成功，则：

1. 它注入一个 `enterprise` source scope 的 server 集。
2. `16A` 的候选集只允许从 `enterprise` source 进入。
3. `global/project/local/builtin-plugin/plugin` 来源全部被排除。

这是一个“源集合替换”语义，而不是“多加一个更高优先级来源”。这样可以避免企业管理员明明要求独占控制，结果用户与 plugin 仍然混入候选集。

### 与 policy 的关系

enterprise-exclusive 不是 policy 规则本身，而是 source injection 规则。因此它必须在 `16A` 之前执行。

但 enterprise servers 在进入 `16A` 后，仍然受以下规则影响：

- denylist
- allowlist
- `disabled_mcp_servers`

也就是说，“exclusive” 控制的是 **谁有资格进入候选源集合**，而不是 **进入后永远不能再被策略裁剪**。

### 保守决策

Phase 16C 明确不在本 spec 中加入“特定 SDK transport 在 exclusive mode 下可豁免”的特例。如果后续确实需要类似 carve-out，必须作为额外 phase 或单独决策提出，因为它会直接削弱 exclusive mode 的可解释性。

## Managed policy 注入到 16A 的方式

### 在 `16A` 之前注入的内容

这些内容属于 source shaping，必须在 selector 之前完成：

- enterprise-exclusive MCP source set
- plugin marketplace / channel 来源 gating
- managed settings 合并结果

### 在 `16A` 中执行的内容

这些内容属于 per-server 决策，必须在 selector 里执行：

- `allowManagedMcpServersOnly`
- `strictPluginOnlyCustomization`
- `allowedMcpServers`
- `deniedMcpServers`
- builtin default-disabled 语义

拆分原则很明确：

- `16C` 决定“哪些政策输入可用”
- `16A` 决定“这些输入如何影响每个 server 的最终命运”

## Plugin / marketplace / channel 来源限制

### Plugin provider gating

plugin 提供的 MCP 不是默认可信来源。`16C` 需要把 plugin 来源限制前置到 `16A` 之前：

1. 若 plugin 来源命中 `blockedMarketplaces`，该 plugin 不得向 `16A` 提供任何 MCP server。
2. 若存在 `strictKnownMarketplaces`，只有白名单 marketplace 的 plugin 能提供 MCP。
3. 若 plugin 处于 managed lock 状态，用户本地副本或替代来源不能覆盖该 policy 决定。

### Channel gating

`channelsEnabled` 与 `allowedChannelPlugins` 的职责只覆盖“与 channel-capable MCP provider 直接相关的信任决策”：

- 它不改变普通用户手工 MCP server 的基本 allow/deny
- 它决定某些 plugin provider 是否有资格作为 channel-capable MCP 进入候选集

换句话说，channel policy 是 plugin provider gating 的一个附加维度，而不是另一套独立的 MCP selector。

## Failure modes 与降级行为

| 场景 | 行为 |
|---|---|
| `managed-settings.json` 缺失 | 视为文件层为空 |
| 某个 drop-in 文件无效 | 记录 warning，跳过该文件 |
| remote cache 校验和不匹配 | 丢弃该 cache，退回文件层 |
| enterprise-exclusive 文件无效 | 记录 warning，忽略 exclusive mode，继续使用普通来源 |
| marketplace policy 与 plugin 元数据不兼容 | 阻断该 plugin 提供 MCP，并记录 warning |
| `allowedChannelPlugins` 设置存在但 `channelsEnabled` 未开 | 视为 channel 功能禁用，并记录 warning |

只有以下情况应直接失败整个 managed layer 初始化：

- managed settings 根路径不可解析
- 本地 cache 存储本身损坏到无法安全读取或覆盖

单个文件、单次远程刷新或单个 plugin policy 失效，都不应阻断 chat 启动。

## 测试矩阵

### Merge order

- `managed-settings.json` 与 `managed-settings.d/*.json` 按预期覆盖
- remote cache 在存在时覆盖文件层
- remote 不可用时能正确回退到 stale cache 或文件层

### Enterprise-exclusive

- enterprise-exclusive MCP 文件存在时，普通来源不会进入 `16A`
- enterprise-exclusive 文件失效时，能退回普通来源而不是整体崩溃
- enterprise-exclusive source 进入 `16A` 后仍能被 denylist 或项目 disable 裁剪

### Policy injection

- `allowManagedMcpServersOnly` 会正确改变 allowlist 来源
- `strictPluginOnlyCustomization` 与 plugin marketplace gating 可以同时成立
- 被 policy 阻断的 plugin provider 不会进入 `16A` 候选集

### Remote managed settings

- eligibility 为 `Ineligible` 时不会产生无意义 warning
- stale cache 的 warning 能进入最终状态输出
- schema 校验失败时不会污染当前有效 cache

## 结果

Phase 16C 完成后，Rust 运行时会具备一层独立的 enterprise / managed 输入整形能力：

- `16C` 负责形成 policy snapshot 与 enterprise source
- `16A` 负责把这些输入应用到每个 server 的命运上
- `16B` 负责连接真正被允许启动的 transport

这样 enterprise 逻辑不再需要零散埋在 `ccc-cli::runtime`、plugin loader 和 transport connector 里，也为后续更复杂的 org policy 扩展留出了清晰边界。
