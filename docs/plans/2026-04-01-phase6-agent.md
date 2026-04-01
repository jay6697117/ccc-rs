# Phase 6: ccc-agent — AI 代理编排与循环

## 目标
构建核心逻辑中枢，协调 AI 模型、工具执行与上下文管理。

## 核心功能
1. **消息循环 (Message Loop)**：
   - 实现传统的 Thinking-Action-Observation 循环。
   - 令牌限制管理与自动上下文压缩。
2. **工具注册表 (Tool Registry)**：
   - 聚合本地工具 (`ccc-tools`) 与远程 MCP 工具。
   - 自动生成符合 JSON Schema 的定义给 AI。
3. **流式事件分发**：
   - 将 AI 的输出（Thinking/Content/Tool Use）实时分发给前端。
4. **记忆系统**：
   - 持久化本地 Memory 文件，支持跨对话上下文存储。

## 依赖项
- `ccc-api` (模型调用)
- `ccc-mcp` (外部工具扩展)
- `ccc-tools` (内置工具)
