# Nebula 架构优化建议（更新版）

## 总体判断

Nebula 当前架构方向合理，且比早期分析时更接近可运行闭环：`cargo check --workspace` 已通过，Gateway、Router、Scheduler、Node、BFF 都具备基本服务能力。旧版文档中关于 Gateway 编译失败、缺少基础 metrics、Router 无熔断/重试等结论已经部分过期。

当前主要风险不再是“缺少组件”，而是几个关键边界和热路径还没有完全收敛：

- Gateway、Router、BFF、UniGateway 的职责边界仍需固定。
- Router/Scheduler 的 stats 决策路径不应强依赖 xtrace 这类观测后端。
- Scheduler 仍存在非 CAS 的 placement 写路径。
- 推理入口鉴权默认值偏开发友好，生产默认不够安全。
- Router 仍会为解析 `model` 缓冲完整请求体。

## 当前已完成的关键改进

### Gateway

- 已接入共享 auth middleware。
- 已有 `/metrics`，包含请求量、状态码、鉴权、请求体超限、上游错误等指标。
- 已有请求体大小上限。
- 已注入 trace context。
- 已代理部分 BFF v2 API。

### Router

- 路由策略已插件化，支持 `least_pending`、`least_kv_cache`、`prefix_cache_aware`。
- 已支持 endpoint Ready 状态过滤、plan_version 过滤、session affinity。
- 已有可配置重试、失败 endpoint 排除、短时熔断。
- 已有 overloading admission control 和 stale stats 降级。
- 已暴露 Router metrics、E2E latency、TTFT histogram。

### Scheduler

- 已有周期 reconcile loop。
- `reconcile.rs` 中 placement 更新已使用 CAS。
- 已支持健康自愈和基于负载信号的副本调整逻辑。

### Node

- 已采集 GPU 显存、温度、利用率。
- 已执行引擎健康检查，连续失败后标记 Unhealthy，并尝试 restart。
- 已 scrape 引擎 stats 并推送 xtrace。
- 已提供 Node API metrics snapshot。

## P0：当前最高优先级

### P0-1 鉴权默认策略改为生产安全（已完成）

当前共享 auth 已默认 fail-closed。`NEBULA_AUTH_TOKENS` 未设置或没有有效 token 时，受保护路由会拒绝请求；只有显式设置 `NEBULA_AUTH_DISABLED=true` 或 `NEBULA_DEV_AUTH_DISABLED=true` 时才进入开发免鉴权模式。

建议：

- docker-compose、部署文档和示例 env 必须继续提供安全配置。

### P0-2 统一 stats 控制面契约

Router 和 Scheduler 的实时决策应依赖控制面状态，不应依赖 xtrace 查询链路。

建议：

- Node 写 etcd `/stats/{model_uid}/{replica_id}`，使用 TTL/lease。
- Router watch `/stats/`，用于路由策略、过载保护、stale stats 判断。
- Scheduler 读取 `/stats/`，用于扩缩容。
- xtrace/Prometheus 只作为历史观测和面板查询，不作为唯一决策源。

### P0-3 placement 更新路径全部 CAS 化（已完成）

`scheduler/src/main.rs` 的 placement 写路径已改为 CAS；`scheduler/src/reconcile.rs` 已检查 CAS 返回的 `ok=false` 冲突结果，不再误判为成功。

建议：

- 明确 `PlacementPlan.version` 语义，避免“时间戳版本”和“逻辑版本”混用。

### P0-4 Router 改为 header-driven routing

Router 现在为了获取 `model` 会完整读取 POST body。这个实现简单，但对长上下文、多模态和恶意大 body 不友好。

建议：

- Gateway 解析请求体中的 `model`，注入 `X-Nebula-Model` 或 `X-Nebula-Model-Uid`。
- Router 优先使用 header 做 endpoint 选择。
- Router 对请求体使用 streaming proxy。
- 保留最大 body 限制作为最后防线。

## P1：边界与一致性收敛

### P1-1 明确 Gateway / Router / BFF / UniGateway 关系

推荐边界：

| 组件 | Owner |
|------|-------|
| Gateway | 外部协议、鉴权、审计、错误映射、请求上下文 |
| Router | endpoint 选择、重试、熔断、过载保护、上游代理 |
| BFF | 控制台 API、用户/session、模型管理视图 |
| UniGateway | 可选协议/转发库，不拥有 Nebula 集群调度语义 |

如果 UniGateway 要替代 Router，应另写迁移计划；如果只是嵌入 Gateway，则不能绕过 Router 的 placement、endpoint、stats 语义。

### P1-2 收敛 Gateway 与 BFF API owner

现在 Gateway 内有 admin API，也代理 BFF v2 API。短期可接受，但需要避免同一资源由两个服务各自实现。

建议选择一种模式：

- 模式 A：Gateway 只暴露推理入口和少量运维只读 API，BFF 负责控制台 API。
- 模式 B：Gateway 做统一入口，BFF 完全内网化，控制台 API 全部经 Gateway 转发。

无论选择哪种，同一类 API 只能有一个 owner。

### P1-3 统一 telemetry 与 auth 初始化

BFF 仍直接使用 `tracing_subscriber::fmt()`，其他 Rust 服务多使用 `nebula_common::telemetry::init_tracing`。

建议：

- BFF 改为统一 telemetry 初始化。
- trace context 在 Gateway、Router、BFF、Node 间统一传递。
- 推理 token、控制台 session、服务间 token 分离。

## P2：测试与维护性

### P2-1 架构回归测试

优先补：

- Router：策略选择、熔断恢复、stale stats、overload 429、header-driven routing。
- Scheduler：CAS 冲突、reconcile 并发、扩缩容边界。
- Node：健康失败、Unhealthy 上报、restart cooldown。
- Gateway：鉴权默认策略、SSE 透传、BFF proxy 错误映射。

### P2-2 拆分过重的 `nebula-common`

短期先保持现状，避免大重构打断主线。后续可以拆分为：

- `nebula-common-types`
- `nebula-common-auth`
- `nebula-common-telemetry`

### P2-3 前端与观测面板

前端主 chunk 后续可能继续增长，建议按视图 dynamic import。观测面板应基于现有 `/metrics` 和 xtrace 查询，优先展示 Router/Gateway 的 5xx、retry、circuit、TTFT、E2E、auth 拒绝等核心指标。

## 建议执行顺序

1. 改 auth 默认值，生产默认 fail-closed。（已完成）
2. 定稿并实现 `/stats/` 控制面契约。
3. 将 placement 更新路径全部 CAS 化。（已完成）
4. Router 改为 header-driven routing，逐步移除 full body buffer。
5. 固定 Gateway / Router / BFF / UniGateway 边界。
6. 统一 BFF telemetry/auth 初始化。
7. 补关键架构测试。
8. 再做 common 拆分、配置收敛、前端拆包。

## 结论

Nebula 当前已经从“架构雏形”进入“生产化收敛”阶段。后续不要再优先堆 feature list，而应先把安全默认值、stats 热路径、placement 一致性和组件边界固定下来。只要这四点收敛，后面的路由智能化、自动扩缩容和观测面板都会更稳定。
