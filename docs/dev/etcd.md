# etcd 放什么 / 不放什么

> 控制面元数据边界（2026-07-13）。Keyspace 总表见 [`../arch/architecture.md`](../arch/architecture.md)；`/stats/` 字段见 [`stats.md`](./stats.md)。抽象：`nebula-meta::MetaStore`。Agent 硬规则见仓库根 [`AGENTS.md`](../../AGENTS.md)。

etcd 是 **声明式协调层的唯一权威**，不是通用数据库。本地缓存可丢可重建；对账是「etcd vs runtime」清孤儿。K8s/HAMi 仅作引擎执行面时的边界见 [`k8s.md`](./k8s.md) 与 [`AGENTS.md`](../../AGENTS.md)。

## 准入准则

写入须同时满足：多组件共享（list/watch/CAS）、需要协调语义（watch / lease / CAS / 选主）、体量小且只要最新值。缺一 → 默认不进（Postgres / Prometheus / xtrace / Loki / 本地盘）。

**生命周期：** 节点附属、随进程消失的 key **必须**挂 Node 共享 lease（或短 TTL）；声明类配置无 lease。

## A. 应进（协调真相）

| Key 前缀 | 写者 | 消费者 | 生命周期 |
|----------|------|--------|----------|
| `/models/…/spec`、`/deployments/` | BFF（主） | Scheduler | 持久；Deployment 唯一期望写入口 |
| `/placements/` | Scheduler（CAS） | Node、Router | 持久；`plan_version` |
| `/endpoints/`、`/stats/`、`/capabilities/` | Node | Router、Scheduler、BFF | **lease** |
| `/nodes/…/status` | Node | Scheduler、BFF | **lease** |
| `/images/` | BFF（主） | Node watch、Scheduler | 持久镜像注册 |
| `/image_status/` | Node | BFF | **lease** |
| `/compat/`、`/slos/`、`/canaries/` | BFF | Scheduler / 发布路径 | 持久治理 |
| `/tenants/` | BFF | Gateway（可选准入） | 持久小配置 |
| `/model_gc_requests/` | BFF | Node | 工作队列；**带 TTL**，Node 处理后删 |
| `/nebula/election/…` | Scheduler | Scheduler | lease + CAS fencing |

`/stats/` 只含实时控制字段，历史观测不进 etcd（见 [`stats.md`](./stats.md)）。

## B. 节点运维附属（应进，非热路径）

| Key 前缀 | 说明 | 生命周期 |
|----------|------|----------|
| `/model_cache/` | 节点本地模型缓存清单 | **lease**；Node 扫描刷新 |
| `/node_disk/` | 节点磁盘占用快照 | **lease** |
| `/download_progress/` | 下载进度 | 短 TTL（~30s） |
| `/alerts/{node}/disk_*` | 磁盘告警位 | **lease**；趋势告警走 Prometheus |

## C. 借住（今天在 etcd，优先迁出）

无热路径 watch；扩容或要历史/审计时迁 Postgres（或对象存储），**禁止**因此让 Scheduler/Node 读 PG。

| Key 前缀 | 更合适的家 |
|----------|------------|
| `/templates/` | Postgres（控制台资产） |
| `/pricing/` | Postgres（账单同域） |
| `/usage/{tenant}/{window}` | Postgres / 计费库（防窗口膨胀） |
| `/benchmarks/runs\|profiles/…` | Postgres；大结果外置 |
| `/model_requests/` | 遗留只读；迁移后删除，**禁止新写** |

## D. 不应进

控制台账号/SSO/会话 → Postgres。Trace → xtrace。时序/告警曲线 → Prometheus。日志 → Loki。模型权重/镜像层 → 本地盘/仓库。审计全文/token 明文 → 审计与密钥系统。高基数 `tenant_id` → 禁止进 Prometheus label。

## 新增 key

先过准入准则与 [`ownership.md`](./ownership.md) 唯一写入口，再改本文件与 `architecture.md` Keyspace。只有 BFF 自读写 → 默认 Postgres，勿开新 etcd 前缀。
