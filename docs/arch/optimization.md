# Nebula 工程优化计划

> 合并原 `docs/arch_optimization.md` 与 `docs/optimization_plan.md`（2026-07）。
> 架构方向与生命周期 Wave 见 [`architecture.md`](./architecture.md)；HA 见 [`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)。

## 总体判断

控制面主链路、Wave A–C、P0 全项与 P1-1/P1-3 已落地。架构方向无需调整。当前优先 Wave D（HA 无状态多副本）与 P2 可维护性。

| 领域 | 当前状态 | 下一步 |
|------|----------|--------|
| Gateway | 鉴权、审计、metrics、body 上限、BFF 代理、SSE abort、`x-nebula-model` | — |
| Router | 策略/熔断/过载、revision watch、plan_version、header 选路、watch `/stats/` | — |
| Scheduler | reconcile + CAS + election、缩容、逻辑 version、deployments、list `/stats/` | — |
| Node | 多副本、Drain、预算、lease、锁外 I/O、写 `/stats/` + lease | — |
| Observability | 各组件 `init_tracing`；BFF session / 推理 token / OBSERVE_TOKEN 分离 | 面板口径 |

## 已落地（摘要）

Gateway：共享 auth（fail-closed）、`/metrics`、body 上限、trace、部分 BFF 代理、断连 abort、C1 header 注入。  
Router：策略插件、Ready/`plan_version` 过滤、affinity、重试/熔断/admission、TTFT/E2E、快照 revision watch、etcd `/stats/` watch。  
Scheduler：周期 reconcile、placement CAS、逻辑单调 version、声明式 `/deployments/`、缩容截断、etcd `/stats/` 扩缩容信号。  
Node：多副本状态键、Drain 周期推进、GPU/健康/预算重启、lease 复用、锁外 download/start/health、`/stats/` put_with_lease。

## P0

**P0-1 鉴权 fail-closed（已完成）**  
未配置 token 则拒绝；仅 `NEBULA_AUTH_DISABLED` / `NEBULA_DEV_AUTH_DISABLED` 免鉴权。

**P0-2 stats 控制面契约（已完成）**  
Node 写 etcd `/stats/{model_uid}/{replica_id}`（TTL/lease）；Router watch `/stats/`；Scheduler list `/stats/` 做扩缩容；xtrace/Prometheus 只做历史与面板。详见 [`../dev/details/stats.md`](../dev/details/stats.md)。

**P0-3 placement 全路径 CAS（已完成）**  
生产路径无裸 `put`；`next_placement_version` 逻辑单调；`updated_at_ms` 存墙钟。

**P0-4 正确性三件套（已完成，architecture Wave A）**  
同节点多副本、缩容截断 assignment、Node 周期 reconcile 推进 Drain。

**P0-5 生命周期闭环（已完成，architecture Wave B）**  
进程树清理、恢复预算 + Failed、取消传播契约脚本。

**P0-6 header-driven routing（已完成）**  
Gateway 注入 `x-nebula-model`；Router header 优先选路 + 字节级 model 改写。

## P1 边界与一致性

**P1-1 组件 owner（已完成）**  
约定见 [`../dev/api_ownership.md`](../dev/api_ownership.md)。推理热路径 Gateway→Router；控制台写路径只走 BFF；禁止双实现。

| 组件 | Owner |
|------|-------|
| Gateway | 外部协议、鉴权、审计、错误映射、请求上下文 |
| Router | endpoint 选择、重试、熔断、过载、上游代理 |
| BFF | 控制台 API、用户/session、模型管理视图 |
| UniGateway | 可选协议库，不拥有集群调度语义 |

**P1-2 声明式单路径（已完成）**  
API/BFF 写 `/deployments/`；Scheduler 只 watch deployments。迁移说明见 [`../dev/migrate_deployments.md`](../dev/migrate_deployments.md)。

**P1-3 telemetry / auth 初始化（已完成）**  
BFF 使用 `nebula_common::telemetry::init_tracing`（与 Gateway/Router/Scheduler/Node 一致）。鉴权面分离：推理 `NEBULA_AUTH_TOKENS`、控制台 BFF session、观测 `OBSERVE_TOKEN` / `OBSERVE_AUTH_MODE`。

**P1-4 watch revision 续传（已完成）**  
Router 快照 revision + compact/重连全量校正；Node periodic full reconcile。

## P2 测试与可维护性

多副本差量、缩容选副本、Drain/plan_version 过滤、B4 version、C1 peek/rewrite、恢复预算单测已覆盖；`cargo test --workspace` 绿。后续再拆 `nebula-common`、统一 HTTP client 超时、前端按视图拆包。

## 建议执行顺序

1. ~~Wave A–C / P0 / P1~~ ✅  
2. Wave D3 HA 无状态多副本（见 [`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)）  
3. D4/D5 按需；P2 可维护性按需
