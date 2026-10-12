# L2 资源池与异构调度

> Phase 2 设计分册（**文档先行，代码有需求再开**）。总纲见 [`vision.md`](./vision.md) §4.3；排期见 [`roadmap.md`](./roadmap.md)；K8s 执行面正交见 [`../dev/k8s.md`](../dev/k8s.md)。

## 定位

在「多厂商、多规格、多角色」机房里，用 **HardwarePool** 约束 Scheduler 放哪里，并挡住跨池故障蔓延。不进 Gateway/Router 热路径；不实现引擎内部 PD/worker 或集合通信。

现状基础：platform 过滤、兼容矩阵、单机拓扑副本。本层补的是 **池抽象 + Deployment 池约束 + 拓扑声明 + 故障隔离规范**。

## 不做

- 全自动跨池搬进程、热路径原地迁移
- 引擎内部 prefill/decode 线程或 cache-aware 调度
- 用 Kubernetes API 替代 etcd 作为 Nebula 元数据权威
- 无引擎原生能力时开放跨节点 TP / PD（矩阵拒绝即可）

## 概念

| 概念 | 含义 |
|------|------|
| **HardwarePool** | 逻辑资源集合：`pool_id` + platform / device_class / 角色 + 成员节点（或选择器） |
| **角色** | `general` \| `prefill` \| `decode` \| `edge`（扩展须进兼容矩阵） |
| **拓扑** | `replicated`（默认）\| `multi_node_tp` \| `pd_split`（后两者按需） |
| **池约束** | Deployment 上 `allowed_pools`（空 = 仅受 platform/compat 约束，兼容今日行为） |

```text
Deployment(allowed_pools, topology)
        │
        ▼
Scheduler：节点 ∈ 池 ∩ platform ∩ compat ∩ 健康
        │
        ▼
Placement（CAS）→ Node 拉起 → /endpoints/（lease）
        ▲
Router 只读 endpoints；不知「池」也可选路
```

## 数据模型（v0 草案）

**HardwarePool**（拟 etcd `/pools/{pool_id}`，BFF 写、Scheduler 读）：

- `pool_id`、`display_name`
- `platform`（如 `nvidia-cuda`）、可选 `device_class`
- `role`（默认 `general`）
- `node_selector`：显式 `node_ids` 和/或标签匹配（与 NodeStatus 对齐）
- `capacity_hints`（可选）：水位告警用，不做硬调度真相
- `updated_at_ms`

**Deployment 增量**：

- `allowed_pools: Vec<String>`（空 = 不额外限制）
- `topology: replicated | multi_node_tp | pd_split`（默认 `replicated`）
- `topology_spec`（仅非 replicated）：如 TP size、prefill/decode 池对；字段随引擎能力表演进

**Placement 增量**（按需）：

- `pool_id`（实际落入哪池，便于审计与 Drain）
- 非 replicated 时记录拓扑片段；仍由 Scheduler 唯一 CAS 写

节点不进池：仅 `/nodes/…/status` lease 心跳；池是控制面配置，不是 Node 自报权威。

## Owner 与 etcd

| 对象 | 写者 | 说明 |
|------|------|------|
| `/pools/` | BFF | 持久配置；实现时写入 [`../dev/etcd.md`](../dev/etcd.md) 与 Keyspace 表 |
| Deployment 池/拓扑字段 | BFF | 期望状态唯一写入口不变 |
| Placement | Scheduler | 唯一放置写入口；拒绝双 reconcile |
| `/endpoints/` | Node | lease；Router 热路径只看这里 |

池水位/历史利用率 → Prometheus 或 Postgres，**不**进 etcd 时序。

## 调度规则（实现时）

1. 硬过滤：`allowed_pools` → 池 platform/role → 兼容矩阵 → Capability → 节点健康/lease。
2. 无候选 → Placement 明确失败原因（`no_pool_capacity` / `topology_unsupported` 等），禁止静默落到「别的池」。
3. `multi_node_tp` / `pd_split`：引擎 Capability + compat 均允许才调度；否则 BFF/Scheduler **显式拒绝**。
4. 故障：节点/池标记不可调度；副本 Drain；**不**自动把负载甩到未授权池。
5. 迁移：只发新 Placement 版本 + 可选 Canary；旧副本声明式收缩。

## 与相邻层

| 层 | 关系 |
|----|------|
| L0 | Placement / Endpoint 契约不变；池是 Scheduler 输入约束 |
| L1 | 热路径不知池；协议/错误码不因池化改语义 |
| L3 | Selection 候选可带建议 `pool_id`/拓扑；仍半自动写 Deployment |
| L4 | 成本/容量按池归因（Phase 3）；本阶段只留 `pool_id` 挂钩 |
| K8s/HAMi | 执行面正交：`k8s` runtime 由控制器写 `/endpoints/`；池约束仍经 Deployment→期望，不双写 |
| 外部控制面 | 发现/鉴权适配器不得把 Dynamo·llm-d 类型带进热路径；见 [`control-plane-adapters.md`](./control-plane-adapters.md) |

## `pd_split` 拓扑与角色化放置（P2c-pd 设计稿，Batch 3 第 2 步，2026-10-12）

> 依据：[`../dev/engines.md`](../dev/engines.md) §7（无 NIXL 共享 KV 形态已在 8×5090 实证：producer Stored 681/681 → `lm://` TCP → decoder Retrieved 512/512）。本节把它变成 nebula 可部署能力的设计。**代码未实现**；字段名即实现契约。

### 已实证基础（不是假设）

- 传输：`lmcache_server`（纯 TCP KV 存储）+ 引擎 `remote_url`；零 NIXL、硬件无关（CUDA 实证，昇腾 connector 同构）。
- 角色：prefill 副本 `kv_role=kv_producer`（prefill 后 store 远端）；decode 副本 `kv_role=kv_both`（先 retrieve 远端，miss 则自 prefill——**正确性自兜底**）。
- 依赖注入路径已存在：k8s-controller 已按 `NEBULA_KV_EVENTS_ENDPOINT` 注入 `--kv-events-config`/`PYTHONHASHSEED`/`--prefix-caching-hash-algo`（`create_engine_pod`），本设计沿用同一注入点。
- 失败语义已实证：远端存储不可达 → 引擎 recompute（LMCache `FallbackPolicy.RECOMPUTE`），只损 TTFT 不损可用性。

### 数据模型增量（实现契约）

**`ReplicaPlacementSpec` 增量**（`model_deployment.rs`，serde 全 default，旧数据兼容）：

```rust
pub role: Option<ReplicaRole>,            // prefill | decode | mixed（None=mixed）
// topology=pd_split 时必填且不得为 mixed；replicated 时忽略并告警
```

**`ModelDeployment` 增量**：

```rust
pub topology: Option<DeploymentTopology>, // None=replicated（今日行为）| pd_split
pub kv_sharing: Option<KvSharingSpec>,    // pd_split 时必填
```

**`KvSharingSpec`**（编排参数即 engines.md §7.4 实证值，引擎无关命名）：

```rust
pub remote_url: String,       // lm://host:port
pub serde: String,            // 默认 "naive"
pub hash_seed: String,        // 进 PYTHONHASHSEED，两端一致
```

**`EndpointInfo` 增量**（`endpoint.rs`，serde default=mixed）：

```rust
pub role: ReplicaRole,        // 注册时由 controller/node 按 spec 写入
```

Router 从 `/endpoints/` 既有对象读到角色——**热路径零新增 watch**，符合本文件 L1 边界。

### 校验规则（BFF create/update，拒绝即显式报错）

1. `topology=pd_split` ⇒ 必须 `kv_sharing` 存在、engine_type=vllm、镜像过 LMCache 门禁（`scripts/check_lmcache_compat.sh`）。
2. `pd_split` ⇒ 每个 replica_spec 有非 mixed 角色，且 ≥1 prefill、≥1 decode。
3. SGLang + pd_split ⇒ 拒绝（无 LMCacheConnector，兼容矩阵条目）。
4. `replicated` + 显式角色 ⇒ 拒绝（防误解）。

### 注入映射（controller/node 同一逻辑，两侧执行面）

| 角色 | `--kv-transfer-config` | LMCACHE yaml | 备注 |
|---|---|---|---|
| prefill | `kv_role=kv_producer` | `local_cpu:5 + remote_url + save_unfull_chunk:true` | store 远端 |
| decode | `kv_role=kv_both` | 同上 + `retrieve_locations:[RemoteBackend]` | miss 自 prefill |

`lmcache_server` **v1 由运维拉起**（nebula-stack.sh 增一容器，同 etcd 模式；无状态缓存，重启丢 KV 可接受）；健康检查进 `scripts/probe_stack.sh`。

### 路由语义（Router，v1 规则保持可解释）

```
stage = 若 session_affinity 命中 或 P1c 索引本地命中 → decode
        否则 → prefill
候选 = endpoints[model] 按 stage 过滤 role；对应角色池空 → 回落全体（计数器可见）
```

- 不引入 vLLM disagg proxy：分发由 nebula 完成（engines.md §7.4 形态结论）。
- P1c 与本设计的关系：共享 KV 让"任意 decode 副本可取"，P1c 索引的价值收敛为**优先本地 GPU 命中**（省一次远端取回）与观测——`prefix_affinity` 策略在 decode 池内继续生效，语义不变。
- 会话亲和既有机制（session_affinity map）继续锚定 decode 副本。

### 失败语义

| 故障 | 行为 | 可见性 |
|---|---|---|
| decode 池全灭 | 回落 prefill 池（kv_producer 可完整服务） | `stage_fallback_total`（新计数器） |
| `lmcache_server` 挂 | 引擎 recompute，仅 TTFT 上升 | LMCache 日志 + probe |
| 远端 KV miss | decode 副本自 prefill | LMCache `hit tokens` 日志 |

### 触点清单（第 3 步实现，估 2-3 会话）

1. common：3 个结构体增量 + 校验函数（含单测）。
2. k8s-controller + node：注入映射表（复用 kv-events 注入点）+ EndpointInfo.role 注册。
3. Router：stage 判定 + 角色过滤 + 回落计数器（metrics + BFF）。
4. BFF：校验规则 + 控制台 deployment 编辑暴露 topology/role（admin-ui-change 流程）。
5. scripts：nebula-stack.sh 增 lmcache_server；probe_stack.sh 增健康检查。

## 分阶段落地（有需求再开）

| 步 | 内容 | 验收 |
|----|------|------|
| P2a | `HardwarePool` CRUD + Deployment `allowed_pools` + Scheduler 硬过滤 | 两池混部；未授权池放不进去 |
| P2b | 池/节点不可调度 + Drain 规范 | 杀一池成员不拖垮他池副本 |
| P2c | `multi_node_tp` / `pd_split`（pd_split 设计见上节，2026-10-12） | 不支持拓扑被拒绝；支持则 Placement 可解释 |

N3 D4/D5（跨节点 TP / EngineShim）挂在 P2c，无客户需求不提前实现。

## 成功标准（产品）

多池可部署；不支持拓扑被显式拒绝；故障不跨池蔓延；Router 热路径零改或仅读既有 endpoints。
