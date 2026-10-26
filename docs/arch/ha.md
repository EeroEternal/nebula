# Nebula HA-first 重构 Roadmap

状态：目标设计与实施顺序，未代表相关能力已经交付。本文约束后续架构、Issues 和生产验收；现状以 [`architecture.md`](./architecture.md) 与对应代码为准。

## 1. 决策

HA 是 Nebula 的初始架构约束，不是性能完成后的增强功能。

- 开发环境允许单进程运行，但代码、契约和测试不得假设单 Gateway、单 Router 或单状态 Owner。
- 生产参考拓扑从首版验收起就使用多 Gateway、多 Router、多 Engine Replica 和高可用控制面。
- 本地状态只能是可重建缓存；任何影响请求正确性、配额、会话续接或发布一致性的状态必须有共享权威和明确所有权。
- 不通过 per-request KV handoff 或 Decode 中途迁移实现“透明 HA”。流式请求失败后必须明确失败或从已提交状态恢复。
- HA 不改变 Zone C/S2 边界：不进入引擎内部，不让 KV bytes 经过 Nebula。

## 2. 已有基础与真实缺口

### 可复用基础

- Scheduler 选主和 fencing、Gateway/Router 多副本及既有 HA 真机报告，见 [`../dev/ha/report.md`](../dev/ha/report.md)。
- etcd 是声明式协调权威，endpoint/stats/capability 使用 lease；BFF 业务与历史数据在 PostgreSQL，见 [`../dev/etcd.md`](../dev/etcd.md)。
- 前缀索引是**多 Owner 索引**：`(model_uid, block_hash) → {replica_id: (last_seen_ms, epoch, generation)}`，带 per-replica 墓碑与 `(epoch, generation)` 水位防乱序覆盖。设计见 [`../dev/direction.md`](../dev/direction.md) §11.2-2 与 [`pool.md`](./pool.md)「两级局部性」，代码见 [`kv_events.rs`](../../crates/nebula-router/src/kv_events.rs) 的 `OwnerMap`。
- 首字节前重试、流式错误帧、abort 与上游错误区分已有契约基础，见 [`../dev/streaming-cache.md`](../dev/streaming-cache.md)。
- Tool-Pause、会话骨骼 pin、租户配额和 LMCache 生命周期已有实现基础，但不等于多实例 HA 已成立。

### 必须优先修复的缺口

| 领域 | 当前缺口 | HA 风险 |
|---|---|---|
| 租户配额 | 主要是进程级，多个 Gateway 可形成 N× 上限 | 集群超卖、故障后容量雪崩 |
| Tool-Pause | 租约、一次性消费、remote/embedded 和多实例一致性未闭合 | 续接丢失、重复 pin、错误锚定 |
| Tool 提交 | tool-call delta 后失败/取消也可能留下状态 | 错误租约和无效资源占用 |
| 配置/Recipe | Placement、Recipe、Prompt/KV ABI 未形成统一版本快照 | 同请求跨副本语义不一致、错误 KV 复用 |
| Canary | 权重主要是发布字段，缺少真实一致分流 | 多接入实例发布行为不一致 |
| 协议语义 | Responses/Messages 的 usage、finish 和异常 EOF 存在缺口 | 失败被误报为完成、计费和恢复错误 |
| 接入层 | xLB/其他 LB 的 retry、超时、错误和 Nebula 语义可能叠加 | 非预期 POST 重放、嵌套预算、错误格式漂移 |

## 3. 目标拓扑

```text
                       ┌── xLB A / 既有 LB A ──┐
Client → VIP/Cloud LB  │                       ├→ Gateway A / Gateway B
                       └── xLB B / 既有 LB B ──┘
                                                       │
                                               shared versioned snapshots
                                                       │
                                               Router A / Router B
                                                       │
                                               selected Engine Replicas

Control plane: 3-node etcd + PostgreSQL HA + multi-owner prefix index
```

约束：

- xLB 是可替换的接入负载均衡组件，不是集群权威。已有 LB 已满足要求时不强制替换。
- xLB 只选择健康的 Nebula 接入实例，不得放在 Router 与 Engine 之间重新选择模型副本。
- xLB 自身也必须多副本，前端 VIP/Cloud LB/其他故障切换机制不得形成新单点。
- 所有 Gateway 同属一个控制面、同一 ModelRecipe/Prompt/KV ABI 和兼容的租户配置。
- 多节点、多副本仍属于单集群 HA；多集群 Fleet 是另一层，不建立全局双权威。

## 4. 状态与所有权重构

| 状态 | 目标权威 | 热路径方式 | 关键约束 |
|---|---|---|---|
| Deployment / Placement / Canary | etcd | watch 后不可变快照 | 一个请求只使用一个完整 plan_version |
| ModelRecipe / Prompt/KV ABI | 与 Placement 同事务的 resolved snapshot | digest 和只读快照 | 禁止新旧 Recipe 与 Placement 混用 |
| Endpoint / Capability | Node 写 etcd lease，Router 消费快照 | `/endpoints/` 只读快照 | 缺失 digest 是 Unknown，不视为同 ABI |
| Tenant 配置 | BFF 写，Gateway 快照 | 禁止逐请求 etcd/PG | 热路径读取失败不能静默放行 |
| Tenant 配额 | 集群一致配额视图 | 首版确定性配额分片，后续按证据优化 | 分片总和不超过全局限制，禁止 N× |
| Tool-Pause lease | 共享租约记录，单写 Owner + CAS | 仅 Tool-Pause 提交/续接/过期等低频转移 | TTL、epoch、fencing、一次性消费 |
| Session/pin | 共享租约与幂等键驱动引擎控制 API | 后台执行，响应不被 pin 阻塞 | 重复提交不重复 pin，失败只降级性能 |
| Prefix index | 现有多 Owner indexd | 可重建 hint-only 视图 | 不承载会话租约，不作为响应正确性唯一依据 |
| xLB backend 列表 | 部署系统生成并校验 | 独立接入配置 | 不与 Nebula Placement 双写或互相猜测 |
| 用量/审计/历史 | PostgreSQL、审计和指标系统 | 异步汇总 | 不进入逐 token 协调路径 |

### 4.1 配额策略

首版采用确定性配额分片，避免每请求访问协调器：

1. 所有 Gateway 从同一成员版本获得配额分片。
2. 同一租户的全局硬上限按活动 Gateway 分片，分片总和不得超过上限。
3. 成员变化时按新版本重新分片，并处理旧版本 permit 的过期和释放。
4. 流式请求的 permit 覆盖整个生命周期，abort、错误和正常完成都准确释放。
5. 预留故障接管余量；未使用分片是否动态再分配由后续真实负载决定，不在首版引入逐请求分布式锁。
6. 对无法保证硬上限的降级状态必须显式标记，按资源选择 fail-open 或 fail-closed，并记录审计。

**已落地（2026-10-26，#68 部分）**：第 1/2/4 条以**静态成员数**实现——每个 Gateway 强制执行 `limit / N`（`NEBULA_GATEWAY_QUOTA_MEMBERS`，默认 1 = 不分片），整数余数丢弃，所以任意时刻 Σ 分片 ≤ 全局上限，准入路径零 etcd/PG I/O；租户并发 guard 现已覆盖整个流式响应。未落地：第 3 条（成员变化自动重分片，需要 `/gateways/` lease 注册表和成员版本）与第 5 条（故障后未使用分片的动态再分配）。因此扩缩 Gateway **必须**同步改该变量，`limit < N` 的租户被拒而不是超卖。详见 [`../dev/queue-admission.md`](../dev/queue-admission.md) §7.1；实测（真 etcd + 2 个真实 Gateway 进程，`members=1` 放行 20 / `members=2` 放行 10）见 [`../dev/ha/quota-drill.md`](../dev/ha/quota-drill.md)，复现脚本 `scripts/ha_quota_drill.sh`。

### 4.2 Tool-Pause 租约

租约至少包含：

```text
tenant_id / session_id / model_uid
original_replica_id / lease_epoch / fencing_token
state / expires_at / idempotency_key
```

状态语义：

```text
ToolCallDelta（不可提交）
  → ToolPauseCommitted（完整且 finish_reason=tool_calls）
  → ResumeGranted（一次性）
  → Consumed / Expired / Released / Failed
```

要求：

- tool-call delta、异常 EOF、客户端取消不得提交租约。
- 任意 Gateway 可验证有效租约并锚定原副本。
- 原副本死亡时不进行 KV handoff，按部署政策明确重算或失败。
- 旧 epoch 的实例恢复后不能继续占有或消费租约。
- 重复提交和并发续接不得造成重复 pin、重复工具执行或重复计费。

## 5. 请求与故障契约

### 新请求

健康副本应能继续接收。接入 LB 的健康检查只判断接入能力，不把业务模型可用性误判为 Gateway 进程健康。

### 首字节前

只允许在统一尝试次数和首字节总预算内重试。xLB、transport、Gateway、Router 不得各自维护一套隐藏 retry；所有上游 attempt 必须可观测、可审计。

### 已开始流式输出

不跨 Engine 透明迁移或拼接输出。允许明确返回流式错误，由客户端或 Agent 从最近已提交状态恢复。异常 EOF 不得生成正常 DONE、completed 或 message_stop。

### 配置发布

Deployment、Recipe、Capability 和 Canary 以完整快照版本发布。请求进入时固定一个版本，响应全过程不切换。回退同时回退关联配置，不只回退某个进程二进制。

### 错误与取消

Nebula 的稳定错误 envelope、Retry-After、abort 和上游 5xx 语义必须跨 Gateway 一致。xLB 自身生成的空 503/504 不得绕过统一错误契约。

## 6. 分阶段实施

阶段编号 **HA-P0–HA-P4** 是本文私有的实施顺序，与 [`roadmap.md`](./roadmap.md)「下一阶段顺序」及 [`vision.md`](./vision.md) §8 的 Phase 0–3（L0/L1/L3/L2/L4 交付轴）**无关、不互相对齐**：那边 Phase 0 已交付，这里 HA-P0 全部是未来工作。

### HA-P0：HA 基线与生产验收拓扑

交付：

- 固定最小生产拓扑：2×接入 LB、2×Gateway、2×Router、3×etcd、PostgreSQL HA、2+ Engine Replica。
- 建立配置、attempt、客户端错误、上游错误和会话事件的统一关联字段。
- 建立 HA 故障注入矩阵和客户端证据格式，复用 [#62 Benchmark v2](https://github.com/EeroEternal/nebula/issues/62)。
- 将 HA 验收加入 `scripts/ci.sh` 的可选多实例套件；普通单元测试不强制启动完整集群。

验收：

- 拓扑中不存在单 Gateway、单 Router 或单接入 LB 假设。
- 能区分客户端 TTFT、上传、排队、上游 TTFT、pool wait 和重试次数。
- 所有当前缺口都有失败用例，不能把“进程启动成功”当作 HA 通过。

### HA-P1：配置快照、协议和失败语义

交付：

- ResolvedModelRecipe、Prompt/KV ABI 和 Placement/Endpoint 投影，对应 [#75](https://github.com/EeroEternal/nebula/issues/75)、[#73](https://github.com/EeroEternal/nebula/issues/73)。
- RoutePreparation 与同 ABI retry，对应 [#76](https://github.com/EeroEternal/nebula/issues/76)。
- Responses/Messages 的会话、真实 usage、finish 和异常 EOF 修复，对应 [#77](https://github.com/EeroEternal/nebula/issues/77)。
- 统一 NormalizedInferenceEvent 和 ToolPauseCommitted。
- Gateway 后台快照替代热路径 etcd/tenant 读取，读取失败按明确政策拒绝或降级，不静默放行。

验收：

- 两个 Gateway 对同一请求产生一致协议、usage 和错误行为。
- 不同 Prompt/KV ABI 不共享缓存，不成为同请求 fallback 候选。
- 任意 SSE 切分、length、异常 EOF、取消均不会生成正常完成。
- 热路径无逐请求 etcd/PG I/O。

### HA-P2：共享配额、会话与 Tool-Pause

交付：

- 确定性集群配额分片及成员版本、permit 生命周期和降级政策，对应 [#68](https://github.com/EeroEternal/nebula/issues/68)。（静态成员数分片已落地，见 §4.1；自动成员版本与再分配未完成）
- 共享 Tool-Pause 租约、TTL/epoch/fencing、一次性消费和幂等 pin，对应 [#64](https://github.com/EeroEternal/nebula/issues/64)。
- Session Governance 的配额、过期、释放、审计与低基数指标，对应 [#65](https://github.com/EeroEternal/nebula/issues/65)。
- 租户 KV 哈希域隔离，对应 [#67](https://github.com/EeroEternal/nebula/issues/67)。

验收：

- 两个以上 Gateway 不再形成 N× 配额，热租户不饿死其他租户。
- 两个 Gateway 竞争续接时最多一个成功；旧 epoch 不能消费。
- 原副本存活时跨 Gateway 续接仍正确锚定；原副本死亡按政策重算或明确失败。
- 取消、失败和异常 EOF 不留下租约或错误 pin。
- 协调故障按资源执行明确的 fail-open/fail-closed，不出现无审计放行。

### HA-P3：多活接入、xLB 与真实 Canary

交付：

- xLB/既有 LB 多副本接入，明确 backend 配置生成、健康检查和故障摘除。
- 接入 retry=受控、body cap 对齐、超时对齐、完整响应头和 Nebula 错误 envelope。
- stable/candidate 并存的真实按权重 Canary，对应 [#63](https://github.com/EeroEternal/nebula/issues/63)。
- Foundry Artifact 作为黑盒 Provider 时使用相同 HA 契约，对应 [#69](https://github.com/EeroEternal/nebula/issues/69)。

验收：

- kill 一个接入 LB/Gateway 后新请求继续，健康副本没有非预期容量雪崩。
- 不发生 xLB 与 Nebula 双重重试、重复计费或重复工具执行。
- 多 Gateway 观察一致的 Canary 权重，SLO 违约可恢复 stable，Drain 不截流。
- 长 Prompt 的额外 buffer、上传时间和 RSS 有测量且未突破部署预算。

### HA-P4：故障注入、长稳与生产准出

必须覆盖：

- kill Gateway/Router/xLB/Engine/Prefix Owner 各至少一次。
- 一个 etcd 节点失联及多数派恢复。
- prefill、流式输出、Tool-Pause 提交和续接期间故障。
- 配置发布中部分实例短暂不同步。
- 配额协调故障、旧 permit/lease 恢复和重复消费竞争。
- 长 Prompt 上传中断、客户端取消与上游故障同时发生。
- 24 小时多实例 soak，包含普通 Chat、Responses/Messages、Agent 和多租户混合流量。

准出标准：

- 新请求在部署定义的恢复目标内恢复；目标值进入该部署 SLO，不虚构统一数值。
- 无跨租户状态、无重复提交、无错误 DONE、无非预期 POST 重放。
- 所有失败均可归因且符合统一错误契约。
- goodput@SLO、p99/p999、失败率、配额和恢复指标保存原始证据。
- 回退流程经过真实执行，文档只记录已验证能力。

## 7. 与现有 Issues 的顺序

```text
#62 证据协议
  → #77 协议与失败语义
  → #75/#73 配置与 ABI
  → #76 同 ABI retry
  → #68 集群配额 + #64/#65 会话与租约
  → #63 真实 Canary + #69 Provider HA
  → 多实例故障注入与生产准出
```

[#70 OIDC](https://github.com/EeroEternal/nebula/issues/70) 与 [#71 Fleet](https://github.com/EeroEternal/nebula/issues/71) 不阻塞单集群 HA-first 主线；Fleet 不得被用作绕过单集群状态重构的理由。

图中未出现的三个 issue 及其原因：

- [#72 Recipe 兼容层](https://github.com/EeroEternal/nebula/issues/72)：HA-P1 的 Provider 侧落地，依赖 [#75](https://github.com/EeroEternal/nebula/issues/75)/[#73](https://github.com/EeroEternal/nebula/issues/73) 先定义 ABI，因此在图中被 #75/#73 覆盖。
- [#74 结构化输出能力拆分](https://github.com/EeroEternal/nebula/issues/74)：只约束 Provider 能力声明与部署期检查，不涉及多实例状态，可独立于阶段推进。
- [#66 replicated Mixed 接通 L1/L2 Offload](https://github.com/EeroEternal/nebula/issues/66)：serving 配置缺口而非 HA 语义缺口，任何阶段都可以单独做，不阻塞准出。

## 8. 非目标

- 不以副本数量代替共享状态和故障语义。
- 不在请求热路径引入逐请求 etcd/PostgreSQL 写入。
- 不做 per-request KV handoff、Decode 中途迁移或跨 Recipe 透明重放。
- 不让 xLB 选择最终 Engine Replica 或成为另一个集群权威。
- 不把多集群 Fleet 当作单集群 HA 的前置。
- 不把文档、健康检查或副本启动成功写成 HA 验收通过。
