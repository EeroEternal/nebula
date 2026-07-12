# Nebula 架构

> 权威架构说明（2026-07-11 修订）。路径评估已吸收原 vs_xoscar / GPT 审查结论。  
> 下一步工程计划见 [`optimization.md`](./optimization.md)；HA 细节见 [`../dev/ha/`](../dev/ha/)；组件边界见 [`../dev/api_ownership.md`](../dev/api_ownership.md)。

**结论：** 架构方向不变——etcd 声明式状态、Rust 控制面、外部引擎进程、HTTP Passthrough。M1 / N2 / N1 HA 主体已闭环；**当前主线可观测（N4-Obs：xtrace+Prometheus 双写 + Loki 日志）**；生产 etcd 三节点暂缓；N3/N4 产品按需。

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
| **Gateway** | OpenAI 兼容 HTTP/SSE；鉴权、规范化、错误映射；abort 传播；注入 `x-nebula-model` |
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

## 4. 调度与节点（已落地行为）

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

Tracing：各组件 `nebula_common::telemetry::init_tracing`（OTLP + W3C 传播；`NEBULA_LOG_FORMAT=json` → Loki）。  
热路径指标：**Prometheus `/metrics` + xtrace batch 双写**（`DualWriteEmitter`）。鉴权分离：推理 token、BFF session、`OBSERVE_TOKEN`。

---

## 6. 里程碑

| 阶段 | 内容 | 状态 |
|------|------|------|
| **M0** | 单机 etcd + gateway/router/node + 引擎 streaming | ✅ |
| **M1** | 多机调度：多副本 + 缩容/Drain + watch + 自愈 + `/stats/` | ✅ |
| **M2** | 声明式单路径；header 热路径；能力面增强（部分已做） | 部分 ✅；capabilities 按需 |
| **M3** | affinity + prefix/KV 路由深化；Agent 友好 | 策略已有，持续打磨 |
| **HA** | Scheduler election ✅；接入多副本真机 ✅；etcd 三节点拓扑/旁路 ✅ | 报告 [`../dev/ha/report-20260711.md`](../dev/ha/report-20260711.md)；生产 etcd 三节点**暂缓** |

---

## 7. Wave 回顾（已关闭）

| Wave | 内容 |
|------|------|
| A | 同节点多副本、自动缩容、Drain 周期、取消契约 |
| B | 恢复预算/进程树、Router revision、每模型 plan_version、逻辑 version、deployments 单路径 |
| C | header 热路径、锁外 I/O、lease 复用、文档与单测 |
| D1/D2 | Scheduler HA、Drain 闭环 |
| D3 主体 | 接入多副本 + 真机 Phase D 演练（生产 etcd 迁移除外） |

未完成与按需项见 [`optimization.md`](./optimization.md)。

---

## 8. 总评

Nebula 用 etcd + 引擎原生 HTTP 重新分解了 actor 式进程管理问题。方向无需调整；正确性、工程可维护性与接入/调度 HA 行为已在真机验证。生产 etcd 三节点**暂缓**；下一步按业务做能力面（N3）与产品化（N4）——而不是再改架构主轴。
