# Nebula 优化计划（当前代码基线）

> 本文基于当前代码状态更新。旧版计划中部分能力已经落地，后续优化重点从“补功能”转为“收敛边界、巩固控制面热路径、补关键测试”。

## 当前判断

Nebula 的主方向仍然成立：Rust 控制面、Python/推理引擎执行面、etcd 作为权威状态源，Scheduler 写期望状态，Node reconcile 实际状态，Router 基于 endpoint 与 stats 做请求路由。

当前 `cargo check --workspace` 已通过。旧文档中 “Gateway 编译未闭合” 已不是当前 P0。现阶段更重要的问题是：

| 领域 | 当前状态 | 下一步重点 |
|------|----------|------------|
| Gateway | 已有鉴权、审计、metrics、请求体上限、BFF 代理 | 默认安全策略与职责边界收敛 |
| Router | 已有策略插件化、重试、熔断、过载保护、TTFT/E2E metrics | 避免 full body buffer，稳定 stats 热路径 |
| Scheduler | 已有周期 reconcile，部分 CAS 更新 | 所有 placement 写路径统一 CAS |
| Node | 已有 GPU 增强、引擎健康检查、stats scrape、xtrace metrics push | 明确 stats 控制面存储契约 |
| Observability | 已有 `/metrics` 与 trace context 注入/提取 | 统一 BFF 初始化与面板/告警口径 |

## 已落地能力

### Router 防护与策略

- `RoutingStrategy` 已拆出，支持 `least_pending`、`least_kv_cache`、`prefix_cache_aware`。
- Router 会过滤非 Ready endpoint、plan_version 不匹配 endpoint、熔断中的 endpoint。
- 已支持一次或多次可配置重试，并在重试时排除首次失败 endpoint。
- 已有请求体大小上限、429 admission control、E2E latency 与 TTFT histogram。
- `/metrics` 已暴露 Router 请求量、状态码、重试、上游错误、熔断、stale stats 等指标。

### Node 健康与信号

- `nvidia-smi` 已采集显存、温度、GPU 利用率。
- heartbeat loop 已执行引擎 health check，连续失败后标记 Unhealthy，并在更高阈值后尝试 restart。
- Node 已 scrape 引擎 stats，并推送 pending、KV cache usage、prefix cache hit rate 到 xtrace。
- Node API 已暴露 Prometheus metrics snapshot。

### Scheduler reconcile

- Scheduler 已有周期 reconcile loop，能基于 endpoint 状态和负载信号做自愈与扩缩容尝试。
- `reconcile.rs` 中 placement 更新已使用 `compare_and_swap`。
- 已支持 `min_replicas` / `max_replicas` 风格的副本边界。

### Gateway 基础防护

- Gateway 推理和 admin 路由已接入共享 auth middleware。
- Gateway 已有请求体大小上限、上游错误分类指标、请求量和状态码 metrics。
- Gateway 已注入 trace context，并暴露 `/metrics`。

## P0：必须先修的风险

### P0-1 鉴权默认改为 fail-closed（已完成）

当前 auth middleware 已默认启用鉴权。`NEBULA_AUTH_TOKENS` 未配置或没有有效 token 时，受保护路由会拒绝请求；只有显式设置 `NEBULA_AUTH_DISABLED=true` 或 `NEBULA_DEV_AUTH_DISABLED=true` 时才允许开发免鉴权。

后续只需在部署文档和 compose 示例中继续强调生产必须配置 `NEBULA_AUTH_TOKENS`。

### P0-2 明确 stats 控制面热路径

旧计划设想 Node 写 `/stats/{model_uid}/{replica_id}` 到 etcd；当前代码实际更偏向 Node 推 xtrace、Router/Scheduler 从 xtrace 拉取指标。

建议将两类数据分开：

- 控制面实时决策：写入 etcd `/stats/{model_uid}/{replica_id}`，带 TTL/lease，只保存最新值。
- 历史观测与面板：推送 xtrace/Prometheus，用于查询、趋势和告警。

这样 Router 和 Scheduler 不依赖观测后端可用性，xtrace 限流或延迟不会直接影响路由和扩缩容。

### P0-3 所有 placement 写路径统一 CAS（已完成）

`scheduler/src/main.rs` 的 placement 写路径已改为 CAS；`reconcile.rs` 也已修正 CAS 返回 `ok=false` 时被误判为成功的问题。

建议：

- 后续仍建议明确 `PlacementPlan.version` 是逻辑版本还是时间戳；推荐使用单调逻辑版本，时间戳另设字段。

### P0-4 Router 避免完整缓冲大请求体

Router 当前需要读取完整 POST body 来解析 `model` 并重写 body。长上下文、多模态或异常请求会放大内存压力。

建议：

- Gateway 在入口解析 `model`，注入 `X-Nebula-Model` 或 `X-Nebula-Model-Uid`。
- Router 优先使用 header 做路由选择。
- 对可直通请求体使用 streaming proxy，不再为路由决策 full buffer。
- body rewrite 只保留在确实需要模型名兼容时的受控路径。

## P1：架构边界收敛

### P1-1 固定 Gateway / Router / BFF 职责

建议采用以下边界：

| 组件 | 职责 |
|------|------|
| Gateway | 对外入口、OpenAI-compatible HTTP/SSE、鉴权、审计、错误映射、请求上下文提取 |
| Router | endpoint 选择、重试、熔断、过载保护、stats 驱动路由、上游代理 |
| BFF | 控制台 API、用户/session、模板、模型管理视图、前端聚合接口 |

Gateway 可以代理 BFF API，但同一类 API 只能有一个 owner。UniGateway 如继续使用，应定位为协议执行或高性能转发库，不应绕过 Router 的集群调度语义。

### P1-2 统一 telemetry/auth 初始化

Gateway、Router、Scheduler、Node 已使用 `nebula_common::telemetry::init_tracing`，BFF 仍直接初始化 `tracing_subscriber::fmt()`。

建议：

- BFF 改用统一 telemetry 初始化。
- 推理 API token、控制台 session、服务间 token 分开建模。
- auth metrics 在 shared middleware 内统一记录，避免 Gateway 单独包装后遗漏其他服务。

### P1-3 补关键架构测试

优先补以下测试：

- Router：策略选择、熔断、stale stats 降级、429 admission、header-driven routing。
- Scheduler：placement CAS 冲突、reconcile 重试、扩缩容边界。
- Node：health check 连续失败、Unhealthy 标记、restart cooldown。
- Gateway：鉴权默认策略、SSE 透传、OpenAI-compatible 错误映射。

## P2：可维护性与扩展性

### P2-1 拆分 `nebula-common`

`nebula-common` 目前同时承载领域类型、auth、telemetry、执行上下文。短期可接受，但继续膨胀会让依赖边界变重。

建议后续拆为：

- `nebula-common-types`：placement、endpoint、model request、node status。
- `nebula-common-auth`：鉴权、角色、token、middleware。
- `nebula-common-telemetry`：tracing、trace context、OTLP/xtrace 初始化。

### P2-2 收敛通用配置与 HTTP client

已有 `CommonArgs`，但各服务仍有不少 HTTP client builder 和超时配置散落。

建议：

- 提供统一 `build_http_client`。
- 对 connect timeout、first byte timeout、request timeout 分层。
- 把默认值写入文档和 `/metrics` build info。

### P2-3 观测面板与调参闭环

现有 `/metrics` 已具备基础数据，下一步应补：

- Router 按 `model_uid` 展示 5xx、TTFT、E2E、retry、circuit。
- Gateway 展示鉴权拒绝、限流、请求体超限、BFF proxy 错误。
- 将配置变更事件叠加到趋势图，方便判断参数调整效果。

## 建议执行顺序

1. 鉴权默认 fail-closed。（已完成）
2. 定稿 stats 控制面路径：etcd 最新值 + xtrace/Prometheus 历史观测。
3. Scheduler placement 写路径全量 CAS。（已完成）
4. Gateway 注入模型 header，Router 改为 header-driven routing，逐步移除 full body buffer。
5. 固定 Gateway / Router / BFF / UniGateway 边界。
6. 补 Router、Scheduler、Node、Gateway 的架构回归测试。
7. 再做 `nebula-common` 拆分和配置/HTTP client 收敛。

## 工作量估算

| 项目 | 预估 |
|------|------|
| 鉴权默认 fail-closed | 已完成 |
| stats 控制面路径定稿与实现 | 1.5-2 天 |
| placement 全路径 CAS | 已完成 |
| header-driven routing + streaming proxy | 2-3 天 |
| 边界收敛与文档同步 | 1 天 |
| 关键架构测试 | 2-3 天 |
| telemetry/auth 初始化统一 | 1 天 |

前 4 项完成后，Nebula 的生产风险会明显下降；后续优化再围绕可维护性和运维体验推进。
