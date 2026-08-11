# 上层平台集成 Nebula

> **读者：** 要把 Nebula 当作「推理控制面」嵌入自有平台（算电、ISV、业务中台等）的集成工程师。  
> **基线：** Nebula v1.6.0 · Gateway 默认 `:8081`  
> **边界：** 本文只描述 **Nebula 原生契约**；Nebula 与 Xinference / PowerLLM 独立，不提供兼容层。  
> **相关：** 安装见 [`../manual/deployment.md`](../manual/deployment.md)；错误码见 [`contracts.md`](./contracts.md)；架构见 [`../arch/architecture.md`](../arch/architecture.md)。  
> **演进计划：** [`integration-plan.md`](./integration-plan.md)（**I0–I6 ✅**）。  
> **OpenAPI：** [`openapi-control.yaml`](./openapi-control.yaml)（仅 `/platform/v1`）。

---

## 1. 集成模型

上层平台与 Nebula 之间只有两类交互：

| 交互 | 谁发起 | 入口 | 频率 |
|------|--------|------|------|
| **推理** | 上层 / 业务 | Gateway `POST /v1/*` | 高 |
| **编排** | 上层平台 | Gateway `/platform/v1/*` | 低 |
| **节点执行** | 运维 / 平台下发 | 各 GPU 机器跑 `nebula-node` | 常驻 |

Router、Scheduler、etcd **不对上层暴露**。上层不要直连 `:18081` Router 或 `:2379` etcd。

```
上层平台 / 业务
    │  Bearer token（同一套 NEBULA_AUTH_TOKENS）
    ├─ POST /v1/chat/completions …     推理
    └─ POST /platform/v1/models/load …    部署 / 扩缩 / 查状态
           │
           ▼
    Gateway :8081 ──► Router ──► vLLM / SGLang
           │
           └── 写 /deployments/ ──► Scheduler ──► Node（各机 agent）
```

**BFF（`:18090`）** 面向控制台网页，鉴权是 Postgres 登录会话，**不是**平台集成的推荐路径（见 §8）。

---

## 2. 前置条件

### 2.1 控制面已就绪

至少运行：etcd、Scheduler、Router、Gateway；GPU 节点上运行 `nebula-node`。一键本地栈见 [`../manual/deployment.md`](../manual/deployment.md) 第 10 节。

生产集成建议：

- Gateway 可多副本（接入面 HA）；etcd 三节点见 [`../manual/etcd-ha.md`](../manual/etcd-ha.md)。
- 开启鉴权：`NEBULA_AUTH_TOKENS`，**不要**在生产使用 `NEBULA_AUTH_DISABLED=1`。

### 2.2 节点 ID 对齐

每台 GPU 机器启动 Node 时指定 ID，与上层平台的 node 标识一致：

```bash
nebula-node \
  --node-id <平台 nodeId> \
  --etcd-endpoints http://etcd:2379
```

Node 心跳写入 etcd `/nodes/{node_id}/status`。部署 API 里的 `node_id` **必须**与 `--node-id` 相同，Scheduler 才能把副本落到该机器。

`model_uid` 规则：`[a-z0-9][a-z0-9-]*`，最长 63 字符，创建后不可改。

### 2.3 健康检查

| 路径 | 鉴权 | 用途 |
|------|------|------|
| `GET /healthz` | 无 | 存活探针 |
| `GET /metrics` | 无 | Prometheus 抓取 Gateway 指标 |

---

## 3. 鉴权

### 3.1 Token 配置

环境变量 `NEBULA_AUTH_TOKENS`，逗号分隔，格式：

```
<token>:<role>
<token>:<role>:<tenant_id>
```

| role | 权限 |
|------|------|
| `viewer` | 读：cluster 状态、模型列表、日志等 |
| `operator` | viewer + 部署 / 扩缩 / 停止 / drain |
| `admin` | operator + 审计日志、镜像注册写删等 |

示例：

```bash
export NEBULA_AUTH_TOKENS="platform-ctrl:operator,platform-ro:viewer,acme-infer:operator:acme"
```

### 3.2 请求头

所有受保护路径（`/v1/*` 除公开 health/metrics 外）：

```http
Authorization: Bearer <token>
Content-Type: application/json
```

验证身份：

```bash
curl -s http://<gateway>:8081/platform/v1/whoami \
  -H "Authorization: Bearer platform-ctrl"
# {"principal":"platform-ctrl","role":"operator"}
```

### 3.3 限流

`NEBULA_AUTH_RATE_LIMIT_PER_MINUTE`（默认 120）：按 token 或租户计数，超限返回 429。

### 3.4 多租户（可选）

`NEBULA_MULTI_TENANT=1` 时：

- 租户身份由 token 第三段绑定（`token:role:tenant_id`），**不可**用客户端 header 伪造。
- Gateway 读 etcd `/tenants/{id}` 做配额准入；拒绝时 header `x-nebula-deny-code` + JSON 错误体（见 [`contracts.md`](./contracts.md) C3）。

### 3.5 Postgres API Key（可选，I1）

设置 `NEBULA_PLATFORM_DB_URL`（或与 BFF 共用 `NEBULA_BFF_DATABASE_URL`）后，Gateway 启动时自动建表 `platform_api_keys`，Bearer token 除 env 映射外也可查库。

| scope | 允许路径 |
|-------|----------|
| `inference` | `/v1/*`（不含 `/platform/`） |
| `control` | `/platform/v1/*` |
| `admin` | 全部受保护路径 |

Key 以 SHA-256 哈希存储。插入示例（需自行生成随机 secret）：

```sql
INSERT INTO platform_api_keys (name, key_hash, role, scopes)
VALUES (
  'platform-ctrl',
  encode(sha256('your-secret-key'::bytea), 'hex'),
  'operator',
  ARRAY['inference','control']
);
```

也可运行 `scripts/platform_api_key.sh create --name platform-ctrl --role operator --scopes inference,control`。

---

## 4. 推理 API（热路径）

Base：`http://<gateway-host>:8081`

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/chat/completions` | OpenAI Chat（流式 / 非流式） |
| POST | `/v1/completions` | Legacy completions |
| POST | `/v1/embeddings` | Embeddings |
| POST | `/v1/rerank` | Rerank |
| POST | `/v1/messages` | Anthropic Messages（经 Gateway 适配） |
| POST | `/v1/responses` | OpenAI Responses |
| GET | `/v1/models` | 已部署模型列表（OpenAI 形状） |

请求体 `model` 填**对外模型名**（通常与 `ModelSpec.model_name` 或 `--served-model-name` 一致）。Gateway 注入 `x-nebula-model` 后转发 Router；Router 按负载、熔断、可选 session 亲和选副本，**上层无需知道引擎 URL**。

### 4.1 示例

```bash
curl -s http://127.0.0.1:8081/v1/chat/completions \
  -H "Authorization: Bearer platform-ctrl" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "Qwen/Qwen2.5-0.5B-Instruct",
    "messages": [{"role": "user", "content": "你好"}]
  }'
```

流式：加 `"stream": true`，响应 `text/event-stream`；客户端断开时 Gateway 向上游 abort（不计入 5xx SLO，见 [`contracts.md`](./contracts.md) C2）。

### 4.2 请求上下文 Header（可选）

Gateway 从入站 header 构建 `ExecutionContext` 并注入下游：

| Header | 含义 |
|--------|------|
| `traceparent` | W3C Trace Context；与外部 OTel 链路贯通（推荐） |
| `x-nebula-request-id` | 请求 ID；缺省自动生成 |
| `x-session-id` | 会话 ID；Router 对同一 session 做副本亲和（非强制 pin） |
| `x-nebula-priority` | 整数优先级 |
| `x-nebula-deadline-ms` | 截止时间（Unix ms） |
| `x-nebula-budget-tokens` | 预估 token 预算（多租户准入用） |

**Trace：** 上层若已有 OpenTelemetry，请发标准 `traceparent`（及可选 `tracestate`）。Gateway → Router → Engine 全 hop 继承，OTLP 导出至 xtrace（`OBSERVE_URL`）。不使用 Langfuse 专有 header。

**租户：** 多租户模式下忽略客户端 `x-nebula-tenant-id`，以 token 绑定为准。

### 4.3 推理错误码

Gateway 对外稳定 JSON 映射见 [`contracts.md`](./contracts.md) C3（502 上游不可达、503 无 ready endpoint、429 过载 / 配额、413 体过大等）。引擎返回的 OpenAI 形态错误原样转发。

---

## 5. 控制 API（编排）

**契约：** `/platform/v1/*`（OpenAPI [`openapi-control.yaml`](./openapi-control.yaml)）。v1.6.0 起 **已移除** `/v1/admin/*` 与 Gateway→BFF `/v1/admin/v2/*` 代理；控制台能力走 BFF `:18090/api/v2/*`，机机集成只用 Platform v1。

所需角色：读 `viewer`，写 `operator`；与推理面共用 Bearer token 或 Postgres API Key（§3.5）。

Base：`http://<gateway-host>:8081/platform/v1`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET/POST | `/models` | 列出 / 创建 `ModelSpec` |
| POST | `/models/load` | 一步 upsert spec + 启动 deployment（compat 校验） |
| GET | `/models/{uid}` | 读 spec |
| GET/PUT | `/models/{uid}/deployment` | 读 / 写 deployment 期望 |
| POST | `/models/{uid}/deployment/scale` | 扩缩容 |
| POST | `/models/{uid}/stop` | 停止 |
| GET | `/models/{uid}/replicas` | 运行副本（含 `status`、`node_id`、`base_url`） |
| GET | `/nodes` | 节点 inventory |
| GET | `/operations/{id}` | 异步操作状态（`ready_replicas` / `succeeded`） |
| GET | `/operations/{id}/events` | SSE 推送 operation 状态（替代轮询） |
| GET | `/health/summary` | 控制面健康摘要 |
| GET | `/audit-logs` | 审计只读（admin；需 xtrace） |
| GET | `/cluster/status` | 集群快照（nodes / endpoints / placements） |
| GET | `/whoami` | 当前 principal / role |
| POST | `/replicas/drain` | 副本摘流 |
| GET/POST/DELETE | `/webhooks` | Operation 事件 Webhook 订阅（需 Postgres） |

写操作返回 **202** + `operation_id`；可轮询 `GET /operations/{id}`、订阅 SSE，或配置 **Webhook**（见下）直到 `succeeded`。支持 `Idempotency-Key` 请求头（24h 内同 key + 同 body 返回同一 `operation_id`）。

**异构放置（I2.4）：** 写 deployment 时可选 `replica_specs` 数组，长度须等于 `replicas`；每项可指定 `node_id`、`gpu_indices`、`config_overrides`、`image_id`。未指定字段继承 deployment 级 `node_id` / `gpu_indices`。Scheduler 按序 reconcile 为 per-replica `PlacementAssignment`。

**Operation Webhook（I2.5）：**

- **单次回调：** 写请求体 `callback_url`（HTTPS），或 `POST …/stop` 时使用请求头 `X-Nebula-Callback-Url`。
- **持久订阅：** `POST /platform/v1/webhooks` 注册 `{ "name", "url", "secret" }`（需 `NEBULA_PLATFORM_DB_URL`）；Operation 状态变更时 POST 全量 `Operation` JSON，带 `X-Nebula-Signature: sha256=…`（HMAC-SHA256）。
- 交付为 best-effort，不阻塞 Operation 完成。

推理响应会回显 `x-nebula-request-id`；可选请求头 `x-nebula-replica-id` 固定 Router 选路到指定副本。

**治理读（I4，只读）：** `GET /models/{uid}/slo`、`GET …/slo/evaluation`、`GET /canaries`（可选 `?model_uid=`）、`GET /canaries/{id}`。SLO/Canary 写操作仍走控制台 BFF。

**示例：一步部署**

```bash
curl -s -X POST http://127.0.0.1:8081/platform/v1/models/load \
  -H "Authorization: Bearer platform-ctrl" \
  -H "Content-Type: application/json" \
  -d '{
    "model_uid": "qwen3-prod",
    "model_name": "Qwen/Qwen3-8B",
    "replicas": 1,
    "node_id": "gpu-node-01",
    "gpu_indices": [0],
    "engine_type": "vllm"
  }'
# {"operation_id":"op_…","model_uid":"qwen3-prod","status":"pending"}

curl -s http://127.0.0.1:8081/platform/v1/operations/op_… \
  -H "Authorization: Bearer platform-ctrl"
# {"status":"running","ready_replicas":0,"desired_replicas":1,…}
```

---

## 6. 典型集成流程

```
1. 平台在 GPU 机器安装并启动 nebula-node（--node-id 与平台 nodeId 一致）
2. POST /platform/v1/models/load（可选 node_id、gpu_indices）
3. 轮询 GET /platform/v1/operations/{id} 直到 succeeded；或 GET …/replicas 直到 ready
4. 业务 POST /v1/chat/completions（model = model_name）
5. 需要下线：POST /platform/v1/models/{uid}/stop
```

**超时建议：** 首次部署含模型下载，可能数分钟至数十分钟；轮询间隔 2–5s，总超时由平台按模型大小配置。

**幂等：** 对同一 `model_uid` 重复 `load` 会更新 spec / deployment 并 bump version，Scheduler 重新 reconcile。

---

## 7. ID 与对象映射

| 概念 | Nebula 字段 | 说明 |
|------|-------------|------|
| 平台节点 ID | `nebula-node --node-id` = `NodeStatus.node_id` = 部署 `node_id` | 三者必须一致 |
| 模型 ID | `model_uid` | 平台侧持久化为主键 |
| 对外模型名 | `model_name` / `config.served_model_name` | 推理请求 `model` 字段 |
| 副本 | `replica_id`（u32，从 0 递增） | 出现在 placement / endpoint；**不能**通过推理 API 指定 |
| 放置计划版本 | `plan_version` / `PlacementPlan.version` | Router 选路可约束版本 |

---

## 8. 不要用的路径

| 路径 | 原因 |
|------|------|
| BFF `:18090/api/v2/*` | 鉴权为控制台登录 session，非机机 API Key |
| Gateway `/v1/admin/*` | **v1.6.0 已移除**；用 `/platform/v1/*` |
| Router `:18081` | 无 Gateway 鉴权 / 审计 / 租户门控；破坏安全边界 |
| etcd | 无稳定对外契约；写入口归属见 [`ownership.md`](./ownership.md) |

控制台能力（Benchmark、SLO 写、模板、租户 CRUD、镜像注册表）走 BFF；机机集成用 `/platform/v1/*`。

---

## 9. 观测

| 能力 | 方式 |
|------|------|
| 指标 | Gateway `/metrics`、Router `:18081/metrics`（Prometheus） |
| 链路 | 请求带 `traceparent` → xtrace（`OBSERVE_URL`） |
| 审计 | `GET /platform/v1/audit-logs`（admin + xtrace） |

平台侧建议：用自有 trace_id 与 `x-nebula-request-id` 关联；不要在 Prometheus 上为高基数 tenant / user 打 label（明细走审计 / trace）。

---

## 10. 实现状态、前提与限制

本文 §4–§5 描述的 **Gateway 推理 + Platform 控制面** 在 v1.6.0 均已注册（路由见 `crates/nebula-gateway/src/main.rs`）。

### 10.1 已实现的接口

| 类别 | 路径 | 说明 |
|------|------|------|
| 推理 | `POST /v1/chat/completions` 等 §4 所列 | Gateway 鉴权 → Router → 引擎 |
| 推理 | `GET /v1/models` | OpenAI 列表形状 |
| 编排 | `/platform/v1/*` §5 | Operation、Webhook、异构放置 |
| 鉴权 | API Key 或 `NEBULA_AUTH_TOKENS` | 推理与 Control 共用 scope |

### 10.2 运行前提（缺一则集成失败）

| 前提 | 后果 |
|------|------|
| **Scheduler 在跑** 且 watch `/deployments/` | 仅 `load` 无 placement，Node 不会起引擎 |
| **Router 在跑** 且 Gateway `NEBULA_ROUTER_URL` 正确 | 推理 502 / 连不上 |
| **目标机 `nebula-node` 在跑** 且 `--node-id` 与部署 `node_id` 一致 | 有 placement 无 endpoint |
| **etcd 可达** | 部署与状态查询均失败 |
| **生产开启鉴权**（勿用 `NEBULA_AUTH_DISABLED`） | 裸奔，非集成契约 |

首次部署常含模型下载，ready 可能需数分钟；须轮询 `GET /platform/v1/operations/{id}` 或 `…/replicas`，不能假设 `load` 响应即 ready。

### 10.3 与控制台（BFF）的行为差异

平台走 Gateway Admin 时，下列行为**与控制台页面不完全相同**，集成方需知晓：

| 项 | `/platform/v1` | 控制台（BFF `/api/v2/*`） |
|----|-------------------------------|---------------------------|
| **写路径** | 均走 `nebula-control`（与 BFF 同源） | 同一 service 层 |
| **兼容矩阵** | I0 起部署写入口统一 compat 校验 | 同左 |
| **鉴权** | API Key 或 `NEBULA_AUTH_TOKENS` | Postgres 登录 session |
| **治理写** | 无（SLO/Canary 写仍走 BFF） | 完整 CRUD |

### 10.4 接口级 caveat

| 接口 | 现状 |
|------|------|
| `POST /v1/embeddings`、`/v1/rerank` | Gateway/Router 已代理；**是否成功取决于引擎**是否实现该路径 |
| `GET /v1/responses` | **未实现**（501）；仅 `POST /v1/responses` 可用 |
| `GET /platform/v1/audit-logs` | 需 Gateway 配置 `OBSERVE_URL` + `OBSERVE_TOKEN`（xtrace）；未配返回 503 |
| `min_replicas` / `max_replicas` | 写入 deployment 后由 **Scheduler 自动扩缩**；须 `min < max` 且 Scheduler 在跑 |

### 10.5 产品级限制（非 bug）

| 项 | 现状 |
|----|------|
| **指定 replica 推理** | 可选：请求头 `x-nebula-replica-id` 固定 Router 选路；默认仍自动负载均衡 + `x-session-id` 亲和 |
| **Per-replica 异构放置** | 支持：`replica_specs[]`（长度 = `replicas`），每项独立 `node_id` / `gpu_indices` |
| **BFF 机机 API** | 无；集成统一走 Gateway `/platform/v1` |
| **纳管外部已有推理集群** | 非产品承诺路径 |
| **训练 / 云 API 聚合** | 不在 Nebula 范围 |

---

## 11. 环境变量速查

| 变量 | 组件 | 说明 |
|------|------|------|
| `NEBULA_GATEWAY_ADDR` | Gateway | 默认 `0.0.0.0:8081` |
| `NEBULA_ROUTER_URL` | Gateway | Router 地址 |
| `NEBULA_AUTH_TOKENS` | Gateway | 集成 token |
| `NEBULA_AUTH_DISABLED` | Gateway | 仅开发 |
| `NEBULA_MULTI_TENANT` | Gateway | 租户配额 |
| `NEBULA_AUTH_RATE_LIMIT_PER_MINUTE` | Gateway | 每分钟限流 |
| `OBSERVE_URL` / `OBSERVE_TOKEN` | Gateway / 各组件 | xtrace / OTLP |
| `ETCD_ENDPOINT` | 各组件 | 元数据（上层不直连） |
| `NODE_ID` | nebula-up / Node | 与 `--node-id` 等价配置 |

完整列表：`deploy/nebula.env.example`、`docs/manual/deployment.md`。

---

## 12. 代码锚点（§10 对照）

| 契约 | 代码 |
|------|------|
| Gateway 路由 | `crates/nebula-gateway/src/main.rs` |
| Platform v1 handlers | `crates/nebula-gateway/src/platform_v1.rs` |
| Admin load / scale / status | `crates/nebula-gateway/src/handlers.rs` |
| Control service | `crates/nebula-control/` |
| 推理代理与 header 注入 | `crates/nebula-gateway/src/proxy_common.rs` |
| 鉴权 | `crates/nebula-common/src/auth.rs` |
| ExecutionContext | `crates/nebula-common/src/execution_context.rs` |
| Trace | `crates/nebula-common/src/telemetry.rs` |
| 部署 / 放置类型 | `crates/nebula-common/src/model_deployment.rs`、`placement.rs` |

契约变更须同步更新本文与 [`contracts.md`](./contracts.md)。
