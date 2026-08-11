# 上层平台集成 Nebula

> **读者：** 要把 Nebula 当作「推理控制面」嵌入自有平台（算电、ISV、业务中台等）的集成工程师。  
> **基线：** Nebula v1.4.0 · Gateway 默认 `:8081`  
> **边界：** 本文只描述 **Nebula 原生契约**；Nebula 与 Xinference / PowerLLM 独立，不提供兼容层。  
> **相关：** 安装见 [`../manual/deployment.md`](../manual/deployment.md)；错误码见 [`contracts.md`](./contracts.md)；架构见 [`../arch/architecture.md`](../arch/architecture.md)。  
> **演进计划：** [`integration-plan.md`](./integration-plan.md)（**I0 ✅ · I1 ✅**；I2–I3 拟议）。  
> **OpenAPI：** [`openapi-control.yaml`](./openapi-control.yaml)（I1 `/platform/v1`；legacy 已标 deprecated）。

---

## 1. 集成模型

上层平台与 Nebula 之间只有两类交互：

| 交互 | 谁发起 | 入口 | 频率 |
|------|--------|------|------|
| **推理** | 上层 / 业务 | Gateway `POST /v1/*` | 高 |
| **编排** | 上层平台 | Gateway `/platform/v1/*`（推荐）或 legacy `/v1/admin/*` | 低 |
| **节点执行** | 运维 / 平台下发 | 各 GPU 机器跑 `nebula-node` | 常驻 |

Router、Scheduler、etcd **不对上层暴露**。上层不要直连 `:18081` Router 或 `:2379` etcd。

```
上层平台 / 业务
    │  Bearer token（同一套 NEBULA_AUTH_TOKENS）
    ├─ POST /v1/chat/completions …     推理
    └─ POST /platform/v1/models/load …    部署 / 扩缩 / 查状态（推荐）
    └─ legacy POST /v1/admin/models/load …（deprecated）
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
curl -s http://<gateway>:8081/v1/admin/whoami \
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
| `inference` | `/v1/*`（不含 `/v1/admin`） |
| `control` | `/platform/v1/*` 与 legacy `/v1/admin/*` |
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

**推荐：** `/platform/v1/*`（I1 正式契约，OpenAPI [`openapi-control.yaml`](./openapi-control.yaml)）。  
**Legacy：** `/v1/admin/*` 仍可用但响应带 `Deprecation: true`，见 §5.2。

所需角色：读 `viewer`，写 `operator`；与推理面共用 Bearer token 或 Postgres API Key（§3.5）。

### 5.1 Platform v1（推荐）

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

写操作返回 **202** + `operation_id`；轮询 `GET /operations/{id}` 直到 `status: succeeded`（或 `failed`），无需解析 placement JSON。

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

### 5.2 Legacy Admin（deprecated）

Base：`http://<gateway-host>:8081/v1/admin`

#### 部署模型

`POST /v1/admin/models/load`

写入 `/models/{model_uid}/spec` 与 `/deployments/{model_uid}`；Scheduler 异步 reconcile 生成 `/placements/`；Node 启引擎并注册 `/endpoints/`。

**请求体**（`ModelLoadRequest`）：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `model_uid` | string | 是 | 全局唯一 ID |
| `model_name` | string | 是 | 对外名 / HF ID，如 `Qwen/Qwen2.5-7B-Instruct` |
| `replicas` | u32 | 否 | 默认 1 |
| `node_id` | string | 否 | 节点亲和；须与 `nebula-node --node-id` 一致 |
| `gpu_index` | u32 | 否 | 单卡（legacy） |
| `gpu_indices` | [u32] | 否 | 多卡 TP，如 `[0,1]` |
| `min_replicas` / `max_replicas` | u32 | 否 | 自动扩缩边界 |
| `engine_type` | string | 否 | `vllm` / `sglang`，默认 vLLM |
| `docker_image` | string | 否 | 引擎镜像覆盖 |
| `config` | object | 否 | 见下表 |

**config 常用字段**（`ModelConfig`）：

| 字段 | 说明 |
|------|------|
| `tensor_parallel_size` | TP 大小 |
| `gpu_memory_utilization` | 显存占用比例 |
| `max_model_len` | 最大上下文 |
| `served_model_name` | 引擎 `--served-model-name` |
| `required_vram_mb` | 调度显存预估 |

**示例：指定节点与 GPU**

```bash
curl -s -X POST http://127.0.0.1:8081/v1/admin/models/load \
  -H "Authorization: Bearer platform-ctrl" \
  -H "Content-Type: application/json" \
  -d '{
    "model_uid": "qwen3-prod",
    "model_name": "Qwen/Qwen3-8B",
    "replicas": 1,
    "node_id": "gpu-node-01",
    "gpu_indices": [0, 1],
    "engine_type": "vllm",
    "config": {
      "tensor_parallel_size": 2,
      "gpu_memory_utilization": 0.9
    }
  }'
```

**响应**（200）：

```json
{
  "request_id": "qwen3-prod",
  "model_uid": "qwen3-prod",
  "status": "running_desired",
  "path": "deployments"
}
```

表示**期望已写入**，非引擎已 ready。须轮询 §5.5 直到 endpoint `status: "ready"`。

**注意：** Gateway Admin **不做**兼容矩阵校验（与控制台 BFF 不同），见 §10.3。

**语义：** `node_id` / `gpu_indices` 为**部署级亲和**（整次 deployment 的约束）。多副本默认同策略下由 Scheduler 选位；**不是** per-replica 独立 `replica_config` 数组。

### 5.2 扩缩容

`PUT /v1/admin/models/requests/:id/scale`

`:id` 为 `model_uid`（或 legacy `model_requests` id，Gateway 会解析）。

```bash
curl -s -X PUT http://127.0.0.1:8081/v1/admin/models/requests/qwen3-prod/scale \
  -H "Authorization: Bearer platform-ctrl" \
  -H "Content-Type: application/json" \
  -d '{"replicas": 2}'
```

响应含 `old_replicas` / `new_replicas`。缩容时 Nebula 对旧副本做 drain（先不接新请求再停），见架构文档。

### 5.3 停止

`DELETE /v1/admin/models/requests/:id`

将 deployment `desired_state` 置为 `stopped`，Scheduler / Node 回收副本。

```bash
curl -s -X DELETE http://127.0.0.1:8081/v1/admin/models/requests/qwen3-prod \
  -H "Authorization: Bearer platform-ctrl"
```

### 5.4 副本 Drain（运维）

`POST /v1/admin/endpoints/drain`

```json
{ "model_uid": "qwen3-prod", "replica_id": 0 }
```

将指定 endpoint 标为 `draining`，用于上线前摘流或缩容配合。

### 5.5 集群状态（集成轮询）

`GET /v1/admin/cluster/status`（viewer+）

返回 `ClusterStatus`：

```json
{
  "nodes": [ { "node_id": "...", "gpus": [...], "last_heartbeat_ms": ... } ],
  "endpoints": [ {
    "model_uid": "qwen3-prod",
    "replica_id": 0,
    "node_id": "gpu-node-01",
    "status": "ready",
    "base_url": "http://127.0.0.1:10814",
    "plan_version": 1
  } ],
  "placements": [ { "model_uid": "...", "assignments": [...] } ],
  "model_requests": []
}
```

**集成就绪判定：**

1. 目标 `node_id` 出现在 `nodes` 且心跳新鲜；
2. 存在 `endpoints` 条目：`model_uid` 匹配且 `status == "ready"`；
3. `placements.assignments` 中 replica 数与期望 `replicas` 一致（可选交叉校验）。

`endpoint.status` 枚举：`starting` | `ready` | `unhealthy` | `draining` | `failed`。

### 5.6 其他 Admin 接口

| 方法 | 路径 | 角色 | 说明 |
|------|------|------|------|
| GET | `/v1/admin/models/requests` | viewer | Legacy 请求列表 |
| GET | `/v1/admin/whoami` | 任意已认证 | 当前 principal / role |
| GET | `/v1/admin/logs?lines=200` | viewer | Gateway 日志 tail |
| GET | `/v1/admin/logs/stream` | viewer | SSE 日志流 |
| GET | `/v1/admin/images` | viewer | 引擎镜像注册表 |
| GET/PUT/DELETE | `/v1/admin/images/:id` | viewer / operator / admin | 镜像 CRUD |
| GET | `/v1/admin/images/status` | viewer | 各节点镜像拉取状态 |
| GET | `/v1/admin/audit-logs` | admin | 审计；须 xtrace，见 §10.4 |

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
| BFF `:18090/api/v2/*` | 鉴权为控制台登录 session，非 `NEBULA_AUTH_TOKENS` |
| Gateway `/v1/admin/v2/*` | 转发 BFF，同样依赖 session token，不适合机机集成 |
| Router `:18081` | 无 Gateway 鉴权 / 审计 / 租户门控；破坏安全边界 |
| etcd | 无稳定对外契约；写入口归属见 [`ownership.md`](./ownership.md) |

控制台能力（Benchmark、SLO、模板、租户 CRUD）若平台需要，应通过 **Gateway Admin 已暴露能力** 或单独排期 **平台专用 API**，而不是复用 BFF 会话模型。

---

## 9. 观测

| 能力 | 方式 |
|------|------|
| 指标 | Gateway `/metrics`、Router `:18081/metrics`（Prometheus） |
| 链路 | 请求带 `traceparent` → xtrace（`OBSERVE_URL`） |
| 审计 | Admin `GET /v1/admin/audit-logs`（admin + xtrace） |

平台侧建议：用自有 trace_id 与 `x-nebula-request-id` 关联；不要在 Prometheus 上为高基数 tenant / user 打 label（明细走审计 / trace）。

---

## 10. 实现状态、前提与限制

本文 §4–§5 描述的 **Gateway 推理 + Admin 编排** 在 v1.4.0 代码中均已注册并实现（路由见 `crates/nebula-gateway/src/main.rs`）。能否在生产联调中一次跑通，还取决于下面运行前提与行为差异。

### 10.1 已实现的接口（代码已落地）

| 类别 | 路径 | 说明 |
|------|------|------|
| 推理 | `POST /v1/chat/completions` 等 §4 所列 | Gateway 鉴权 → Router 选路 → 引擎 Passthrough |
| 推理 | `POST /v1/messages`、`POST /v1/responses` | Gateway 协议适配后走同一路由 |
| 推理 | `GET /v1/models` | 读 etcd placement，OpenAI 列表形状 |
| 编排 | `POST /v1/admin/models/load` | 写 spec + deployment |
| 编排 | `PUT …/scale`、`DELETE …/requests/:id` | 改副本 / 停止 |
| 编排 | `GET /v1/admin/cluster/status` | 节点 + endpoint + placement 快照 |
| 编排 | `POST /v1/admin/endpoints/drain` | 副本摘流 |
| 鉴权 | `NEBULA_AUTH_TOKENS` + `Authorization: Bearer` | 推理与 Admin 共用 |
| 追踪 | `traceparent` + ExecutionContext header | Gateway→Router inject/extract |
| 多租户 | `NEBULA_MULTI_TENANT=1` + `token:role:tenant_id` | 可选 |

`admin/models/load` 写入的 `node_id` / `gpu_indices` 会进入 deployment；Scheduler `planner.rs` 读取 `node_affinity` / `gpu_affinity` 生成 placement（需 Scheduler 进程在跑）。

### 10.2 运行前提（缺一则集成失败）

| 前提 | 后果 |
|------|------|
| **Scheduler 在跑** 且 watch `/deployments/` | 仅 `load` 无 placement，Node 不会起引擎 |
| **Router 在跑** 且 Gateway `NEBULA_ROUTER_URL` 正确 | 推理 502 / 连不上 |
| **目标机 `nebula-node` 在跑** 且 `--node-id` 与部署 `node_id` 一致 | 有 placement 无 endpoint |
| **etcd 可达** | 部署与状态查询均失败 |
| **生产开启鉴权**（勿用 `NEBULA_AUTH_DISABLED`） | 裸奔，非集成契约 |

首次部署常含模型下载，ready 可能需数分钟；须轮询 `cluster/status`，不能假设 `load` 响应即 ready。

### 10.3 与控制台（BFF）的行为差异

平台走 Gateway Admin 时，下列行为**与控制台页面不完全相同**，集成方需知晓：

| 项 | Gateway Admin | 控制台（BFF `/api/v2/*`） |
|----|---------------|---------------------------|
| **Gateway Admin** | **走 `nebula-control`（与 BFF 同源）** | 部署前校验引擎/GPU/镜像 |
| **鉴权** | `NEBULA_AUTH_TOKENS` | Postgres 登录 session |
| **失败时机** | 不兼容配置可能到 Node 启引擎时才失败 | 部署请求可能被 BFF 直接拒绝 |

若平台需要兼容门禁，当前应自行在调用前做规则校验，或后续在 Gateway Admin 补同等校验（产品排期项）。

### 10.4 接口级 caveat

| 接口 | 现状 |
|------|------|
| `POST /v1/embeddings`、`/v1/rerank` | Gateway/Router 已代理；**是否成功取决于引擎**是否实现该路径 |
| `GET /v1/responses` | **未实现**（501）；仅 `POST /v1/responses` 可用 |
| `GET /v1/admin/audit-logs` | 需 Gateway 配置 `OBSERVE_URL` + `OBSERVE_TOKEN`（xtrace）；未配返回 503 |
| `GET /v1/admin/logs` | 读 Gateway 本地 `NEBULA_GATEWAY_LOG_PATH`，非集群统一日志 |
| `min_replicas` / `max_replicas` | 写入 deployment 后由 **Scheduler 自动扩缩**；须 `min < max` 且 Scheduler 在跑 |

### 10.5 产品级限制（非 bug）

| 项 | 现状 |
|----|------|
| **指定 replica 推理** | 不支持。Router 自动选路 + 可选 `x-session-id` 亲和；不能 `replica_id` pin |
| **Per-replica 异构放置** | 部署级 `node_id` / `gpu_indices`；多副本跨不同节点需多次单副本部署或等产品扩展 |
| **BFF 机机 API** | 无；集成统一走 Gateway |
| **纳管外部已有推理集群** | 非产品承诺路径 |
| **训练 / 云 API 聚合** | 不在 Nebula 范围 |

Replica 级路由（如 `x-nebula-replica-id`）若成为平台硬性需求，需在 Nebula 产品侧单独立项。

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
| Admin load / scale / status | `crates/nebula-gateway/src/handlers.rs` |
| 推理代理与 header 注入 | `crates/nebula-gateway/src/proxy_common.rs` |
| 鉴权 | `crates/nebula-common/src/auth.rs` |
| ExecutionContext | `crates/nebula-common/src/execution_context.rs` |
| Trace | `crates/nebula-common/src/telemetry.rs` |
| 部署 / 放置类型 | `crates/nebula-common/src/model_deployment.rs`、`placement.rs` |

契约变更须同步更新本文与 [`contracts.md`](./contracts.md)。
