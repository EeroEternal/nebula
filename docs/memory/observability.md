# Nebula 可观测性方案

## 当前方案：xtrace 自研轻量级方案

已从 Prometheus + Grafana 转向自研轻量级方案 xtrace（/Users/home/github/xtrace）。

### 术语与职责边界（统一口径）

- **数据层（Backend）**：xtrace / nebula-observe 负责采集、存储、查询（metrics/traces API）。
- **展示层（Frontend）**：Nebula 前端负责 Dashboard/Panel（图表、卡片、告警态、表格）。
- 结论：xtrace 本身不作为最终产品化 UI；“观测面板”属于前端能力。

### 已完成的集成

1. **nebula-observe** — 内嵌 xtrace 服务的独立组件（依赖 xtrace 0.0.7 的 run_server），已部署在 10.21.11.92:8742
2. **nebula-node** — 心跳循环中通过 xtrace-client push_metrics 上报 GPU/KV cache/pending requests 指标
3. **OTLP tracing** — gateway/node/router 三个组件通过 nebula-common::telemetry::init_tracing 统一接入 OTLP/HTTP exporter，将 tracing spans 推送到 xtrace 的 /api/public/otel/v1/traces 端点
4. 所有组件支持 --xtrace-url / --xtrace-token 参数（环境变量 OBSERVE_URL / OBSERVE_TOKEN）

### xtrace 现有能力

- Trace/Observation 采集（Langfuse 兼容 + OTLP/HTTP）
- Metrics 时序采集（POST /v1/metrics/batch）
- PostgreSQL 存储，异步微批写入
- 查询 API：traces 列表、trace 详情、metrics query/names
- 为前端观测面板提供数据查询后端（而非面板 UI 本体）

### Nebula 上报的指标

- gpu_utilization, gpu_temperature, gpu_memory_used_mb, gpu_memory_total_mb（来自 GpuStatus）
- kv_cache_usage, pending_requests, prefix_cache_hit_rate（来自 EndpointStats）

### OpenTelemetry 依赖版本

opentelemetry 0.27, opentelemetry_sdk 0.27, opentelemetry-otlp 0.27, tracing-opentelemetry 0.28

## 早期设计（已部分被 xtrace 替代）

### 统一 Metrics 暴露

每个组件（Node、Router、Gateway）暴露 `/metrics` endpoint（Prometheus 格式）：

- **Node**: nebula_container_status, nebula_container_restart_total, nebula_gpu_temperature, nebula_gpu_utilization, nebula_engine_kv_cache_usage, nebula_engine_pending_requests
- **Router**: nebula_route_total, nebula_route_latency_seconds
- **Gateway**: nebula_request_total, nebula_request_concurrent

### 设计原则

- etcd 只存控制面数据（placement、endpoint、stats 用于实时路由决策）
- 可观测性数据（metrics、logs、traces）走专用系统，不存 etcd
- Engine Stats Pipeline 目标形态仍是写入 etcd `/stats/` 供 Router/Scheduler 实时决策，同时通过 xtrace/Prometheus 暴露历史观测；当前代码已具备 Node scrape 和 xtrace 推送，`/stats/` 控制面热路径仍需收敛。
