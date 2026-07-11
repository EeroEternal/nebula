# Nebula 架构与优化计划

> 融合原 `docs/architecture.md` 与 `docs/vs_xoscar_analysis.md`（2026-07）。
> 2026-07-11 对照代码审查后修订：正确性问题优先于生命周期增强。
> 相关：[`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)、[`../dev/absorb_powerllm_engineering_guidance.md`](../dev/absorb_powerllm_engineering_guidance.md)、[`../dev/api_ownership.md`](../dev/api_ownership.md)、[`optimization.md`](./optimization.md)。

**结论先行：** 架构方向合理——etcd 声明式状态、Rust 控制面、外部引擎进程、HTTP Passthrough 均无方向性问题。真正的优先项不是换架构，而是先修 **多副本状态模型、缩容未执行、Drain 无法推进** 三类正确性缺陷；再补恢复预算、进程树清理与 Router 一致性；生命周期增强与性能优化排在其后。

---

## 1. 背景与动机

### 1.1 控制面负债

Xinference / powerllm 可复用资产在模型与协议侧，主要瓶颈在控制面：

- 多级 RPC（Supervisor → Worker → ModelActor）调度与路由开销高。
- 活状态留在 actor 内存，重启恢复成本高、故障域大。
- Actor 子进程模型与 vLLM 等引擎内部进程模型互相干扰。
- 引擎当库嵌入并 monkey-patch，升级牵动大量兼容代码。

Nebula 的方向：重组资产、替换负债——Rust 控制面 + 外部引擎进程 + etcd 权威状态。

### 1.2 相对 xoscar 的路径判断

| 维度 | powerllm (xoscar) | nebula | 评价 |
|------|-------------------|--------|------|
| 状态权威 | actor 进程内存 | etcd（TTL + CAS + watch） | **nebula 优**：可外部审计、天然重建 |
| 进程模型 | 模型嵌在 actor subpool | Node 持有 RunningModel；Docker/本地进程 | **nebula 优**：故障域=容器/进程 |
| RPC | actor ref + 自定义序列化 | 纯 HTTP/SSE，引擎原生端点 | **nebula 优**：标准协议、可观测 |
| 引擎耦合 | import 进 worker | 外部 `vllm serve` / sglang；控制面 Rust | **nebula 优**：升级引擎不动控制面 |
| 取消传播 | 有（有挂死坑） | SSE 断连已 drop 上游；需契约测试钉死 | **部分完成** |
| 同节点多副本 | actor 可多实例 | Scheduler 可生成多 assignment，Node 以 `model_uid` 为键只跑一个 | **正确性缺陷** |
| 自动缩容 | 有 | 算出 `desired < current` 但未截断 assignment | **正确性缺陷** |
| Drain | 有 | 已标 Draining，缺周期推进，可能永久停留 | **部分完成** |
| 恢复预算 | `AUTO_RECOVER_LIMIT` | 固定阈值重启，无上限；本地 `try_restart` 空操作 | **待补** |
| 进程树清理 | 框架侧管理 | 本地 `Child::kill` 杀不掉 EngineCore | **待补** |
| 跨节点多 rank | 每 rank 一个 actor | 仅单节点 TP | 按需；编排引擎原生，不自建 rank |

取舍：nebula 隐含放弃「任意 Python 模型对象进程内托管」；长尾模型一律引擎化 / HTTP 化（virtual engine 即此思路）。

### 1.3 设计原则（吸收 Dynamo）

- **控制面 / 执行面分离**：控制面决策与编排，执行面推理与占资源。
- **声明式 + Reconcile**：写期望状态，Node 持续对齐实际状态。
- **Watch-driven**：notify 快路径（可选）+ watch 主路径 + **periodic full reconcile 兜底（必须落地）**。
- **引擎零侵入**：不 patch 引擎内部，不与引擎进程模型冲突。
- **capability 驱动**：按能力协商特性与降级（目标态；当前 Passthrough 下能力面较薄）。
- **可观测性优先**：路由、调度、引擎、取消、缓存命中可量化；abort/drain 有独立 metrics 口径。

---

## 2. 总体架构

### 2.1 组件

| 组件 | 语言 | 职责 |
|------|------|------|
| **Gateway** | Rust | OpenAI 兼容 HTTP + SSE；鉴权、规范化、错误映射、usage；abort 传播 |
| **Router** | Rust | endpoint 选择（least-connections / 健康 / affinity）+ 代理 |
| **Scheduler** | Rust | PlacementPlan；声明式写入 etcd（CAS）；leader election + fencing |
| **MetaStore** | etcd | 权威元数据（watch / lease / CAS） |
| **Node Daemon** | Rust | watch placement → reconcile 引擎；心跳；健康与自愈 |
| **Engine** | 外部进程 | vLLM / SGLang 等原生 HTTP；生产路径优先 Docker |

> **落地现实：** 当前默认是 **Engine-Passthrough**（Gateway → Router → 引擎原生 HTTP），不是文档早期写的 Unified Gateway → EngineShim gRPC。EngineShim / `EngineService` gRPC 为可选增强，按需引入；不要为 Shim 再引入进程内嵌模型。

### 2.2 数据流

```
Client
  │
  ▼
Gateway (8081)  ──  /v1/chat/completions, /v1/responses, …
  │
  ▼
Router (18081)  ──  endpoint 选择 + 请求代理（断连 drop 上游）
  │
  ▼
Engine (vLLM/SGLang)  ──  模型推理
  ▲
  │
Node Daemon     ──  watch placement → 启停引擎 → 注册 endpoint
  ▲
  │
Scheduler       ──  写入 PlacementPlan 到 etcd
  ▲
  │
etcd            ──  权威状态存储
```

### 2.3 引擎接入策略

- **Passthrough（当前默认）**：反向代理到引擎原生 HTTP；取消靠断 TCP。
- **EngineShim（可选）**：统一 gRPC（Health / Chat / CancelRequest / Capabilities / Metrics）；仅在需要非 HTTP 引擎或统一控制面契约时引入。
- **跨节点 TP**：不自建 rank 协调；编排 vLLM 原生多节点（Ray / torchrun 等），nebula 只下发参数与地址。

---

## 3. 元数据（etcd Keyspace）

| Key | 值类型 | 说明 |
|-----|--------|------|
| `/nodes/{node_id}/status` | `NodeStatus` | 节点心跳（lease/TTL） |
| `/models/{model_uid}/spec` | `ModelSpec` | 模型规格 |
| `/deployments/{model_uid}` | `ModelDeployment` | 声明式期望运行状态（目标唯一入口） |
| `/placements/{model_uid}` | `PlacementPlan` | 期望放置；`version` 须单调递增（见下） |
| `/endpoints/{model_uid}/{replica_id}` | `EndpointInfo` | Node 注册；必须带 `plan_version` |
| `/model_requests/` | 遗留 | **待删除**；过渡期双路径，收敛到 `/deployments/` |

一致性约束：

- Scheduler 更新 placement 必须 CAS（`expected_version` / revision 一致才写）。
- `PlacementPlan.version` 必须是 **逻辑单调版本**（CAS 时用 `old.version + 1`），时间戳单独存 `updated_at_ms`；禁止用 `now_ms()` 充当 version（同毫秒冲突与时钟回拨会导致重复/倒退）。
- Router 对 **每个** `model_uid` 维护 `plan_version`，只使用最新版本的 endpoints（禁止仅主模型过滤、其它模型裸 `route()`）。
- Watch：list 快照的 revision 启动 watch；断线续传；处理 compact；重连后 `list_prefix` 全量校正。Router 的 endpoints / placements 两条链路均须满足。

---

## 4. 调度与节点

### 4.1 PlacementPlan

- `version: u64`（逻辑单调递增，见 §3）
- `updated_at_ms: u64`（观测用时间戳）
- `assignments[]`: `replica_id / node_id / gpu_indices / engine_config / role`
- `role`：首期 `Unified`；schema 预留 `Prefill/Decode`

策略：MVP IdleFirst（健康 node + 显存过滤 + 负载最小 GPU）；后续 MemoryAware / Disaggregated / SLA。

扩缩容语义（目标态，当前代码未完整实现）：

- `healthy > desired`：按负载低 / 无 affinity / 节点资源紧张优先选择待移除副本，截断 plan 并 CAS 写回。
- `healthy < desired`：增加 assignment。
- `healthy == desired`：不更新（除非有 stale 清理）。

> **现状缺口：** Scheduler 会增加 `scale_down_total` metric，但 `need_update` 只在副本不足或 stale 时为真，缩容路径不会截断 assignment。

### 4.2 Node 主循环

目标态：

- `watch_placements_loop()`：监听 `/placements/`，reconcile。
- `heartbeat_loop()`：上报 `/nodes/{id}/status`（复用启动时创建的 lease + keepalive，禁止每次 put 新建 lease）。
- `health_check_loop()`：引擎健康；不健康达阈值则 restart（受恢复预算约束）。
- `periodic_full_reconcile()`：**必须落地**——推进 Drain、兜底 watch 漏事件；仅靠 placement watch 不够。

运行状态键：`(model_uid, replica_id)`，不是单独的 `model_uid`。Reconcile 以本节点 assignment **集合**做差量：多则启、少则 drain、配置变则滚动。

> **现状缺口：** `HashMap<String, RunningModel>` 以 `model_uid` 为键，且 `find` 只取本节点第一个 assignment，同节点多副本只会启动一个。

Drain 语义：首次标记 `Draining` 后返回；后续周期 reconcile 检查 pending / 超时再 `finish_drain_stop`。无周期循环时，placement 删除后可能永久停在 Draining。

自愈目标态：每 `(model_uid, replica_id)` 恢复预算（如 24h 内 N 次）+ 指数退避；超限写 Failed 到 etcd。本地进程须进程组清理（`setsid` + SIGTERM→SIGKILL 整组），或生产路径强制 Docker-only；`try_restart` 须真正重建 `EngineHandle`。

锁约定：全局 `running` 锁内只做状态快照与提交；下载、启动、readiness、健康检查、metrics scrape 放锁外，避免单模型冷启动阻塞整节点。

---

## 5. 对外 API 与 Router 契约

### 5.1 OpenAI 兼容

| 接口 | 状态 |
|------|------|
| `POST /v1/chat/completions` | ✅ |
| `POST /v1/responses` | ✅（streaming 对齐 OpenAI 事件序列） |
| `POST /v1/embeddings` / `rerank` | ✅（代理到 Router） |
| `GET /v1/models` | ✅ |

Admin：集群状态、模型 load/unload/scale、endpoint drain、metrics/logs 等（见 Gateway/BFF 契约文档）。

### 5.2 EndpointInfo（Router 必须项）

`model_uid / replica_id / plan_version / node_id / endpoint_kind / status / last_heartbeat_ms / base_url`。

规则：按模型丢弃落后 `plan_version`；`status != ready`（含 Draining）或心跳超时下线。可选 `EndpointStats`（pending、prefix cache、kv_cache）供策略与过载保护。

### 5.3 ExecutionContext

目标：Gateway 抽取 `request_id / session / tenant / priority / deadline / budget` 并贯穿 Router 与引擎侧。当前结构已有，决策路径尚未充分使用。

热路径目标：Gateway 解析 `model` 后写入可信内部 header（如 `X-Nebula-Model-Uid`）；Router 按 header 路由并 **流式转发原始 body**，避免 Gateway + Router 双层完整缓冲。

---

## 6. 可观测性与错误语义

- tracing：`request_id` 全链路；span 含 `model_uid / replica / node_id / engine`。
- metrics：各组件 Prometheus；`requests_aborted_total` 等 abort 指标不计入 5xx 成功率分母（与 SLO 口径一致）。
- 错误：上游失败映射为 OpenAI 风格 400/429/500。

---

## 7. 里程碑（目标态）

| 阶段 | 内容 |
|------|------|
| **M0** | 单机：etcd + gateway/router/node + vLLM；chat/responses streaming |
| **M1** | 多机调度：placement + 多 node + **同节点多副本** + 缩容/Drain 闭环 + endpoint watch + 自愈 |
| **M2** | capabilities / fallback / structured output；声明式单路径收敛 |
| **M3** | session affinity + prefix cache / KV-aware routing；Agent 友好 |

HA：Scheduler election + leader gate + fencing epoch **已实现**；后续见 [`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)（etcd 三节点与无状态多副本）。

---

## 8. 架构优化计划

原则：不换架构方向。优先修正确性，再补控制面一致性与生命周期完成条件，最后做性能与工程质量。

### 8.1 Wave A — 正确性修复（最高优先）

| ID | 项 | 现状 | 做法 | 验收 |
|----|----|------|------|------|
| A0 | 同节点多副本 | Scheduler 可生成多 assignment；Node 以 `model_uid` 为键且只取第一个 | 状态键改为 `(model_uid, replica_id)`；reconcile 按 assignment 集合差量 | 同节点 `replicas=2` 启动两个引擎并注册两个 endpoint |
| A1 | 自动缩容 | 算出 desired 偏小并打 metric，但不截断 plan | `healthy > desired` 时选待移除副本并 CAS 写回；优先低负载/无 affinity/资源紧张 | scale 后 assignment 数下降，多余引擎被 drain 停止 |
| A2 | Drain 推进 | 首次标 Draining 后返回；无周期 reconcile | 增加 Node `periodic_full_reconcile`，或为每个 drain 建可取消定时任务 | placement 删除后在超时内真正停引擎并删 endpoint |
| A3 | 取消传播验收 | Gateway/Router SSE 断连已 drop 上游 | 补真实引擎契约测试；统一 abort metrics/SLO | curl Ctrl-C → 引擎 aborted，GPU 回落；abort 不计 5xx |

### 8.2 Wave B — 恢复与控制面一致性

| ID | 项 | 现状 | 做法 | 验收 |
|----|----|------|------|------|
| B1 | 恢复预算 + 进程树 | 无限重启；本地 `try_restart` 空操作；`Child::kill` 留孤儿 | 每 replica 预算 + 退避 + Failed；本地 `setsid` 进程组清理，或生产 Docker-only；Node 重建 `EngineHandle` | 必崩配置停止空转；kill 后无 GPU 残留 |
| B2 | Router list→watch | list 后从「当前」revision watch，中间事件可能丢 | 用快照 revision 启 watch；处理 compact；重连全量校正；endpoints 与 placements 都改 | 断 etcd 再连无陈旧 endpoint 长期残留 |
| B3 | 每模型 `plan_version` | 全局一个 `AtomicU64`，仅主模型过滤 | `model_uid → plan_version`；所有模型统一过滤 | 多模型并存时旧 plan endpoint 不可被选中 |
| B4 | placement version 单调 | `version: now_ms()` | CAS 时 `old.version + 1`；时间戳改 `updated_at_ms` | 同毫秒连续更新 version 严格递增 |
| B5 | 删除双路径 | 同时 watch `/model_requests/` 与 `/deployments/` | API/BFF 只写 `ModelDeployment`；Scheduler 只认 deployment | 仅声明式一条路；迁移 CLI 可用 |

### 8.3 Wave C — 性能与工程质量

| ID | 项 | 做法 | 验收 |
|----|----|------|------|
| C1 | 热路径去缓冲 | Gateway 注入 model header；Router header 路由 + 流式转发 body | 大 embedding body 不双层整包进内存 |
| C2 | Node 锁粒度 | 每 replica 状态机或细粒度锁；锁外做 I/O 与进程操作 | 单模型冷启动不阻塞同节点其它模型 reconcile |
| C3 | etcd lease 复用 | 节点启动创建 lease + keepalive；status/endpoints 共用 | 心跳不再每次 `lease_grant` |
| C4 | 文档与测试对齐 | architecture 以 Passthrough 为默认；补多副本/缩容/Drain 测试 | README/契约与代码一致；`cargo test --workspace` 绿 |

### 8.4 Wave D — HA 与按需能力（部分已落地）

| ID | 项 | 状态 | 说明 |
|----|----|------|------|
| D1 | Scheduler HA | **已完成** | election、leader-only 写、`leader_epoch` fencing |
| D2 | Drain 闭环 | **部分完成** | Draining 状态已有；持续推进见 A2 |
| D3 | 无状态多副本 | 待做 | Gateway/Router/BFF 多副本 + LB；etcd 三节点 |
| D4 | 跨节点 TP | 按需 | 编排引擎原生分布式，不自建 rank |
| D5 | EngineShim / capabilities | 按需 | 仅当 Passthrough 不够时引入 |

### 8.5 建议排期

1. **第一批（正确性）：** A0 多副本状态模型、A1 缩容、A2 Drain 周期推进。
2. **第二批：** B1 恢复预算/进程树、B2/B3 Router revision 与 plan_version、A3 取消契约测试。
3. **第三批：** B4/B5 版本单调与双路径删除、C1–C3 流式化/锁/lease、C4 文档与测试。
4. **按业务：** D3；D4/D5 明确需求再开。D1 已完成，勿再标为目标态。

工程杂项（workspace 测试修复、CI 全量 `cargo test --workspace`）随 Wave A/B 一并做，避免回归不可见。

---

## 9. 总评

Nebula 用 etcd + 引擎原生 HTTP + 容器边界，重新分解了 xoscar 试图用 actor 解决的分布式进程管理问题，分解方式更现代。架构方向无需调整。当前最优先的不是文档早期强调的「生命周期增强」，而是 **同节点多副本、缩容真正执行、Drain 可持续推进** 三类正确性修复；其后才是恢复预算、进程清理、Router 一致性与声明式单路径。
