# 指导意见：Nebula 如何吸收 PowerLLM 工程完备性

> 状态：架构指导（2026-07-10）
> 读者：Nebula 维护者 / 控制面负责人
> 前提：保持 Nebula 的 no-xoscar 方向（Rust 控制面 + etcd reconcile + 引擎容器化）
> 对照仓库：`/Users/xinference/github/powerllm`、本仓库
> 相关文档：`../arch/architecture.md`、`ha/ha_roadmap.md`、`plan.md`、`../arch/optimization.md`；PowerLLM 侧 `docs/supervisor_api_actor_architecture_analysis.md`、`docs/ha_redesign_and_testing.md`、`docs/core_supervisor_worker_descent_design.md`

---

## 0. 结论先行

Nebula 的架构方向比 PowerLLM 更干净：编排不在推理热路径上，引擎不嵌入控制面，状态外置到 etcd。PowerLLM 的优势不在 Actor 本身，而在**生产工程完备性**——HA 闭环、生命周期细节、企业能力、测试纪律、运维脚本。

指导原则一句话：

**吸收 PowerLLM 的可靠性模式、运维实践、测试纪律与企业 API 面；拒绝 xoscar / 上帝 Actor / 内嵌多模态引擎；用 etcd lease + Rust reconcile + Docker 镜像族实现等价能力。**

不要把「功能对齐 PowerLLM」理解成「把 PowerLLM 搬过来」。要对齐的是**行为与验收标准**，不是实现形态。

---

## 1. 双方定位与差距总览

| 维度 | PowerLLM | Nebula |
|------|----------|--------|
| 控制面 | Python + xoscar Actor（Supervisor/Worker/ModelActor） | Rust 微服务（Gateway/Router/Scheduler/Node） |
| 状态权威 | Actor 内存 + DB（需 rectify） | etcd（声明式 Placement/Endpoint） |
| 推理路径 | FastAPI → 多跳 RPC → ModelActor → 引擎 | Gateway → Router → 引擎 HTTP |
| 引擎耦合 | 库嵌入 / monkey-patch / subpool | Docker / 原生进程，零侵入 |
| 工程完备度 | 高（HA、企业、多模态、测试门禁） | 中低（MVP+，HA 仍规划） |
| 架构债 | 上帝 RPC 面、热路径编排、api↔core 双向依赖 | Gateway/BFF 边界重叠、CAS 未统一、CI 弱 |

最大缺口（按对生产影响排序）：

1. **HA**：PowerLLM 已有 DB lease + fencing + `/healthz` LB 门禁 + 测试技能；Nebula 仍是 `ha_roadmap.md` 规划稿。
2. **生命周期闭环**：async launch / progress、scale-in drain、request abort。
3. **测试与 CI**：PowerLLM 有 v2-core / v2-api gate；Nebula 几乎无 PR 必跑契约测试。
4. **企业能力**：license / 细粒度 RBAC / batch / tuning——按产品需要分期，不要为对齐而全抄。
5. **交付**：一键 local、Helm、HA compose；PowerLLM 更成熟。

Nebula 已有、应作为吸收底座的优势：镜像 registry、Engine trait、router 熔断/admission、xtrace、`nebula-observe`、autoscale reconcile 雏形、plan_version 过滤。

---

## 2. 能力矩阵（应吸收什么）

图例：强 / 弱 / 缺。

| 能力域 | PowerLLM | Nebula | 吸收优先级 |
|--------|----------|--------|------------|
| HA 选主 + fencing | 强 | 缺 | **P0** |
| `/healthz` LB 门禁 | 强 | 弱（无 leader 语义） | **P0** |
| Drain on scale-in | 强 | 弱（Draining 被硬切） | **P0** |
| Request abort | 强 | 缺 | **P0** |
| Placement CAS 统一 | 中（分片锁） | 弱（部分 put） | **P0** |
| CI 契约测试 | 强 | 缺 | **P0** |
| Async launch + progress | 强 | 弱 | **P1** |
| Metadata 对账 | 强 | 弱（靠 reconcile） | **P1** |
| Autoscale 生产化 | 强 | 弱 | **P1** |
| etcd 3 节点 + 接入层多副本 | 中-强 | 弱 | **P1** |
| 一键部署 / Helm | 中 | 弱 | **P1** |
| Observability SLO | 强 | 弱（有 contract 无落地） | **P1** |
| PD 分离 | 强 | 缺（schema 预留） | P2 |
| 多模态引擎 | 强 | 缺 | P2（容器/Virtual，不内嵌） |
| License | 强 | 缺 | P2（可选，落 BFF） |
| Batch / Tuning | 强 | 缺 | P2（可选） |
| 细粒度 RBAC / SSO | 强 | 弱（API key） | P2（按产品） |

---

## 3. 明确不要复制的内容

### 3.1 xoscar / Actor 运行时

不要引入 Actor pool、ActorRef、subpool、进程内 RPC 链。PowerLLM 自己的分析也承认：痛点是「编排放进热路径」，不是「没有 Actor 就活不了」。Nebula 已用 HTTP 短路径解决这个问题，不要倒退。

### 3.2 上帝 Orchestrator

不要把 Scheduler 做成「一个服务包打天下」。PowerLLM Supervisor 曾 ~5000 行 / 100+ RPC，拆 mixin 后拓扑与 lifecycle 仍难解耦。Nebula 保持 Scheduler 写期望、Node 对齐实际、Router 只读 endpoint。

### 3.3 内存权威状态 + 事后 rectify

不要复制 `StatusGuardActor` 式内存权威。etcd 是唯一权威；Node/Router 本地缓存可丢可重建。对账是「etcd vs runtime」清理孤儿，不是「两套权威源同步」。

### 3.4 旧 HA 概念

不要复制 HTTP 525 secondary、binlog/checkpoint、slave supervisor 元数据同步。直接采用 **etcd lease election + plan_version / epoch fencing**（对齐 PowerLLM 新 HA 的语义，不对齐其 Actor 实现）。

### 3.5 引擎内嵌与 Python venv/subpool

多模态、embedding、TTS 若要做，走 **独立容器 + VirtualEngine / NativeHttp**，不要把引擎当库链进 Rust。依赖隔离用镜像族（Pin/Rolling），不要复刻 PowerLLM virtualenv。

### 3.6 Core ↔ API 共享 ORM

PowerLLM 的 license/device 逻辑让 core import api.database，形成双向依赖。Nebula：Scheduler/Node **不碰 Postgres**；用户/会话/license 只在 BFF；策略快照若需要可下沉到 etcd。

### 3.7 Gateway 吞掉 Router

不要让 Gateway 同时做协议转换、鉴权、实例选择、管理 API。Gateway = 协议/鉴权/审计；Router = 选 endpoint + 代理；BFF = 控制台聚合。`../arch/optimization.md` 里的边界问题要先收敛再加功能。

---

## 4. 应吸收的模式（映射到 Nebula 组件）

### 4.1 HA：选主、fencing、LB 门禁

**从 PowerLLM 学什么**

- 单调 fencing token：旧 leader 的写操作必须被拒绝。
- `/healthz`：非主返回 503，LB 摘除。
- 失主后清空或冻结「可变控制面写路径」，避免双写。
- HA 必须有可重复的 in-process / 单机多实例测试（PowerLLM `testing-ha` skill）。

**在 Nebula 怎么落地**

| 组件 | 改动 |
|------|------|
| `nebula-meta` | 封装 etcd election / lease；暴露 `leader_epoch` |
| `nebula-scheduler` | 仅 leader 处理 model_request、写 placement；follower `/healthz`→503 |
| `nebula-common` | `PlacementPlan` 带 `leader_epoch` 或沿用并强化 `version` CAS |
| `nebula-node` | 拒绝 epoch/version 落后的 placement；旧 endpoint 不覆盖新 |
| `nebula-router` | 只认最新 `plan_version` 的 Ready endpoint（已有雏形，补测试） |
| Gateway/BFF | 无状态多副本；不参与选主，靠 LB |

**验收**

- 2 个 scheduler：恰好 1 leader；kill leader 后新 leader 在约定 RTO 内接管（建议目标 <10s）。
- 旧 leader 写 placement 被 CAS/epoch 拒绝（脑裂演练）。
- follower `/healthz` = 503；LB 摘除后流量只打 leader 写路径（读路径可按设计允许 follower 只读或全部经 leader）。

### 4.2 Drain 与 Scale-in

**从 PowerLLM 学什么**

Scale-in 不是立刻 kill：先停接新流量 → 等 in-flight 结束 → 再 terminate / 清元数据。

**在 Nebula 怎么落地**

当前问题：endpoint 标 `Draining` 后，Router 只路由 `Ready`，等于硬切，in-flight 可能被掐断。

正确语义：

1. Scheduler 决定缩容 → 目标 replica 标 `Draining`。
2. Router：**不选 Draining 接新请求**；已建立的上游连接继续跑完（或直到 deadline）。
3. Node：观察该 replica pending/in-flight = 0（或超时）→ stop 引擎 → 删 `/endpoints/...`。
4. Scheduler：确认 endpoint 消失后再从 Placement 去掉 assignment。

**验收**：drain 期间新请求不到该 replica；已有流式请求可完成；随后 endpoint 消失且无孤儿。

### 4.3 Request Abort

**从 PowerLLM 学什么**

客户端取消 / 显式 abort 要传到引擎侧，且取消请求**不计入 error budget**（见 PowerLLM SLO 口径）。

**在 Nebula 怎么落地**

- Gateway：client disconnect 或 abort API → 取消对 Router 的下游请求。
- Router：断开到引擎的 HTTP/SSE。
- Node/引擎：依赖引擎原生取消（vLLM 连接断开即停）；若未来有 gRPC Shim，再补显式 Cancel RPC。
- Metrics：abort 单独标签，不进 5xx 成功率分母（或按 SLO 文档定义）。

### 4.4 Async Launch + Progress

**从 PowerLLM 学什么**

`wait_ready=false`、ProgressTracker、前端可轮询进度；大模型启动不阻塞控制 API。

**在 Nebula 怎么落地**

- `ModelRequest` / Deployment 状态机：`Pending → PullingImage → DownloadingWeights → StartingEngine → Ready | Failed`。
- Node reconcile 各阶段写 progress 到 etcd（如 `/model_requests/{id}/progress`）。
- BFF/CLI/前端读 progress；现有 `DownloadProgress` 扩展为完整 LaunchProgress，不要只覆盖权重下载。

### 4.5 Metadata 对账

**从 PowerLLM 学什么**

运行久了必有孤儿副本 / 幽灵元数据；需要可触发的 rectify + 周期 maintenance。

**在 Nebula 怎么落地**

权威源已是 etcd，对账更简单：

- Placement 有、Endpoint 无 → 重试 reconcile 或标 Failed。
- Endpoint 有、Placement 无 → 删 endpoint / stop 引擎。
- Endpoint `plan_version` 落后 → 忽略并清理。
- 提供 `POST /admin/reconcile`（仅 admin）+ 周期 full reconcile（Node/Scheduler 已有 watch 断线全量校正思路，做成显式运维能力）。

### 4.6 测试纪律

**从 PowerLLM 学什么**

- 契约测试锁住 API/SSE/鉴权行为。
- HA 有可重复 harness，而不是「真机碰运气」。
- CI gate 挡回归；真机只做发布前验收。

**在 Nebula 怎么落地**

P0 必建：

- `cargo test --workspace` 进 PR CI。
- 最小契约集：Router `plan_version` 过滤、Scheduler CAS 冲突、Gateway SSE 事件序列、auth 拒绝场景。
- HA in-process：单测里起 2 scheduler + 测试用 etcd（或嵌入式 mock），覆盖选主与 fencing。

避免重蹈 PowerLLM：文档写了 HA，但 CI 不跑多节点——Nebula 应在 P0 就把 election 单测纳入 gate。

### 4.7 可观测性 SLO

有 metrics ≠ 可运维。吸收 PowerLLM `observability_slo.md` 的口径思维：

- 定义 SLI：成功率、TTFT P95、流式中断率。
- 明确：client abort / 主动 drain 不算错误。
- `nebula-observe` + 告警规则落地，而不是只写 contract。

### 4.8 企业能力（按需，落对层）

| 能力 | 是否必须 | 落点 |
|------|----------|------|
| 细粒度 RBAC / SSO | 产品决定 | BFF + Gateway 鉴权，不进 Scheduler |
| License / GPU 限额 | 产品决定 | BFF 中间件；限额策略可镜像到 etcd 供 Scheduler 只读 |
| Batch | 可选 | 独立 job 空间 `/jobs/` 或 BFF 队列，Router 复用 |
| Tuning | 可选 | 后期；用容器跑训练任务，Node 扩展 Engine 类型 |

原则：企业逻辑不污染 reconcile 热路径；Scheduler 只读策略快照。

### 4.9 多引擎 / 多模态

PowerLLM 的广度来自多年 Python 适配；Nebula 用 **Engine trait + 镜像矩阵** 换广度：

- LLM：继续 vLLM / SGLang 容器。
- Embedding / Rerank / 多模态：新镜像 + NativeHttp，或 VirtualEngine 代理外部服务。
- 硬件感知：`(硬件, 引擎, 模型) → 镜像`（`plan.md` §7）——这是 Nebula 相对 PowerLLM 的正确扩展轴。

---

## 5. 分阶段路线图

### P0（建议 4–6 周）：可靠性基线

目标：能敢上多副本控制面，且 CI 能挡住回退。

| ID | 工作 | 验收标准 |
|----|------|----------|
| P0.1 | Scheduler etcd leader election | 双实例单 leader；故障切换 <10s；follower healthz=503 |
| P0.2 | Placement fencing（epoch/version CAS） | 旧 leader 写入被拒；脑裂演练通过 |
| P0.3 | Drain 闭环 | Draining 不接新流量；in-flight 可完成；再删 endpoint |
| P0.4 | Request abort / disconnect 传播 | 取消后引擎停生成；metrics 口径正确 |
| P0.5 | 所有 placement 写入走 CAS | `../arch/optimization.md` 中「直接 put」路径清零 |
| P0.6 | CI gate | PR 必跑 workspace test + 上述契约 |
| P0.7 | Gateway / BFF / Router 边界收敛 | 每个 API 唯一 owner；文档更新 |

**P0 完成标志**：单机可演示「杀 scheduler leader + drain 缩容」且请求成功率符合约定；PR 无测试不能合。

### P1（建议 6–10 周）：生命周期与交付

| ID | 工作 | 验收标准 |
|----|------|----------|
| P1.1 | Async launch + LaunchProgress | UI/CLI 可见分阶段进度；API 不阻塞 |
| P1.2 | Autoscale 生产化 | 扩缩有 cooldown；scale-down 走 drain |
| P1.3 | Metadata reconcile API + 周期任务 | 孤儿 endpoint 可清理 |
| P1.4 | etcd 3 节点 + gateway/bff/router 多副本 | 按 `ha_roadmap.md` Phase 1–2 |
| P1.5 | SLO dashboard + 告警 | TTFT/成功率可查可告 |
| P1.6 | 一键部署（compose profile 或 Helm 初版） | 新环境约 30 分钟跑通 E2E |

**P1 完成标志**：多机小集群可日常使用；扩缩容与发布有 runbook。

### P2（建议 10–16 周）：深度能力（按产品取舍）

| ID | 工作 | 说明 |
|----|------|------|
| P2.1 | PD 分离 | Placement role + Router PD 策略；对齐 PowerLLM 行为，不引入 PDModelActor |
| P2.2 | 多模态容器 | VirtualEngine / 专用镜像；禁止嵌入 Rust |
| P2.3 | License | 仅 BFF/Gateway |
| P2.4 | Batch API | 可选 |
| P2.5 | 前端对齐核心运维流 | 监控、审计、告警、多集群上下文 |
| P2.6 | 硬件感知调度 | 昇腾/NPU 适配矩阵 |

---

## 6. 组件级改造清单（速查）

| 组件 | P0 | P1 | P2 |
|------|----|----|-----|
| `nebula-meta` | election/lease API | etcd 集群运维文档 | — |
| `nebula-scheduler` | leader-only 写、CAS、healthz | autoscale+cooldown、reconcile admin | PD placement |
| `nebula-node` | drain stop、epoch 校验、progress 上报 | 镜像/权重阶段细化 | 新 Engine 类型 |
| `nebula-router` | Draining 语义、abort 传播、契约测试 | 多副本无状态 | PD 路由 |
| `nebula-gateway` | abort、SLO 标签、边界收敛 | 多副本 | 企业中间件钩子 |
| `nebula-bff` | API ownership 收敛 | progress/reconcile UI API | RBAC/license/SSO |
| `nebula-cli` | drain/scale 与新语义一致 | `cluster up`、progress | 多集群 context |
| `frontend` | 少改，跟 BFF | 启动进度、HA 状态提示 | 监控/告警完善 |
| CI / docs | test workflow、更新 ha_roadmap | Helm/runbook | 发布验收清单 |

---

## 7. 从 PowerLLM 迁移「行为」时的对照表

便于实现时对照「用户可感知行为」，而不是对照类名。

| 用户/运维行为 | PowerLLM | Nebula 目标实现 |
|---------------|----------|-----------------|
| 主备切换后旧主不能写 | fencing token 拒 RPC | etcd CAS + leader_epoch 拒写 |
| LB 摘除备节点 | `/healthz` 503 | scheduler（及需选主组件）`/healthz` 503 |
| 缩容不打断进行中请求 | scale-in drain | Draining 语义修正 |
| 取消生成 | abort_request | Gateway cancel → 断上游 |
| 看启动进度 | ProgressTracker API | etcd progress + BFF |
| 修元数据不一致 | rectify_metadata | admin reconcile |
| 过载保护 | rate limit 等 | 保持并强化 Router 429/熔断 |
| 多引擎 | Python 适配全家桶 | 镜像矩阵 + Engine trait |

---

## 8. 反模式清单（吸收时自检）

1. 为了「功能全」把 Scheduler 做成新的上帝服务。
2. 为了「少跳」把引擎又嵌回控制面进程。
3. 为了「兼容 PowerLLM API」在 Gateway 复制 enterprise 单体。
4. 上 HA 却不把 election/fencing 测进 CI。
5. 开 autoscale scale-down 却未修 drain。
6. License/用户体系让 Scheduler 直连 Postgres。
7. 用 Python venv 替代已有容器隔离。
8. 同时维护旧 binlog HA 概念与新 lease HA。
9. 只有 metrics 没有 SLO 口径（abort/drain 算不算错）。
10. 文档写完 Phase，代码仍是单实例 compose 且无演练记录。

---

## 9. 建议的工作方式

1. **先 P0 再谈功能对齐**。没有 HA/drain/CI，多模态和微调只会放大故障面。
2. **每个吸收项写「行为验收」**，不要写「实现一个类似 Supervisor 的模块」。
3. **PowerLLM 当行为与测试用例库**，不当代码依赖。可移植的是场景（杀主、缩容、取消、孤儿元数据），不是 Python 类。
4. **保持短推理路径**：Gateway → Router → Engine HTTP。任何「先问 Scheduler 再推理」的设计都视为回归。
5. **与现有 Nebula 文档对齐**：本指导落地后，应回写 `ha/ha_roadmap.md`（从规划改为执行）、`plan.md`（插入 P0 可靠性里程碑）、`../arch/optimization.md`（关闭 CAS/边界项）。

---

## 10. 一句话收束

Nebula 不要变成「Rust 版 PowerLLM」，而要变成「具备 PowerLLM 同级生产完备度的声明式模型平面」。架构上继续领先；工程上按 P0→P1→P2 把 HA、生命周期闭环、测试纪律和企业能力补齐——这才是去 xoscar 路径真正跑赢的方式。
