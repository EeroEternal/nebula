# BFF 能力对齐清单

## 1. 目标与边界

- BFF 独立服务，对外提供 UI/CLI 的统一管理 API。
- BFF 不依赖 gateway，也不要求 gateway 为适配而改动。
- BFF 直接连接 etcd + router + node（必要时补管理接口）。

## 2. 现有系统能力盘点

### 2.1 etcd 关键数据

| 数据 | Key 前缀 | 用途 |
| --- | --- | --- |
| 节点状态 | `/nodes/{node_id}/status` | 节点心跳、资源、健康 |
| 端点信息 | `/endpoints/{model_uid}/{replica_id}` | endpoint 列表与状态 |
| 放置计划 | `/placements/{model_uid}` | 模型分配与版本 |
| 模型请求 | `/model_requests/{request_id}` | 加载/卸载请求流 |
| 模型规格 | `/models/{model_uid}/spec` | 模型静态元数据（如存在） |

### 2.2 网元对外接口

| 组件 | 已有接口 | 备注 |
| --- | --- | --- |
| router | `/healthz`, `/metrics`, `/v1/*` | 主要是推理路由与指标 |
| node | 无 HTTP 接口 | 只通过 etcd watch/reconcile |
| scheduler | 无 HTTP 接口 | 只 watch `/model_requests` |
| gateway | `/v1/admin/*`, `/metrics` | BFF 不依赖 | 

## 3. BFF 首期能力对齐

### 3.1 聚合视图（只读）

| 能力 | 数据来源 | 当前可行性 | 需要补齐 |
| --- | --- | --- | --- |
| 集群总览 | etcd: `/nodes`, `/endpoints`, `/placements`, `/model_requests` | 可直接实现 | 无 |
| 模型列表 | etcd: `/placements` 或 `/models/*/spec` | 可直接实现 | 若需要模型元信息，补充 `/models/*/spec` 写入来源 |
| 请求队列 | etcd: `/model_requests` | 可直接实现 | 无 |
| 组件健康 | router `/healthz` + etcd 节点心跳 | 可直接实现 | 若需要 node 进程健康，需补接口或依赖心跳 |

### 3.2 模型管理（读写）

| 能力 | 数据来源/动作 | 当前可行性 | 需要补齐 |
| --- | --- | --- | --- |
| 加载模型 | 在 etcd 写入 `/model_requests` (Pending) | 可直接实现 | 无 |
| 卸载模型 | 在 etcd 写入 `/model_requests` (Unloading) | 可直接实现 | 无 |
| 强制停止某节点模型 | 依赖 node 管理接口 | 需要补齐 | 给 node 增加管理 API 或外部运维通道 |

说明：scheduler 已监听 `/model_requests` 并写入 `/placements`，BFF 只需写请求即可驱动流程。

### 3.3 观测能力

| 能力 | 数据来源 | 当前可行性 | 需要补齐 |
| --- | --- | --- | --- |
| 路由指标 | router `/metrics` | 可直接实现 | 无 |
| 节点/调度指标 | 目前无 HTTP 指标 | 不可直接实现 | 为 node/scheduler 增加指标端点，或接 Prometheus exporter |
| 日志 | 现无统一入口 | 不可直接实现 | 接入集中日志系统（Loki/ELK）或加日志采集代理 |

## 4. 对齐结论

- **BFF 可完全绕开 gateway**，直接以 etcd 为权威状态来源，并通过 router 读取运行指标。
- **模型加载/卸载流程可直接由 BFF 驱动**，通过写入 `/model_requests` 与 scheduler 协同。
- **观测类能力存在缺口**：node/scheduler 缺少指标与日志入口，需要补齐或接入外部观测系统。
- **运维控制类能力存在缺口**：如强制回收/重启模型，需要 node 管理接口或外部运维工具。

## 5. 后续补齐建议（不改 gateway）

1. 为 node 与 scheduler 增加最小 `/healthz` 与 `/metrics` HTTP 端口（可选独立端口）。
2. 引入日志采集与查询系统（例如 Prometheus + Loki），BFF 只做查询聚合。
3. 若需要强制操作，增加 node 的管理 API（如 `POST /admin/models/{uid}/stop`）。
