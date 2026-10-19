# K8s 运行时方案

> 2026-07-15。边界硬规则见仓库根 [`AGENTS.md`](../../AGENTS.md)；etcd 边界见 [`etcd.md`](./etcd.md)；排期入口见 [`../arch/roadmap.md`](../arch/roadmap.md)。

## 边界（不变）

- **etcd**：Nebula 声明与协调权威（Deployment / Placement 期望、`/endpoints/` 等）。不换成 kube-apiserver 当元数据主存。
- **K8s**：仅 **k8s** 部署形态下的引擎**执行面**（用 Pod 持有 GPU、起停引擎）。
- **Gateway / Router**：继续认 etcd `/endpoints/`，不改为依赖 K8s Service 发现做选路。拟议的外部控制面适配器（llm-d / Dynamo EndpointSlices 等）只允许在可选适配层投影为同一 Endpoint 形状，禁止核心热路径直读 kube 对象（草案 [`../arch/control-plane-adapters.md`](../arch/control-plane-adapters.md)）。
- **一种形态一个 owner**：`process`/`docker` 仍由 Node；`k8s` 由集群内 **K8s controller/Operator** 独占，禁止 Node 与 Operator 对同一副本双重 reconcile。
- **GPU**：`k8s` 形态由 Pod 独占整张卡（当前实现：`nodeName` + `NVIDIA_VISIBLE_DEVICES=<gpu_index>`，见 `create_engine_pod`）。不做卡级切分 —— 切卡会让共享同一张卡的副本争抢 HBM 带宽，与 pd_split 的 prefill/decode 隔离目标相反（见 [`heterogeneous-compute.md`](./heterogeneous-compute.md) §1.2）。
- **生成清单未被 kube-scheduler 参与**：`nodeName` 硬绑跳过了调度器，因此节点级 PDB/拓扑约束目前不生效；这属于未实现项，不是待接的第三方能力。

## 目标形态

```text
BFF → etcd /deployments/
        ↓
Scheduler（k8s 形态：只定副本期望 / Drain，不做宿主机绑卡）
        ↓
etcd 期望（Placement 或等价声明）
        ↓
nebula-k8s-controller（唯一执行 owner）
        ↓ apply Pod（NVIDIA_VISIBLE_DEVICES=整卡）
Pod Ready → 写 /endpoints/{model}/{replica}（lease）
        ↓
Router → 引擎 HTTP
```

现有裸机路径不变：Scheduler 绑 `node_id`+GPU → Node 起 Child/Docker → 写 `/endpoints/`。

## 分阶段

| 阶段 | 内容 | 验收 |
|------|------|------|
| **K0** | 契约：`runtime=process\|docker\|k8s`；`k8s` 下 Placement 不承载宿主机 GPU 绑定；文档与 AGENTS 边界 | 契约单测 + 文档 |
| **K1** | 轻量 in-cluster controller：watch 期望 → Deployment/Pod（整卡独占）→ Ready 回写 `/endpoints/`；缩容/Drain = 摘 endpoint + 删 Pod | 单集群 vLLM 一副本通 Gateway（当前仅 `docker`/`process` 形态经过实测，`k8s` controller 见下方「与现状关系」） |
| **K2** | 与 Scheduler 对齐：`k8s` 扩缩只改期望副本；失败退避、重建对账；不引入 Node 本地启停 | 扩缩/自愈不双写 |
| **K3** | 正式 Operator/CRD（可选）：Nebula CR ↔ etcd 或 CR 为集群内投影；多集群/多租户命名空间 | 有客户再开 |

## 不做

- 用 K8s etcd/对象替换 Nebula etcd 控制面。
- 让现有 Scheduler/Node 直接散落调用 kube API 起 Pod（PoC 除外，不进主路径）。
- Router 热路径改走 kube EndpointSlice / Ingress 选路。
- 在 etcd 存 Pod YAML 全文或高基数集群事件。

## 与现状关系

- **已存在**：`crates/nebula-k8s-controller` 通过 `kubectl` 把 `/deployments/{model}` 落成 Pod（整卡独占、`nodeName` 硬绑、Ready 后回写 `/endpoints/`、按模型/副本隔离错误、`config_fingerprint` 变更走 delete-then-create）。裸机的 `process` / `docker` 形态由 `nebula-node` 负责，两种形态互斥且各自单一 owner。
- **未实现**（本文档 K0–K3 里仍是设计）：`runtime=process|docker|k8s` 的显式契约字段、`k8s` 形态的 Scheduler 对齐（K2）、Operator/CRD（K3），以及"由 kube-scheduler 参与放置"（当前 `nodeName` 硬绑跳过了调度器）。
- 本方案是**并行运行时**，按需开启；不阻塞裸机交付。Agent 改代码时须遵守 [`AGENTS.md`](../../AGENTS.md)「K8s 边界」。
