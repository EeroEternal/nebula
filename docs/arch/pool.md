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

## `pd_split` 拓扑与角色化放置（P2c-pd 设计稿 v1.1，2026-10-12，经独立评审 REWORK 修订）

> 依据：[`../dev/engines.md`](../dev/engines.md) §7（无 NIXL 共享 KV 形态已在 8×5090 实证：producer Stored 681/681 → `lm://` TCP → decoder Retrieved 512/512）。**代码未实现**；字段名即实现契约。v1.1 修订吸收独立评审（1 blocker + 5 major）：重新定位 v1 语义、补生命周期/故障验证/产物绑定/多 owner 索引/seed 契约。

### v1 定位（评审 #1 修正）：角色化完整请求路由，**不是**单请求 PD

v1 是**共享 KV 之上的角色偏置完整请求路由**：每个请求由单一副本完整服务（prefill 副本也会 decode；decode 副本 miss 时自 prefill）。它不是"prefill→KV 写完→decode 续接"的单请求阶段分离——那需要交接、首 token 归属、流式切换、取消与计费契约，**列为 v2 开放设计**（ nebula 自研协调器或届时再评估上游 proxy），未经设计与实证不得宣称。

### 已实证基础（与未实证声明分开）

已实证（rig，见 engines.md §7.4）：
- 传输链路：`lmcache_server`（纯 TCP）+ 引擎 `remote_url`；零 NIXL；CUDA 端到端（Stored/Retrieved 681/512 tokens）。
- 角色：`kv_producer` store 远端；`kv_both` retrieve、miss 自 prefill（完整请求能力）。
- 注入点：controller `create_engine_pod` 已按 env 注入 kv-events/seed/algo；node 同构。
- 升级门禁：`scripts/check_lmcache_compat.sh` quick/full 两模式 8/8。

**未实证，不得作为设计前提**（评审 #2/#4 修正）：
- 存储故障（不可达/断线/黑洞/传输中断）下的回退行为——`FallbackPolicy.RECOMPUTE` 的存在≠所有网络异常及时回退；且重算抬升 GPU 负载，**不保证只影响 TTFT**。验证计划见"故障注入"节。
- 昇腾 e2e、跨 TP/KV dtype/block size/硬件族的 KV 互用。

### 数据模型增量（实现契约）

**`ReplicaPlacementSpec`**（serde default 兼容——**仅证明旧数据可反序列化**；滚动升级行为在第 3 步实现时验证，不宣称"零破坏"）：

```rust
pub role: Option<ReplicaRole>,            // prefill | decode | mixed（None=mixed）
```

**`ModelDeployment`**：

```rust
pub topology: Option<DeploymentTopology>, // None=replicated | pd_split
pub kv_sharing: Option<KvSharingSpec>,    // pd_split 必填
```

**`KvSharingSpec` v1 收敛**（评审 #6 修正：seed 域统一）：

```rust
pub remote_url: String,       // lm://host:port
pub serde: String,            // v1 固定 "naive"（其他值拒绝）
// 注意：无 hash_seed 字段。v1 全局单一种子：Router 的
// NEBULA_KV_HASH_SEED == 注入引擎的 PYTHONHASHSEED，来源必须是同一处
// 配置；per-deployment 种子 v1 拒绝（Router 哈希域是进程级，per-model
// hasher + 索引失效规则留 v2）。算法 sha256_cbor、block 16 固定。
```

**`EndpointInfo`**：`pub role: ReplicaRole`（serde default=mixed，注册时由执行面按 spec 写入；Router 从既有 `/endpoints/` 读，**热路径零新增 watch**）。

### 配置传播与生命周期（评审 #3 补齐）

```
Deployment(topology, kv_sharing, replica_specs[].role)
   → Scheduler planner：角色/kv_sharing 投影进 PlacementAssignment（唯一 CAS 写）
   → Node reconcile：按 assignment 生成实际启动配置（role→kv-transfer-config + LMCACHE yaml）
   → 就绪 → /endpoints/{model}/{replica}（含 role）
```

- **变更规则**：`remote_url`/`serde`/`topology`/任一 `role` 变更 ⇒ 新 Placement 版本 + 受影响副本 Drain 重建（与今日镜像切换同径）；`desired_state` 不变。
- **扩缩容 v1 禁止自动**：pd_split 的 scale 仅允许显式指定完整 `replica_specs` 的手动变更；缩容校验必须保留 ≥1 prefill 与 ≥1 decode，否则 BFF 拒绝。角色感知自动调度（含首 assignment 模板问题）留 v2。

### 路由语义（Router v1，评审 #1/#5 修正）

```
warm  = 前缀索引命中（共享存储已持有该前缀，任意副本的 Stored 即证明可检索）
        或 session_affinity 锚定在 decode 副本
stage = warm → decode 池（便宜地 retrieve）
        否则 → prefill 池（先把 KV 写进共享存储）
候选 = endpoints[model] 按 stage 过滤 role；空池 → 回落全体（stage_fallback_total 计数）
池内选择 v1 = least_pending
```

**实现修正（2026-10-12，rig 验证时发现）**：初版实现只看 session affinity，导致冷流量进 prefill 后被锚定、**decode 池永不可达**。正确信号是前缀索引命中——它回答的是"共享存储里有没有这个前缀"（store-level 事实），而不是"哪个 decode 副本本地持有"（review #5 的问题，仍需多 owner 索引，v1.1）。索引与共享 KV 共用同一 block-hash 键空间，故可直接复用（P1c 与 pd_split 的协同点）。

- session affinity 的锚定域限制在 decode 池内（首次命中 prefill 不锚定 prefill）。
- **P1c 在 v1 不承担 decode 池内本地命中优先**：现索引每 hash 单 owner，prefill 的 Stored 会覆盖 decode 的 owner，角色过滤后必回落（评审 #5 指出）。多 owner 索引（`(model,hash) → {replica: ts}`，查询限定候选集；区分 GPU 本地持有 vs 远端已写入两个信号）为 **v1.1 项**，届时 `prefix_affinity` 才在 decode 池内恢复本地优先语义；replicated 拓扑下 P1c 行为不变。
- 不引入 vLLM disagg proxy；分发协调由 nebula 承担（v1 即本节规则）。

### 校验规则（共享控制逻辑，BFF/CLI 共用；不只覆盖 BFF——评审 #4）

1. `pd_split` ⇒ `kv_sharing` 合法、engine_type=vllm、**部署镜像 digest 必须是过门禁的产物**（见下）。
2. 每个 replica_spec 非 mixed 角色；≥1 prefill、≥1 decode。
3. **保守 KV 兼容范围**：v1 每副本 TP=1、同 kv_cache_dtype、同 block size、同硬件族；范围外组合拒绝（互用性未实证）。SGLang+pd_split 拒绝。
4. `replicated` + 显式角色 ⇒ 拒绝。
5. seed 一致性：Router 进程 env 与引擎注入同源；不一致启动告警并使 P1c 降级可见。

### 部署产物绑定（评审 #4 修正）

门禁在容器内 `pip install` 的环境 ≠ 生产镜像。**部署契约**：pd_split 部署引用的镜像必须是"过门禁的固化产物"——v1 流程 = `check_lmcache_compat.sh` 对 **baked digest** 跑 full 通过 → 该 digest 记入兼容矩阵（engines.md）→ Deployment 校验只认矩阵内 digest。full 门禁断言收紧为 **正数传输**（`Retrieved [1-9][0-9]* out of`），非仅文本出现。

### lmcache_server 与故障注入计划（评审 #2 补齐）

- v1 运维拉起（nebula-stack.sh 增容器，同 etcd 模式；无状态缓存重启可丢）；probe_stack.sh 增健康检查。
- **故障注入验证（第 3 步实现后、宣称回退语义前必须完成）**：①启动前不可达 ②运行中断线 ③网络黑洞 ④传输中故障；记录各场景的引擎行为、超时上界、错误响应与恢复路径。在此之前，失败语义表述为"**预期回退，待故障注入验证**"。

### 失败语义（修正后）

| 故障 | 预期行为 | 状态 |
|---|---|---|
| decode 池全灭 | 回落 prefill 池（完整请求能力） | 设计；实现+计数器后验证 |
| `lmcache_server` 挂/断线/黑洞 | 引擎 recompute（可能抬升 GPU 负载、影响吞吐，非仅 TTFT） | **待故障注入验证** |
| 远端 KV miss | decode 副本自 prefill | 已实证 |

### 触点清单（第 3 步实现，评审后仍估 2-3 会话）

1. common：结构体增量 + **共享校验函数**（含兼容范围与 digest 白名单）+ 单测。
2. scheduler：planner 投影 role/kv_sharing 进 PlacementAssignment；变更→新版本+Drain。
3. controller + node：注入映射（复用 kv-events 注入点）；EndpointInfo.role 注册。
4. Router：stage 判定（affinity 限 decode 域）+ 角色过滤 + `stage_fallback_total`；**不做** decode 池内 P1c 本地优先（v1.1）。
5. BFF/UI：校验接入共享逻辑；deployment 编辑暴露 topology/role（admin-ui-change 流程）。
6. scripts：nebula-stack/probe 增 lmcache_server；**故障注入脚本**（上节四场景）。

## 分阶段落地（有需求再开）

| 步 | 内容 | 验收 |
|----|------|------|
| P2a | `HardwarePool` CRUD + Deployment `allowed_pools` + Scheduler 硬过滤 | 两池混部；未授权池放不进去 |
| P2b | 池/节点不可调度 + Drain 规范 | 杀一池成员不拖垮他池副本 |
| P2c | `multi_node_tp` / `pd_split`（pd_split 设计见上节，2026-10-12） | 不支持拓扑被拒绝；支持则 Placement 可解释 |

N3 D4/D5（跨节点 TP / EngineShim）挂在 P2c，无客户需求不提前实现。

## 成功标准（产品）

多池可部署；不支持拓扑被显式拒绝；故障不跨池蔓延；Router 热路径零改或仅读既有 endpoints。
