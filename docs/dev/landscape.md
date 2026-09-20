# 推理编排框架对照与 Nebula 的缺口

> 来源：2026-09-20 抓取 GitHub 现状。**Dynamo** `v1.4.2`（[ai-dynamo/dynamo](https://github.com/ai-dynamo/dynamo)，NVIDIA，8.1k★，**Rust 为主 + Python 扩展**）；**llm-d** `v0.9.0`（[llm-d/llm-d](https://github.com/llm-d/llm-d)，CNCF sandbox，Red Hat/Google/IBM/CoreWeave/NVIDIA）。
> 定位：本文只做「外部编排框架现状 → Nebula 的能力缺口 / 差异化」。控制面适配契约本身见 [`../arch/control-plane-adapters.md`](../arch/control-plane-adapters.md)；分区定义见 [`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §10。

## 1. 两家都在「引擎之上」

| | Dynamo | llm-d |
|---|---|---|
| 自我定位 | 引擎**之上**的编排层，“把 SGLang / TensorRT-LLM / vLLM 变成协调的多节点系统” | 引擎**之上**的编排与优化 |
| 形态 | **Rust 为主，Python 扩展** | K8s（Helm/Shell），后端 vLLM / SGLang |
| 核心能力 | 分离式服务、智能路由、**多级 KV 缓存**、自动扩缩 | 前缀/负载感知路由、**分层 KV + 全局索引**、PD + 宽专家并行、**SLO 自动扩缩**、Batch API |

→ 两者都落在 `vllm-serving-perf.md` §10 定义的 **Zone C**。Nebula 的「引擎之上编排层」不再是独有叙事。

## 2. Dynamo 组件 vs Nebula

Dynamo 的 `lib/` Rust crate 与 Nebula 高度对应，但多出关键几块：

| Dynamo crate | Nebula 对应 |
|---|---|
| `kv-router` | `nebula-router` |
| `kv-hashing` | **（无）** |
| `kvbm-common` / `kvbm-config` / `kvbm-engine` / `kvbm-kernels` / `kvbm-logical` / `kvbm-physical` / `kvbm-consolidator` | **（无）** |
| `router-plugins` | `RoutingStrategy` trait |
| `runtime` / `sidecar` | `nebula-scheduler` / `nebula-node` |
| `rl` / `gpu_memory_service` / `memory` | （无） |

## 3. 能力缺口（启发）

1. **KV 全局索引 + 分层缓存（最大缺口）**。Dynamo 有整套 KV Block Manager（GPU/CPU/disk 分层、logical/physical 分离）；llm-d 有 “tiered offloading + precise global indexing of the KV cache state”。Nebula 只有 per-endpoint `EndpointStats`，**没有全局 KV 索引、没有分层 offload 编排**。这是多轮对话 / 长 system prompt 的头号能力。
2. **信号已就绪**：vLLM 已能发 **KV events**（`EventPublisher.register_publisher`），消费方缺位——正是 Nebula 的机会。
3. **选路升级为 Filter→Score→Pick + 插件**。llm-d 的 EPP 是多阶段管线，Dynamo 有 `router-plugins`；Nebula 的 `RoutingStrategy::select()` 是单阶段。
4. **闭环 SLO 自动扩缩**。llm-d `llm-d-planner.md` / `autoscaler.md`、Dynamo Planner；Nebula 的 SLO 只到「建议 / 草稿」。
5. **Batch / async API 缺口**。llm-d `batch-gateway.md` / `llm-d-async.md`（OpenAI Batch API + 异步，提升利用率）；Nebula 无。
6. **RL / 权重同步缺口**。llm-d `rl-time-slicing-platform.md`、Dynamo `lib/rl`、vLLM RL weight sync；Nebula 无。
7. **互操作：GAIE / InferencePool**。llm-d 用 Gateway API Inference Extension（InferencePool / EPP）标准；Nebula 用自有 etcd 模型。建议归一化 Endpoint 对齐 InferencePool，或提供 EPP 兼容接口。
8. **KV 传输（NIXL）不自造**。Dynamo 的 NIXL、llm-d 亦用。Nebula「不搬 KV」的边界正确，但应**集成其信号/接口**而非无视。

## 4. 最值得注意：连 llm-d 都在考虑「非 K8s 模式」

llm-d 的 proposal 列表里有 **`non-kubernetes-mode.md`**。

- **反向验证**了 Nebula 的 etcd-native / 不依赖 K8s 定位：这不是没人要的角落，而是连 K8s 阵营都意识到的需求。
- 但也意味着**该差异化窗口会被上游补上**，Nebula 需要更快。

## 5. Nebula 保留的差异化

1. **etcd-native 控制面**（不依赖 K8s）。
2. **引擎无关 + OpenAI passthrough**（Dynamo 也宣称，但重心在 NVIDIA 栈）。
3. **轻量**（单二进制 vs K8s 全家桶）。
4. **边界清晰**（不搬 KV），但用集成拿到 KV 感知信号。

## 6. 行动排序（均在 Zone C）

| 优先 | 动作 | 启发 |
|------|------|------|
| 高 | **KV 全局索引 + 分层 offload 编排** | 1 |
| 高 | 吃 **vLLM KV events** 做跨副本亲和 | 2 |
| 中 | 选路升级 **Filter→Score→Pick + 插件** | 3 |
| 中 | **SLO 闭环自动扩缩** | 4 |
| 中 | **Batch / async API** | 5 |
| 低 | **GAIE / InferencePool 互操作** | 7 |
