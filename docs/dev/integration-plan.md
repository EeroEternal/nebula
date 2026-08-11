# Nebula 对外暴露与集成 API 优化计划

> **状态：** 拟议（2026-08-11）· 基线 v1.4.0  
> **视角：** 以 Nebula 控制面自身分层（[`../arch/vision.md`](../arch/vision.md) L0–L4）决定**暴露什么、不暴露什么**；不围绕某一类客户场景定制。  
> **当前契约：** [`integration.md`](./integration.md)  
> **关系：** 补充 [`plan.md`](./plan.md) / [`../arch/roadmap.md`](../arch/roadmap.md)；与第三方推理产品 **无兼容要求**。

---

## 1. Nebula 是什么、对外应扮演什么角色

Nebula 是 **跨引擎推理控制面**：声明式部署与放置、统一接入、路由、观测与治理。真正算 token 的是外部引擎（vLLM / SGLang 等）。

对外应呈现 **两个面**，职责清晰、长期稳定：

| 面 | 路径 | 对象 | Nebula 职责 |
|----|------|------|-------------|
| **Inference API** | `/v1/*` | 业务 / 应用 | OpenAI 兼容接入；鉴权、租户、trace、转发 Router |
| **Control API** | `/platform/v1/*`（目标） | 自动化 / 平台编排 | 模型与副本生命周期；读节点与运行态；异步任务与错误 |

**Console（BFF + 前端）** 是人类运维 UI，不是机机集成的主契约。BFF 与 Control API 应共享同一套业务逻辑，而不是两套写 etcd 的路径。

**不对外承诺：** etcd / Router 直连、引擎 Pod 级操作、Serving Cell、训练、公有云 API 聚合（与 [`../manual/positioning.md`](../manual/positioning.md) 一致）。

---

## 2. 自评：今天暴露得怎样

| Nebula 内在对象 | 控制面真相（etcd） | 今天对外 | 问题 |
|-----------------|-------------------|----------|------|
| 模型规格 | `/models/{uid}/spec` | Admin load 顺带写 | 无独立 Model 资源；legacy 命名 |
| 部署期望 | `/deployments/{uid}` | `/v1/admin/models/load` | Gateway 与 BFF **行为不一致**（compat） |
| 放置计划 | `/placements/{uid}` | 埋在 `cluster/status` | 集成方不应解析 plan_version |
| 运行副本 | `/endpoints/{uid}/{replica_id}` | 同上 | 无 `/replicas` 读模型；ready 靠轮询 |
| 节点 | `/nodes/{id}/status` | 同上 | 无 `/nodes` 读 API |
| 推理 | Router → Engine | `/v1/chat/completions` | ✅ 主路径正确 |
| 鉴权 | Token / 租户 | `NEBULA_AUTH_TOKENS` vs BFF session | 机机与人机两套 |
| 治理 | SLO / Benchmark / Canary | 仅 BFF `/api/v2/*` | 是否暴露见 §3 Tier C |

**结论：** Inference 面基本到位；Control 面仍是「Admin 补丁 + 控制台完整能力」，缺少 Nebula 原生的、版本化的控制面 API。

---

## 3. 暴露分层（Nebula 自己定的边界）

集成 API 不按客户定制，按 **控制面分层** 决定默认暴露范围：

### Tier A — 默认承诺（Control API v1 核心）

与 L0 控制面主轴绑定，**任何**私有化 / ISV 集成都应包含：

- 模型规格 CRUD（`ModelSpec`）
- 部署期望：副本数、`node_affinity` / `gpu_affinity`、config override、启停
- 副本运行态只读：`replica_id`、`status`、`node_id`（不暴露引擎 `base_url` 给业务流量；编排侧可读）
- 节点 inventory 只读：在线、GPU、platform
- 异步 **Operation**（部署 / 扩缩 / 停止的结果追踪）
- **API Key** 机机鉴权（role + 可选 tenant）；与推理面可拆 scope
- 错误体、OpenAPI、与控制台 **同一套** compat 校验

### Tier B — 默认承诺（Inference API，已有）

- OpenAI 兼容路径（chat / completions / embeddings 等，以引擎能力为准）
- W3C `traceparent`、`ExecutionContext` header
- 多租户准入（可选）、C3 错误码、abort 语义

### Tier C — 按需扩展（单独立项，不进 v1 冻结范围）

控制面能力存在，但 **不默认** 写进 `/platform/v1` 首批承诺；有明确调用方再开：

| 能力 | Nebula 内在 | 暴露条件 |
|------|-------------|----------|
| Replica 固定路由 | Router 选路策略扩展 | 需要确定性路由（调试、亲和实验、合规隔离）时加 `x-nebula-replica-id` |
| Per-replica 放置 spec | PlacementAssignment 已 per-replica | 多副本 **必须** 异构落位时扩展 deployment schema；否则 Scheduler 自动放置即可 |
| Webhook / SSE | Operation 事件 | 集成方强烈反对轮询时加；不是 v1 阻塞项 |
| 治理读 API | SLO / Canary / Benchmark 结果 | 只读摘要进 `/platform/v1/...` 子路径，写操作仍走控制台或后续批次 |
| 租户 / 定价 CRUD | BFF + etcd `/tenants/` | 多租户平台要自助开户时再暴露；非所有部署需要 |

### Tier D — 不暴露给 Control API（Console 专用）

Benchmark 向导、Selection 草稿 UI、用户密码、前端偏好等 — 留在 BFF，不进入机机集成契约。

---

## 4. 目标形态

```text
Inference   Gateway  /v1/*                 OpenAI 生态，长期稳定
Control     Gateway  /platform/v1/*         Nebula 原生 REST + Operation + API Key
Console     BFF + 前端                      人机；调用与 Control 相同的 service 层
Internal    Router / Scheduler / etcd       永不对外
```

---

## 5. 分阶段计划

### I0 — 控制面单一写路径（快赢）

**目的：** 在改 URL 之前，让「API 部署 = 控制台部署」。

| ID | 交付 | 状态 |
|----|------|------|
| I0.1 | 共享 **`nebula-control`** crate；Gateway Admin 调 `load_model` / `scale_model` / `stop_model` | ✅ |
| I0.2 | 部署入口统一 **兼容矩阵** + 结构化拒绝 | ✅ |
| I0.3 | Control 侧错误统一 C3 envelope | ✅ |
| I0.4 | OpenAPI [`openapi-control.yaml`](./openapi-control.yaml) | ✅ |
| I0.5 | Legacy 路径 OpenAPI `deprecated` + 文档互链 | ✅ |

**范围：** Tier A 的前置；不新增 Tier C 能力。

---

### I1 — Control API v1（Tier A 落地）

**目的：** Nebula 控制面的 **正式对外契约**。

| ID | 交付 | 验收 | 状态 |
|----|------|------|------|
| I1.1 | `GET/POST /platform/v1/models` … | ModelSpec 资源化 | ✅ |
| I1.2 | `GET/PUT …/models/{uid}/deployment`；`POST …/stop` | 部署期望与 Tier A 字段对齐 | ✅ |
| I1.3 | `GET …/models/{uid}/replicas`；`GET /platform/v1/nodes` | 运行态 / inventory 只读 | ✅ |
| I1.4 | `POST` 变更 → `operation_id`；`GET /platform/v1/operations/{id}` | 集成方不读 placement JSON | ✅ |
| I1.5 | **API Key**（Postgres）；scope：`inference` / `control` / `admin` | 替代纯 env token 的长期方案；env token 可保留兼容 | ✅ |
| I1.6 | 废弃 `/v1/admin/models/load`、`/v1/admin/v2/*` 代理 | 文档与 OpenAPI 只认 `/platform/v1` | ✅ |

**Inference：** 仍 `/v1/*`；与 Control 共享 Key 或分 scope。

---

### I2 — 控制语义补全（Tier C 择优）

**目的：** 补齐 Nebula **已有概念** 的对外表达，而非为某行业加字段。每项 **独立可选**，按需求开关：

| ID | 能力 | Nebula 依据 | 默认 | 状态 |
|----|------|-------------|------|------|
| I2.1 | Operation **SSE** `…/operations/{id}/events` | 异步 reconcile 天然有阶段 | 建议做；替代轮询 | ✅ |
| I2.2 | 推理响应回显 `x-nebula-request-id` / trace | 已有 ExecutionContext + OTel | 建议做；低成本 | ✅ |
| I2.3 | **`x-nebula-replica-id`** 固定 Router 选路 | Router 扩展；默认仍自动负载均衡 | 可选；显式 opt-in | ✅ |
| I2.4 | Deployment **`replicas[]` 放置明细** | 对齐 `PlacementAssignment` | 仅当「多副本异构落位」产品承诺时做 |
| I2.5 | Webhook 订阅 | Operation 事件外推 | 有企业集成需求时做 |

**原则：** v1 集成不依赖 I2；I2 不改变 Tier A 冻结语义。

---

### I3 — 集成运维与契约冻结

| ID | 交付 | 验收 | 状态 |
|----|------|------|------|
| I3.1 | `GET /platform/v1/health/summary` | 控制面组件 + ready 副本计数 | ✅ |
| I3.2 | 审计轻量读 API（Postgres 或 xtrace 抽象层） | 不强制集成方部署 xtrace | ✅ |
| I3.3 | `Idempotency-Key` on Control `POST` | 重试安全 | ✅ |
| I3.4 | 半实现路径清理（如 `GET /v1/responses`） | 承诺与实现一致 | ✅ |
| I3.5 | `/platform/v1` OpenAPI 冻结 + 契约 CI | 破坏性变更 → v2 | ✅ |

Tier C 治理读 API（SLO 状态、Canary 阶段）若要做，在 I3 之后单开 **I4 批次**，不挤占 v1。

---

## 6. 总体顺序

```text
I0  单一 control service + compat + 错误/OpenAPI
 └─ I1  /platform/v1（Tier A）
      └─ I2  按需 Tier C（SSE → replica pin → 异构放置 → webhook）
           └─ I3  health / 审计 / 幂等 / v1 冻结
                └─ I4? 治理读 API（另批）
```

I0 阻塞 I1。I2 各项独立排期，**不**作为 v1 完成前提。

---

## 7. 明确不做

- 第三方推理框架 API / ID 兼容  
- 暴露 etcd、Router 管理口、引擎直连 URL 作为业务发现  
- Tier D 控制台能力整体搬进 Control API  
- 推理热路径嵌入 compat / 部署逻辑  
- 用 K8s Service 替代 `/endpoints/` 热路径发现  

---

## 8. 与现有排期

| 批次 | 关系 |
|------|------|
| P0 / C3 | I0、I3 错误与契约 |
| P3 兼容矩阵 | I0.2 Tier A 必选 |
| P6 租户 | I1.5 API Key 继承 tenant 绑定 |
| L2 Pool（[`pool.md`](../arch/pool.md)） | I2.4 异构放置与 Pool 可 later 合并；v1 用 affinity 即可 |
| Console UX | Tier D 不变；与 Control 共享 service |

[`roadmap.md`](../arch/roadmap.md) **Integration I0–I3** 行跟踪本计划。

---

## 9. 完成定义

**Tier A + Inference（I1 完成）即视为 Nebula 集成面 v1 就绪：**

1. 机机方仅用 API Key + `/platform/v1` 完成部署 → Operation → ready → `/v1` 推理 → 停止  
2. 与控制台对同一部署输入结果一致（含 compat）  
3. OpenAPI + `integration.md` 为唯一契约；legacy Admin 有 sunset 日期  

I2 / I3 增强体验与可选能力，不重新定义 v1 最小集。

---

## 10. 文档维护

| 事件 | 更新 |
|------|------|
| 阶段交付 | 本计划、`integration.md`、OpenAPI |
| 新增 Control 写路径 | `ownership.md` |
| 版本发布 | `versions/vX.Y.Z.md` |
