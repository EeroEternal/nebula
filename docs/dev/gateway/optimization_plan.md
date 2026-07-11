# Nebula Gateway 优化方案（更新版）

> 范围约束：本文只描述 Gateway 与 Router 的入口、防护、代理边界，不引入 provider pool 语义，不改变 Nebula 以 placement、endpoint、stats 为核心的调度模型。

## 1. 当前状态

早期 P0 防护能力已经基本落地：

- Router 已支持可配置重试、失败 endpoint 排除、endpoint 熔断。
- Router 和 Gateway 已支持请求体大小上限。
- Router 和 Gateway 已暴露 `/metrics`。
- Router 已有上游错误分类、重试、熔断、stale stats、TTFT/E2E 指标。
- Gateway 已有请求量、状态码、鉴权、请求体超限、上游错误指标。
- Gateway 已接入共享 auth middleware 和审计 middleware。

当前剩余风险主要集中在三个方向：

- Gateway 默认鉴权策略偏开发友好，生产安全默认不足。
- Gateway、Router、BFF、UniGateway 边界仍需固定。
- Router 为了解析 `model` 仍会完整缓冲请求体。

## 2. 目标边界

推荐职责划分：

| 组件 | 职责 |
|------|------|
| Gateway | 外部入口、OpenAI-compatible HTTP/SSE、鉴权、审计、错误映射、请求上下文提取 |
| Router | endpoint 选择、重试、熔断、过载保护、上游代理、stats 驱动策略 |
| BFF | 控制台 API、用户/session、模型管理视图 |
| UniGateway | 可选协议/转发库，不拥有 Nebula 集群路由决策 |

Gateway 可以代理 BFF API，但同一类 API 只能有一个 owner。Gateway 不应绕过 Router 直接做 endpoint 选择或集群重试。

## 3. P0：安全默认值

### P0-1 鉴权 fail-closed（已完成）

当前共享 auth 已调整为：

- 默认必须配置 `NEBULA_AUTH_TOKENS`。
- 仅当显式设置 `NEBULA_AUTH_DISABLED=true` 或 `NEBULA_DEV_AUTH_DISABLED=true` 时才允许免鉴权。
- 启动日志输出 `auth_mode=enabled|disabled_for_dev`。
- `/metrics` 保留 auth missing、invalid、forbidden、rate_limited 指标。

验收标准：

- 未配置 token 且未显式关闭 auth 时，受保护路由返回 401/启动失败。
- 配置 token 后，Bearer token 与 `x-api-key` 均可正常鉴权。
- 显式关闭 auth 时日志有清晰告警。

## 4. P1：请求路径收敛

### P1-1 Gateway 注入模型路由 header

Router 当前需要读取完整 body 才能解析 `model`。建议 Gateway 在入口解析一次请求体，注入：

- `X-Nebula-Model`
- 或已解析后的 `X-Nebula-Model-Uid`
- `X-Nebula-Request-Id`
- 可选 `X-Nebula-Session-Id`

Router 优先使用 header 做路由决策，body 只作为透传流处理。

验收标准：

- Router 在 header 存在时不再解析完整 body。
- 长上下文请求不会因为路由解析额外复制大 body。
- 缺 header 的兼容路径仍受 `NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES` 限制。

### P1-2 统一推理代理路径

推荐推理路径固定为：

```text
Client -> Gateway -> Router -> Engine
```

Gateway 做入口校验、审计和错误映射；Router 负责 endpoint 选择、重试、熔断和代理。不要保留 “Gateway 先 pick endpoint 再自己代理 Engine” 的双轨路径。

验收标准：

- `/v1/chat/completions`、`/v1/responses`、`/v1/embeddings`、`/v1/rerank` 的 endpoint 选择都经过 Router。
- Gateway metrics 只反映入口和代理到 Router 的状态。
- Router metrics 负责上游 Engine 错误、retry、circuit 和 route latency。

## 5. P2：防护能力增强

### P2-1 超时分层

当前 timeout 仍偏粗。建议拆为：

- `NEBULA_GATEWAY_CONNECT_TIMEOUT_MS`
- `NEBULA_GATEWAY_REQUEST_TIMEOUT_MS`
- `NEBULA_ROUTER_CONNECT_TIMEOUT_MS`
- `NEBULA_ROUTER_FIRST_BYTE_TIMEOUT_MS`
- `NEBULA_ROUTER_REQUEST_TIMEOUT_MS`

首包超时尤其重要，用于识别 engine hang 或 SSE 长时间无响应。

### P2-2 背压从“硬拒绝”走向“候选缩减”

Router 已能在 endpoint 全部过载时返回 429。下一步建议保留现有行为，同时补更细指标：

- `nebula_router_admission_reject_total{reason}`
- `nebula_router_route_fallback_total{reason}`
- `nebula_router_candidate_dropped_total{reason}`

### P2-3 调参文档化

需要为以下参数补默认值、推荐区间和回滚条件：

- `NEBULA_ROUTER_RETRY_MAX`
- `NEBULA_ROUTER_RETRY_BACKOFF_MS`
- `NEBULA_ROUTE_CIRCUIT_FAILURE_THRESHOLD`
- `NEBULA_ROUTE_CIRCUIT_OPEN_MS`
- `NEBULA_ROUTE_STATS_MAX_AGE_MS`
- `NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES`
- `NEBULA_GATEWAY_MAX_REQUEST_BODY_BYTES`

## 6. 验收矩阵

| 场景 | 预期 |
|------|------|
| 未配置 auth token | 默认拒绝或启动失败 |
| 显式关闭 auth | 请求放行且日志告警 |
| 单 endpoint 5xx | Router 记录 upstream_5xx，并按配置重试 |
| 双副本一坏一好 | 重试排除坏副本并命中健康副本 |
| 连续失败超过阈值 | endpoint 熔断，窗口内不参与路由 |
| 超大请求体 | Gateway/Router 返回 413 |
| 全部 endpoint 过载 | 返回 429 和 `Retry-After` |
| 长上下文请求 | Router header-driven routing，不 full buffer body |
| SSE 首包过慢 | 首包超时指标或错误可观测 |

## 7. 当前建议排期

| 优先级 | 项目 | 预估 |
|--------|------|------|
| P0 | 鉴权默认 fail-closed | 已完成 |
| P1 | Gateway 注入模型 header，Router header-driven routing | 2-3 天 |
| P1 | 推理代理路径收敛 | 1 天 |
| P2 | 超时分层 | 1 天 |
| P2 | admission/fallback/candidate 指标补全 | 1 天 |
| P2 | 调参文档与 runbook 更新 | 0.5 天 |

## 8. 架构边界说明

本方案保留 Nebula 现有内部调度模型：

- 不引入 provider pool 语义。
- 不让 Gateway 拥有 endpoint 选择权。
- 不让 UniGateway 绕过 Router 的 placement、endpoint、stats 语义。
- 不把 xtrace 作为 Router/Scheduler 实时决策的唯一依赖。
