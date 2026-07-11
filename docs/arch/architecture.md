# Nebula 架构与优化计划

> 融合原 `docs/architecture.md` 与 `docs/vs_xoscar_analysis.md`（2026-07）。
> 2026-07-11 对照代码审查后修订：正确性问题优先于生命周期增强。
> 相关：[`../dev/ha/ha_roadmap.md`](../dev/ha/ha_roadmap.md)、[`../dev/absorb_powerllm_engineering_guidance.md`](../dev/absorb_powerllm_engineering_guidance.md)、[`../dev/api_ownership.md`](../dev/api_ownership.md)、[`optimization.md`](./optimization.md)。

**结论先行：** 架构方向合理——etcd 声明式状态、Rust 控制面、外部引擎进程、HTTP Passthrough 均无方向性问题。Wave A–C、P0 与 P1 已落地；后续优先 Wave D（HA 无状态多副本）与 P2 可维护性。

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
| 取消传播 | 有（有挂死坑） | SSE 断连已 drop 上游；契约脚本 `scripts/test_cancel_sse.sh` | **已落地** |
| 同节点多副本 | actor 可多实例 | `(model_uid, replica_id)` 状态键 + assignment 集合差量 | **已落地** |
| 自动缩容 | 有 | `healthy > desired` 时截断 assignment 并 CAS | **已落地** |
| Drain | 有 | `periodic_full_reconcile` 推进 pending/超时后停引擎 | **已落地** |
| 恢复预算 | `AUTO_RECOVER_LIMIT` | 每 replica 24h 预算 + 退避 + Failed | **已落地** |
| 进程树清理 | 框架侧管理 | 本地进程组 SIGTERM→SIGKILL；Docker `restart` | **已落地** |
| 跨节点多 rank | 每 rank 一个 actor | 仅单节点 TP | 按需；编排引擎原生，不自建 rank |

取舍：nebula 隐含放弃「任意 Python 模型对象进程内托管」；长尾模型一律引擎化 / HTTP 化（virtual engine 即此思路）。

### 1.3 设计原则（吸收 Dynamo）

- **控制面 / 执行面分离**：控制面决策与编排，执行面推理与占资源。
- **声明式 + Reconcile**：写期望状态，Node 持续对齐实际状态。
- **Watch-driven**：notify 快路径（可选）+ watch 主路径 + periodic full reconcile 兜底（已落地）。
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
| `/stats/{model_uid}/{replica_id}` | `EndpointStats` | Node scrape 写入（lease/TTL）；Router watch、Scheduler list；xtrace 只做历史 |
| `/model_requests/` | 遗留 | 过渡只读/失败回写；新 load·scale·stop 写 `/deployments/` |

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

扩缩容语义（已落地）：

- `healthy > desired`：按负载低 / 无 affinity / 节点资源紧张优先选择待移除副本，截断 plan 并 CAS 写回。
- `healthy < desired`：增加 assignment。
- `healthy == desired`：不更新（除非有 stale 清理）。

### 4.2 Node 主循环

已落地：

- `watch_placements_loop()`：监听 `/placements/`，reconcile。
- `heartbeat_loop()`：上报 `/nodes/{id}/status`（启动时 `grant_lease` + keepalive；status/endpoints 共用）。
- 健康检查：锁外 probe；不健康达阈值则 restart（受恢复预算约束）。
- `periodic_full_reconcile_loop()`：推进 Drain、兜底 watch 漏事件。

运行状态键：`(model_uid, replica_id)`。Reconcile 以本节点 assignment **集合**做差量：多则启、少则 drain、配置变则滚动。锁内只做快照/提交；download/start/stop/health/scrape 在锁外。

Drain：首次标记 `Draining` 后返回；后续周期 reconcile 检查 pending / 超时再 `finish_drain_stop`。

自愈：每 `(model_uid, replica_id)` 恢复预算（24h 内 N 次）+ 指数退避；超限写 Failed。本地进程组清理（`process_group` + SIGTERM→SIGKILL），Docker 走 `docker restart`；`try_restart` 重建 `EngineHandle`。

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

热路径（已落地）：Gateway peek `model` 后写入 `x-nebula-model`；Router 优先按 header 选路，并对 model 字段做字节级改写（无完整 JSON DOM），避免 Gateway + Router 双层完整缓冲。

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

### 8.1 Wave A — 正确性修复 ✅

| ID | 项 | 验收（已达成） |
|----|----|----------------|
| A0 | 同节点多副本 | `(model_uid, replica_id)`；同节点 `replicas=2` 双引擎双 endpoint |
| A1 | 自动缩容 | `healthy > desired` 截断 assignment CAS；单测选副本逻辑 |
| A2 | Drain 推进 | `periodic_full_reconcile_loop`；删除 placement 后超时内停引擎 |
| A3 | 取消传播验收 | `scripts/test_cancel_sse.sh`；abort 不计 5xx |

### 8.2 Wave B — 恢复与控制面一致性 ✅

| ID | 项 | 验收（已达成） |
|----|----|----------------|
| B1 | 恢复预算 + 进程树 | 24h 预算 + Failed；进程组/Docker restart |
| B2 | Router list→watch | 快照 revision 启 watch；compact/重连全量校正 |
| B3 | 每模型 `plan_version` | `model_uid → plan_version`；旧 plan 不可被选 |
| B4 | placement version 单调 | `next_placement_version(old)`；`updated_at_ms` 墙钟 |
| B5 | 删除双路径 | API/BFF 写 `/deployments/`；Scheduler 只 watch deployments |

### 8.3 Wave C — 性能与工程质量 ✅

| ID | 项 | 验收（已达成） |
|----|----|----------------|
| C1 | 热路径去缓冲 | Gateway `x-nebula-model`；Router header 优先 + 字节级 model 改写 |
| C2 | Node 锁粒度 | reconcile/heartbeat 锁外 I/O；锁内仅快照/提交 |
| C3 | etcd lease 复用 | `grant_lease` + keepalive；status/endpoints 共用 |
| C4 | 文档与测试对齐 | Passthrough 为默认；多副本/缩容/Drain/B4/C1 单测；`cargo test --workspace` 绿 |

### 8.4 Wave D — HA 与按需能力

| ID | 项 | 状态 | 说明 |
|----|----|------|------|
| D1 | Scheduler HA | **已完成** | election、leader-only 写、`leader_epoch` fencing |
| D2 | Drain 闭环 | **已完成** | 见 A2 |
| D3 | 无状态多副本 | 待做 | Gateway/Router/BFF 多副本 + LB；etcd 三节点 |
| D4 | 跨节点 TP | 按需 | 编排引擎原生分布式，不自建 rank |
| D5 | EngineShim / capabilities | 按需 | 仅当 Passthrough 不够时引入 |

### 8.5 建议排期

1. ~~第一批（正确性）：A0–A2~~ ✅  
2. ~~第二批：B1–B3、A3~~ ✅  
3. ~~第三批：B4/B5、C1–C4~~ ✅  
4. **按业务：** D3；D4/D5 明确需求再开。P2 见 [`optimization.md`](./optimization.md)。

---

## 9. 总评

Nebula 用 etcd + 引擎原生 HTTP + 容器边界，重新分解了 xoscar 试图用 actor 解决的分布式进程管理问题。架构方向无需调整。M1 正确性与工程 P0/P1 已闭环；下一阶段以 Wave D HA 与按需能力为主。
