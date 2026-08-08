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

## 分阶段落地（有需求再开）

| 步 | 内容 | 验收 |
|----|------|------|
| P2a | `HardwarePool` CRUD + Deployment `allowed_pools` + Scheduler 硬过滤 | 两池混部；未授权池放不进去 |
| P2b | 池/节点不可调度 + Drain 规范 | 杀一池成员不拖垮他池副本 |
| P2c | `multi_node_tp` / `pd_split`（按客户引擎） | 不支持拓扑被拒绝；支持则 Placement 可解释 |

N3 D4/D5（跨节点 TP / EngineShim）挂在 P2c，无客户需求不提前实现。

## 成功标准（产品）

多池可部署；不支持拓扑被显式拒绝；故障不跨池蔓延；Router 热路径零改或仅读既有 endpoints。
