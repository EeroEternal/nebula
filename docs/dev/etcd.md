# etcd 放什么 / 不放什么

> 控制面元数据边界（2026-07-13）。Keyspace 清单见 [`../arch/architecture.md`](../arch/architecture.md)；`/stats/` 契约见 [`stats.md`](./stats.md)。实现抽象：`nebula-meta::MetaStore`（etcd + memory）。

etcd 是 **声明式协调层的唯一权威**，不是通用数据库。本地缓存可丢可重建；对账是「etcd vs runtime」清孤儿，不是双权威同步。

## 准入准则

写入 etcd 须同时满足：

1. **多组件共享**：至少一个写者、至少一个其他组件靠 list/watch/CAS 消费。
2. **需要协调语义**：前缀 watch、lease 自动过期、revision CAS、或选主/fencing 之一。
3. **体量小、最新值即可**：单 key 存当前真相；不存历史序列、大 blob、高频时序。

缺任一条件 → 默认 **不进 etcd**（Postgres / Prometheus / xtrace / Loki / 进程本地）。

## 应进 etcd

| 类别 | Key 前缀 | 理由 |
|------|----------|------|
| 声明期望 | `/models/…/spec`、`/deployments/` | BFF 唯一写入口；Scheduler reconcile 源 |
| 调度结果 | `/placements/` | CAS + `plan_version`；Node/Router 对齐期望拓扑 |
| 运行时注册 | `/endpoints/`、`/cells/` | lease；Router 选路；Node 死则自动摘除 |
| 节点心跳 | `/nodes/…/status` | lease；调度候选与平台身份 |
| 实时控制面 stats | `/stats/` | lease；Router/Scheduler **决策用最新值**（非历史） |
| 运行时能力快照 | `/capabilities/` | 与副本同生命周期；控制台/选型读 |
| 集群治理声明 | `/compat/`、`/slos/`、`/canaries/` | 影响调度或发布路径的小配置 |
| 选主 | `/nebula/election/…` | lease + CAS fencing（Scheduler） |
| 租户准入快照 | `/tenants/` | Gateway 可选读配额；体量小、需集群一致 |

约束：placement 全路径 CAS；watch 用快照 revision，compact/重连后全量校正；`/stats/` 只含控制字段（见 [`stats.md`](./stats.md)）。

## 当前在 etcd、宜视为「借住」

下列 key **今天在 etcd**，因实现省事且体量尚小；**不依赖 watch/lease 做热路径协调**。扩容或要审计/历史时，优先迁出而非扩 etcd 职责。

| Key 前缀 | 现状 | 更合适的家 |
|----------|------|------------|
| `/pricing/` | 成本单价配置 | Postgres（跟租户/账单同域） |
| `/usage/{tenant}/{window}` | 用量窗口聚合 | Postgres 或专用时序/计费库；窗口一多易膨胀 |
| `/benchmarks/runs\|profiles/…` | 跑批与画像 | Postgres；大结果外置对象存储 |
| `/model_requests/` | 遗留只读 | 迁移后删除，禁止新写 |

迁出原则：控制面热路径（Gateway/Router/Scheduler/Node）**不得**因此改读 Postgres；BFF 可双读过渡。

## 不应进 etcd

| 数据 | 应落何处 | 原因 |
|------|----------|------|
| 控制台账号 / SSO / 会话 | Postgres | 用户态；Scheduler/Node 禁止碰 |
| Trace、LLM 语义 span | xtrace（OTLP） | 历史与高基数 |
| 时序指标、告警曲线 | Prometheus | 历史；etcd 只留控制面最新 `/stats/` |
| 应用日志 | stdout → Loki 等 | 不可作协调真相 |
| 引擎权重、模型文件、镜像层 | 本地盘 / 镜像仓库 | 大对象 |
| Cell 内部 worker 拓扑 | 外部 Cell 自管 | Nebula 只登记整入口 `/cells/` |
| 请求审计全文、token 明文 | 审计后端 / 密钥管理 | 安全与体积 |
| Prometheus 高基数标签（如 `tenant_id`） | 禁止进 metrics；用量归因走 `/usage/` 或迁库后的计费面 | 基数爆炸 |

观测三平面与 etcd 的关系：历史观测 **永不** 进 etcd；Node 可向 xtrace 推指标，同时把 **决策所需最新字段** 写入 `/stats/`。

## 新增 key 检查清单

加前缀前自问：

- 谁写、谁 watch/CAS？若只有 BFF 读写 → 倾向 Postgres。
- 进程崩溃后是否必须靠 lease 消失？否 → 多半不是 etcd。
- 是否需要保留历史或多个版本？是 → 不是 etcd。
- 单集群预计 key 数与 value 大小是否可控（MB 级元数据，而非 GB）？否 → 外置。
- 是否已有同语义前缀可扩展，而不是再开平行权威？

通过后再改 `architecture.md` Keyspace 表，并保证只有一个写入口 owner（见 [`ownership.md`](./ownership.md)）。
