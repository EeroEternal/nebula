# K8s / HAMi 运行时方案

> 2026-07-15。边界硬规则见仓库根 [`AGENTS.md`](../../AGENTS.md)；etcd 边界见 [`etcd.md`](./etcd.md)；排期入口见 [`../arch/roadmap.md`](../arch/roadmap.md)。

## 边界（不变）

- **etcd**：Nebula 声明与协调权威（Deployment / Placement 期望、`/endpoints/` 等）。不换成 kube-apiserver 当元数据主存。
- **K8s / HAMi**：仅 **k8s** 部署形态下的引擎**执行面**（用 Pod 拿虚拟 GPU、起停引擎）。
- **Gateway / Router**：继续认 etcd `/endpoints/`，不改为依赖 K8s Service 发现做选路。
- **一种形态一个 owner**：`process`/`docker` 仍由 Node；`k8s` 由集群内 **K8s controller/Operator** 独占，禁止 Node 与 Operator 对同一副本双重 reconcile。
- **GPU**：`k8s` 形态不写 `gpu_indices` / `CUDA_VISIBLE_DEVICES`；由 Pod resource request（及 HAMi annotation）交给 kube-scheduler + HAMi。

## 目标形态

```text
BFF → etcd /deployments/
        ↓
Scheduler（k8s 形态：只定副本期望 / Drain，不做宿主机绑卡）
        ↓
etcd 期望（Placement 或等价声明）
        ↓
nebula-k8s-controller（唯一执行 owner）
        ↓ apply Pod/Workload（HAMi resources）
Pod Ready → 写 /endpoints/{model}/{replica}（lease）
        ↓
Router → 引擎 HTTP
```

现有裸机路径不变：Scheduler 绑 `node_id`+GPU → Node 起 Child/Docker → 写 `/endpoints/`。

## 分阶段

| 阶段 | 内容 | 验收 |
|------|------|------|
| **K0** | 契约：`runtime=process\|docker\|k8s`；`k8s` 下 Placement 不承载宿主机 GPU 绑定；文档与 AGENTS 边界 | 契约单测 + 文档 |
| **K1** | 轻量 in-cluster controller：watch 期望 → Deployment/Pod（含 HAMi 资源模板）→ Ready 回写 `/endpoints/`；缩容/Drain = 摘 endpoint + 删 Pod | 单集群 vLLM 一副本通 Gateway |
| **K2** | 与 Scheduler 对齐：`k8s` 扩缩只改期望副本；失败退避、重建对账；不引入 Node 本地启停 | 扩缩/自愈不双写 |
| **K3** | 正式 Operator/CRD（可选）：Nebula CR ↔ etcd 或 CR 为集群内投影；多集群/多租户命名空间 | 有客户再开 |

## 不做

- 用 K8s etcd/对象替换 Nebula etcd 控制面。
- 让现有 Scheduler/Node 直接散落调用 kube API 起 Pod（PoC 除外，不进主路径）。
- Router 热路径改走 kube EndpointSlice / Ingress 选路。
- 在 etcd 存 Pod YAML 全文或高基数集群事件。

## 与现状关系

当前仅 `process` / `docker`（`EngineProcess`）。本方案是**并列运行时**，按需开启；不阻塞裸机交付。Agent 改代码时须遵守 [`AGENTS.md`](../../AGENTS.md)「K8s / HAMi 边界」。
