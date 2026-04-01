# Phase 9: ccc-telemetry — 观测与遥测

## 目标
集成 OpenTelemetry 协议，实现分布追踪、日志聚合与性能监控。

## 核心功能
1. **Tracing (追踪)**：
   - 记录 Agent 思考、工具执行与 API 调用的耗时。
   - 支持导出到本地文件或 Honeycomb/Jaeger。
2. **Logging (日志)**：
   - 统一使用 `tracing` crate 替代 `println!`。
   - 支持分级别过滤日志输出。
3. **Error Reporting**：
   - 异常崩溃时的自动上下文收集与快照生成。

## 依赖项
- `tracing`, `tracing-subscriber`
- `opentelemetry`
