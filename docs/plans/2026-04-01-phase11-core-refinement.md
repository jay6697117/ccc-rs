# Phase 11: ccc-core Refinement — 配置分层与优化

## 目标
实现高性能的配置管理系统，支持多源配置合并与动态热加载。

## 核心功能
1. **配置分层 (Configuration Layering)**：
   - 优先级：CLI 参数 > 环境变量 > 项目配置文件 (`.claudecode.json`) > 全局配置文件 (`~/.claude/config.toml`)。
2. **File Watching**：
   - 实时监听配置文件变动，动态更新 TUI 样式或 Agent 行为。
3. **依赖注入优化**：
   - 使用 `shuttle` 或类似的模式简化各 crate 间的服务传递。

## 依赖项
- `figment` (配置引擎)
- `notify` (文件监听)
