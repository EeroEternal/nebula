# Nebula 架构

> 权威架构说明（2026-07-12）。排期与勾选见 [`optimization.md`](./optimization.md)；HA 报告见 [`../dev/ha/`](../dev/ha/)；边界见 [`../dev/api_ownership.md`](../dev/api_ownership.md)。

**结论：** 方向不变——etcd 声明式状态、Rust 控制面、外部引擎进程、HTTP Passthrough。M1 正确性、N1 接入/调度 HA（真机）、N2 工程质量已完成；生产 etcd 三节点暂缓。剩余工程项以 [`optimization.md`](./optimization.md) 为准（可观测 O1–O8 ✅；按需 O9/N3/N4；Product P1）。

---

## 1. 背景与动机

Xinference / powerllm 可复用资产在模型与协议侧，负债在控制面：多级 RPC、actor 内存态、引擎嵌入与 monkey-patch。Nebula 的取舍：Rust 控制面 + 外部引擎进程 + etcd 权威状态；放弃「任意 Python 模型对象进程内托管」，长尾模型一律引擎化 / HTTP 化。

相对 xoscar：状态可审计重建、故障域=容器/进程、标准 HTTP/SSE、引擎升级不牵动控制面。跨节点多 rank 不自建协调，按需编排引擎原生分布式。

### 设计原则

- 控制面 / 执行面分离；声明式 + Reconcile；watch + periodic full reconcile 兜底。
- 引擎零侵入；当前能力面以 Passthrough 为主，capability / EngineShim 按需。
- 可观测优先：abort/drain 有独立 metrics，不计 5xx 成功率分母。
- etcd 是唯一权威；本地缓存可丢可重建；对账是「etcd vs runtime」清孤儿，不是双权威同步。
- Gateway = 协议/鉴权/审计；Router = 选路+代理；BFF = 控制台；Scheduler 只写期望、不碰 Postgres。

### 相对 PowerLLM：学什么 / 不学什么

**学行为：** fencing 拒旧主写、follower healthz=503、scale-in drain、abort 传播、契约进 CI、镜像族隔离。  
**不学形态：** xoscar/Actor、上帝 Orchestrator、内存权威+事后 rectify、binlog 旧 HA、引擎内嵌/venv subpool、Gateway 吞 Router、core↔API 共享 ORM。

---

## 2. 总体架构

| 组件 | 职责 |
|------|------|
| **Gateway** | OpenAI 兼容 HTTP/SSE；鉴权、规范化、错误映射；abort；注入 `x-nebula-model` |
| **Router** | endpoint 选择 + 代理；plan_version / stats / 熔断过载 |
| **Scheduler** | PlacementPlan CAS；只认 `/deployments/`；leader election + fencing |
| **MetaStore (etcd)** | 权威元数据（watch / lease / CAS） |
| **Node** | watch placement → 启停引擎 → 注册 endpoint/stats；心跳与自愈 |
| **Engine** | vLLM / SGLang 等原生 HTTP；生产优先 Docker |
| **BFF** | 控制台 API、session、声明式模型管理 |

默认路径：**Engine-Passthrough**（Gateway → Router → 引擎原生 HTTP）。EngineShim gRPC 为可选增强。

```
Client → Gateway → Router → Engine
                ↗
Node / Scheduler / BFF ⇄ etcd
```

生产元数据默认可单节点 etcd；接入面可多副本（见 HA 文档）。三节点 etcd 能力已旁路验证，生产迁移暂缓。

---

## 3. etcd Keyspace

| Key | 类型 | 说明 |
|-----|------|------|
| `/nodes/{node_id}/status` | `NodeStatus` | 心跳（lease） |
| `/models/{model_uid}/spec` | `ModelSpec` | 规格 |
| `/deployments/{model_uid}` | `ModelDeployment` | 声明式期望（唯一写入口） |
| `/placements/{model_uid}` | `PlacementPlan` | 逻辑单调 `version` + `updated_at_ms` |
| `/endpoints/{model_uid}/{replica_id}` | `EndpointInfo` | 须带 `plan_version` |
| `/stats/{model_uid}/{replica_id}` | `EndpointStats` | Node 写、Router watch、Scheduler list；xtrace 只做历史 |
| `/model_requests/` | 遗留 | 仅失败回写等；新路径不写 |

约束：placement 全路径 CAS；Router 每模型 `plan_version`；watch 用快照 revision，compact/重连后全量校正（endpoints / placements / stats）。

---

## 4. 调度与节点

**扩缩容：** `healthy > desired` 截断 assignment；`<` 增加；`==` 不改（除非 stale）。缩容优先低 pending。

**Node：** `(model_uid, replica_id)` 集合差量 reconcile；`periodic_full_reconcile` 推进 Drain；lease 复用；锁外 download/start/health/scrape；恢复预算 + 进程组/Docker restart。

**热路径：** Gateway peek model → `x-nebula-model`；Router header 优先 + 字节级 model 改写。

---

## 5. API 与可观测

| 接口 | 状态 |
|------|------|
| `/v1/chat/completions`、`/v1/responses` | ✅ |
| `/v1/embeddings` / `rerank`、`/v1/models` | ✅ |

控制台写路径走 BFF；Gateway `/v1/admin/*` 写接口不扩新双实现（见 api_ownership）。

可观测三平面（设计见 [`../dev/observability.md`](../dev/observability.md)）：

- **Trace / LLM 语义：** OTLP → xtrace（`init_tracing` + W3C `traceparent`）
- **时序：** Prometheus `/metrics`；热路径与 xtrace **双写**（`DualWriteEmitter`）
- **日志：** `NEBULA_LOG_FORMAT=json` → stdout → Promtail/Vector → Loki（[`../dev/loki.md`](../dev/loki.md)）

鉴权分离：推理 token、BFF session、`OBSERVE_TOKEN`。abort/drain 指标不计入 5xx 错误预算。

---

## 6. 里程碑

| 阶段 | 内容 | 状态 |
|------|------|------|
| **M0** | 单机 etcd + gateway/router/node + 引擎 streaming | ✅ |
| **M1** | 多机调度：多副本 + 缩容/Drain + watch + 自愈 + `/stats/` | ✅ |
| **M2** | 声明式单路径；header 热路径 | ✅；capabilities 按需 |
| **M3** | affinity + prefix/KV 路由深化 | 策略已有，持续打磨 |
| **HA** | Scheduler election；接入多副本真机；旁路 etcd 三节点 | ✅ 主体；生产 etcd 三节点 ⏸ |
| **Obs** | 双写 + JSON/Loki 路径 | ✅ O1–O8；runbook [`../dev/slo_alerts.md`](../dev/slo_alerts.md) |

HA 真机报告：[`../dev/ha/report-20260711.md`](../dev/ha/report-20260711.md)。

---

## 7. Wave 回顾（已关闭）

| Wave | 内容 |
|------|------|
| A–C | 多副本、缩容、Drain、恢复预算、plan_version、deployments、header、lease、锁外 I/O |
| D1/D2 | Scheduler HA、Drain 闭环 |
| D3 主体 | 接入多副本 + Phase D 真机演练（生产 etcd 迁移除外） |
| N2 | BFF 去重、HTTP client、observe、CI |

未尽项（O9、N3、产品 N4、生产 etcd、Product P1）只在 [`optimization.md`](./optimization.md) 维护，本文不另开排期表。

---

## 8. 总评

Nebula 用 etcd + 引擎原生 HTTP 重新分解了 actor 式进程管理问题，架构主轴无需再改。正确性与接入/调度 HA 行为已有真机报告；工程债 N2 已收。生产继续单节点 etcd 即可。具体「下一步做什么」以 [`optimization.md`](./optimization.md) 为唯一排期源，避免在本文重复过时结论。
