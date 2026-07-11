# Nebula 工程优化计划

> 合并原 `docs/arch_optimization.md` 与 `docs/optimization_plan.md`（2026-07）。
> 架构方向与生命周期 Wave 见 [`architecture.md`](./architecture.md)；HA 见 [`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)。

## 总体判断

控制面主链路已打通（`cargo check --workspace` 通过）。旧结论（Gateway 编译失败、无 metrics、Router 无熔断等）已过期。架构方向无需调整。当前最优先是 [`architecture.md`](./architecture.md) Wave A 的**正确性修复**（同节点多副本、缩容、Drain 推进），其次才是恢复预算/进程树、Router 一致性，以及 stats / header 路由等生产化项。

| 领域 | 当前状态 | 下一步 |
|------|----------|--------|
| Gateway | 鉴权、审计、metrics、body 上限、BFF 代理、SSE abort | 边界收敛；header 注入 model |
| Router | 策略插件、重试、熔断、过载、TTFT/E2E | 快照 revision watch；每模型 plan_version；header-driven 流式 |
| Scheduler | reconcile + placement CAS + election/fencing **已落地** | 真正执行缩容；version 改逻辑单调；声明式单路径 |
| Node | GPU/健康检查/restart/stats scrape | `(model_uid,replica_id)` 状态；周期 reconcile；进程树/恢复预算 |
| Observability | `/metrics` + trace context | BFF 统一 telemetry；面板口径 |

## 已落地（摘要）

Gateway：共享 auth（fail-closed）、`/metrics`、body 上限、trace、部分 BFF 代理、断连 abort。  
Router：`least_pending` / `least_kv_cache` / `prefix_cache_aware`、Ready/`plan_version` 过滤、affinity、重试/熔断/admission、TTFT/E2E。  
Scheduler：周期 reconcile、placement CAS、自愈与负载扩缩容边界。  
Node：GPU 指标、健康检查与 docker restart、引擎 stats → xtrace、Node API metrics。

## P0

**P0-1 鉴权 fail-closed（已完成）**  
未配置 token 则拒绝；仅 `NEBULA_AUTH_DISABLED` / `NEBULA_DEV_AUTH_DISABLED` 免鉴权。部署文档与 compose 须继续强调生产配置 `NEBULA_AUTH_TOKENS`。

**P0-2 stats 控制面契约（待做）**  
实时决策走 etcd `/stats/{model_uid}/{replica_id}`（TTL/lease）；xtrace/Prometheus 只做历史与面板。Router watch `/stats/`，Scheduler 读 `/stats/` 做扩缩容，避免强依赖观测后端。

**P0-3 placement 全路径 CAS（已完成）**  
生产路径无裸 `put`；冲突计 `nebula_scheduler_placement_cas_conflict_total`。下一步把 `version` 从 `now_ms()` 改为 `old + 1` 逻辑单调版本。

**P0-4 正确性三件套（见 architecture Wave A，最高优先）**  
同节点多副本状态键、缩容真正截断 assignment、Node 周期 reconcile 推进 Drain。

**P0-5 生命周期闭环（见 architecture Wave B）**  
进程树清理（或 Docker-only）、恢复预算 + Failed 终态、取消传播契约测试。

**P0-6 header-driven routing（待做，原 P0-4）**  
Gateway 解析 `model` 注入 `X-Nebula-Model` / `X-Nebula-Model-Uid`；Router 按 header 选 endpoint 并流式转发 body；保留 body 上限作防线。

## P1 边界与一致性

**P1-1 组件 owner**（详见 [`../dev/api_ownership.md`](../dev/api_ownership.md)）

| 组件 | Owner |
|------|-------|
| Gateway | 外部协议、鉴权、审计、错误映射、请求上下文 |
| Router | endpoint 选择、重试、熔断、过载、上游代理 |
| BFF | 控制台 API、用户/session、模型管理视图 |
| UniGateway | 可选协议库，不拥有集群调度语义 |

同一类 API 只能有一个 owner；Gateway 可代理 BFF，但不可双实现。

**P1-2 声明式单路径**  
收敛 `/deployments/` → PlacementPlan；淘汰 `/model_requests/` 写路径。

**P1-3 telemetry / auth 初始化**  
BFF 改用 `nebula_common::telemetry::init_tracing`；推理 token、控制台 session、服务间 token 分离。

**P1-4 watch revision 续传**  
Router/Node 记录 last revision + 定期 full reconcile。

## P2 测试与可维护性

优先补：Router（策略/熔断/stale/429/header 路由）、Scheduler（CAS/扩缩容）、Node（健康/Unhealthy/cooldown/预算）、Gateway（鉴权/SSE/abort）。  
后续再拆 `nebula-common`（types/auth/telemetry）、统一 HTTP client 超时、前端按视图拆包与观测面板。

## 建议执行顺序

1. P0-4 正确性三件套（architecture Wave A：多副本 / 缩容 / Drain）  
2. P0-5 生命周期（Wave B：进程树 / 恢复预算 / 取消验收）  
3. P1-4 Router watch revision + 每模型 plan_version；P0-3 version 单调  
4. P1-2 声明式单路径；P0-6 header-driven；P0-2 `/stats/`  
5. P1-1 / P1-3 边界与 telemetry；P2 测试  
6. HA 无状态多副本按 [`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)（Scheduler election 已完成）

| 项目 | 预估 |
|------|------|
| 多副本 + 缩容 + Drain | 3–5 天 |
| 恢复预算 / 进程树 | 2–3 天 |
| Router revision / plan_version | 1–2 天 |
| `/stats/` 契约 | 1.5–2 天 |
| header-driven + streaming | 2–3 天 |
| 边界与文档 | 1 天 |
| 关键架构测试 | 2–3 天 |
