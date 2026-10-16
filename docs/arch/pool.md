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

**实现修正（2026-10-12，rig 验证时发现）**：初版实现只看 session affinity，导致冷流量进 prefill 后被锚定、**decode 池永不可达**。正确信号是前缀索引命中——它回答的是"共享存储里有没有这个前缀"（store-level 事实），而不是"哪个 decode 副本本地持有"（review #5 的问题，仍需多 owner 索引，v1.1）。

**⚠️ rig 阻断（2026-10-12，必须解决的 v1 前置）**：启用 `LMCacheConnectorV1` 后，引擎日志明确输出 **`LMCache INFO: KV events are disabled.`**（`cache_engine.py:188`）——**LMCache 接管 KV 路径后停用了 vLLM 的 kv-events 发布**。实测：569-token prompt 只产生 1 条索引条目、`prefix_hit_total=0`，两条请求都落 prefill（`prefix_lookup_total=2` 证明哈希/查表已在跑）。因此**P1c 索引在 pd_split 形态下不能充当 warm 信号**。

**✅ 解法 1 已落地并真机验证（2026-10-13）**：warm 信号改用 **LMCache controller** 的 `POST /lookup`（store-level 权威事实）。

```
请求 → Router 对 prompt / chat messages 分词 → POST <controller>:9000/lookup {"tokens":[...]}
     → {"layout_info":{"<instance>":["LocalCPUBackend", 569]}}   # 命中 token 数
     hit > 0 → warm → decode 池；否则 cold → prefill 池
     响应按 (model, sha256(token_ids)) 缓存 2s；任何错误/200ms 超时 → 降级为 cold（可见计数）
```

- 引擎侧注册：`lmcache_controller_env`（四项必填：`ENABLE_CONTROLLER` / `CONTROLLER_PULL_URL`(8300) / `CONTROLLER_REPLY_URL`(8400) / `WORKER_PORTS` + `INSTANCE_ID`）——缺 reply URL 会 crash-loop（rig 教训）。
- 脚本：`lmcache_controller --host 0.0.0.0 --port 9000 --monitor-port 9001`（**必须带 `PYTHONHASHSEED`**，与引擎同值）。
- 新计数：`nebula_router_pd_lookup_total` / `pd_lookup_warm_total`。

**真机验证（2026-10-13，8×5090）**：
| 检查 | 结果 |
|---|---|
| controller 命中 | `{"qwen25-7b-1":["LocalCPUBackend",512],"qwen25-7b-0":["LocalCPUBackend",569]}` |
| Router 计数 | `pd_lookup_total=2, pd_lookup_warm_total=2` |
| 路由分布 | **prefill=1 / decode=2**（冷→prefill，暖→decode） |
| KV 取回 | decode 副本 `Retrieved 512/512 tokens (0.0273 GB)` + `hit tokens: 512` |



**已实证的部分（不受此阻断影响）**：角色注入（prefill `kv_producer`+`SAVE_UNFULL_CHUNK` / decode `kv_both`+`RETRIEVE_LOCATIONS`，同 seed=42）、scheduler 投影、EndpointInfo.role 注册、共享存储写入与取回（prefill 副本二次请求 **Retrieved 569/569 tokens**）、stage 过滤逻辑与计数器（单测全绿）。

- session affinity 的锚定域限制在 decode 池内（首次命中 prefill 不锚定 prefill）。
- **两级局部性（v1.1 多 owner 索引，MapReduce Locality 分级）**：`PrefixIndex` 存储 `(model_uid, block_hash) → {replica_id: (last_seen_ms, generation)}` 多持有者映射（带 per-replica 墓碑与代际水位），并提供限定候选集的 `lookup_in(model, hashes, allowed_replicas)`。路由判暖先看全局（Store-local 共享 KV 命中），阶段过滤缩小到 Decode 池后再做池内 `lookup_in`（Node-local 显存命中）：若某 Decode 副本本地 GPU 显存已持有该前缀且未过载（`pending <= min_pool_pending`），优先直连该 Decode 副本（省去 TCP `lm://` Shuffle，计数 `nebula_router_pd_decode_local_hit_total`），否则再退化为池内 `least_pending` 从 `LMCacheServer` 远端拉取。
- **冷启动单 Mapper 瓶颈溢出保护（Prefill Spillover）**：当冷请求进入 Prefill 池且所有 Prefill 副本均处于忙碌（`min_prefill_pending >= NEBULA_PD_PREFILL_SPILLOVER_PENDING`，默认 `1`），而 Decode 池中存在负载更低的空闲副本（Decode 以 `kv_both` 运行，具备完整自 prefill + 写回共享存储能力）时，自动将冷请求溢出到空闲 Decode 副本（计数 `nebula_router_stage_prefill_spillover_total`），避免宽前缀冷启动并发被单 Prefill 副本串行卡死。
- 不引入 vLLM disagg proxy；分发协调由 nebula 承担（v1/v1.1 即本节规则）。

### 校验规则（共享控制逻辑，BFF/CLI 共用；不只覆盖 BFF——评审 #4）

1. `pd_split` ⇒ `kv_sharing` 合法、engine_type=vllm、**部署镜像 digest 必须是过门禁的产物**（见下）。
2. 每个 replica_spec 非 mixed 角色；≥1 prefill、≥1 decode。
3. **保守 KV 兼容范围**：v1 每副本 TP=1、同 kv_cache_dtype、同 block size、同硬件族；范围外组合拒绝（互用性未实证）。SGLang+pd_split 拒绝。
4. `replicated` + 显式角色 ⇒ 拒绝。
5. seed 一致性：Router 进程 env 与引擎注入同源；不一致启动告警并使 P1c 降级可见。

### 部署产物绑定（评审 #4 修正）

门禁在容器内 `pip install` 的环境 ≠ 生产镜像。**部署契约**：pd_split 部署引用的镜像必须是"过门禁的固化产物"——v1 流程 = `check_lmcache_compat.sh` 对 **baked digest** 跑 full 通过 → 该 digest 记入兼容矩阵（engines.md）→ Deployment 校验只认矩阵内 digest。full 门禁断言收紧为 **正数传输**（`Retrieved [1-9][0-9]* out of`），非仅文本出现。

### lmcache_server 与故障注入（✅ 2026-10-13 实测，评审 #2 关闭）

- v1 运维拉起：`scripts/nebula-stack.sh` 增 `lmcache_server` + `lmcache_controller`（均带共享 `PYTHONHASHSEED`；未设 `LMCACHE_IMAGE` 时静默跳过，replicated 部署不受影响）；`probe_stack.sh` 增 controller `/lookup` 与 server 存在性检查。
- **故障注入实测（8×5090，pd_split 1+1）**：

| 场景 | 结果 |
|---|---|
| 存储运行中被杀 | **0 拒绝**；吞吐 26.0 → 13.2 rps（−49%），TTFT p50 428 → 1084ms（+153%）；恢复后回升 |
| 黑洞（`iptables DROP` 6800，无 RST） | **0 拒绝**，12.7 rps、p50 1132ms——**无挂起**（LMCache 客户端超时生效） |
| 长请求传输中被杀 | 请求仍 **200 完成**（50.2s，回退重算） |
| **存储启动前不可达** | 引擎可启动但**伴随 1–3 次重启**、就绪显著变慢（300s 时仅 1/2 endpoint）；起来后 0 拒绝（重算，15.98 rps）；存储恢复后 22.01 rps |
| **controller 启动前不可达** | **引擎 crash-loop**（注册强校验失败）——部署顺序约束：controller 必须先于引擎可用 |

结论：**存储/传输故障优雅降级（只损性能不损可用性）；启动期依赖（controller、store）不可达会有重启/慢就绪，controller 缺失直接不可用**。前者的回退语义已由实测支撑，后者转化为部署前置条件与告警项。

### 失败语义（修正后）

| 故障 | 预期行为 | 状态 |
|---|---|---|
| decode 池全灭 | 回落 prefill 池（完整请求能力） | 设计；实现+计数器后验证 |
| `lmcache_server` 挂/断线/黑洞 | 引擎 recompute（可能抬升 GPU 负载、影响吞吐，非仅 TTFT） | **待故障注入验证** |
| 远端 KV miss | decode 副本自 prefill | 已实证 |

### 收益 A/B（✅ 2026-07-13，客户端口径，评估建议 3）

同机同模型，2 副本；客户端经 Gateway 全栈测量：

| 配置 | 有效吞吐 | TTFT p50 | TTFT p99 | e2e p99 |
|---|---|---|---|---|
| replicated（暖态，8 前缀） | **28.66 rps** | **401.6ms** | 720.9ms | 893.2ms |
| pd_split（暖态，8 前缀） | 27.33 rps | 437.1ms | **572.1ms（−21%）** | **736.3ms（−18%）** |
| replicated（96 前缀，冷启动主导） | **3.72 rps** | **644ms** | 47.7s | 48.1s |
| pd_split（96 前缀，冷启动主导） | 3.15 rps | 1878ms | 54.0s | 55.1s |

**结论**：暖态下 pd_split 吞吐持平（−5%）、**尾部延迟更优（p99 −21%）**——共享存储让 KV 只存一份、decode 侧取回替代重复 prefill；但**宽前缀/冷启动场景更差**（1:1 池配比把全部冷 prefill 压到单个 prefill 副本，且 TCP 取回慢于本地命中）。v1 定位为"角色化路由 + 共享 KV 的尾部优化"，**不是吞吐方案**；池配比（prefill:decode）是后续调优旋钮。

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
