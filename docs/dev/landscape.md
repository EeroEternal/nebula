# 推理编排框架对照与 Nebula 的缺口

> 来源：2026-09-20 抓取 GitHub 现状，2026-10-17 增补主干进展。对照对象：**Dynamo** `v1.4.2`（[ai-dynamo/dynamo](https://github.com/ai-dynamo/dynamo)，NVIDIA，8.1k★，**Rust 为主 + Python 扩展**）；**llm-d** `v0.9.0`（[llm-d/llm-d](https://github.com/llm-d/llm-d)，CNCF sandbox，Red Hat/Google/IBM/CoreWeave/NVIDIA）；**AIBrix**（[vllm-project/aibrix](https://github.com/vllm-project/aibrix)，ByteDance）；**Mooncake / sgl-router**（[kvcache-ai/Mooncake](https://github.com/kvcache-ai/Mooncake) / [sgl-project/sglang](https://github.com/sgl-project/sglang)）。
> 定位：外部分布式 Serving 框架趋同演化现状 → Nebula 的已对齐能力、剩余缺口与差异化。控制面适配契约见 [`../arch/control-plane-adapters.md`](../arch/control-plane-adapters.md)；分区定义见 [`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §10；Zone A/B/C 与 Stage S1/S2/S3 边界及趋势映射见 [`direction.md` §10–§11](direction.md#10-边界抽象与核心工程哲学用约束替代强耦合)。

## 1. 主流分布式 LLM Serving 编排框架五方横向对比

各系统均处于单机引擎（`vLLM` / `SGLang` / `TRT-LLM`）之上的 **Zone C（分布式 Serving 与编排层）**，并在过去一年出现了显著的**工程趋同演化（Convergent Evolution）**：

| 维度 | **llm-d** ([llm-d/llm-d](https://github.com/llm-d/llm-d)) | **NVIDIA Dynamo** ([ai-dynamo/dynamo](https://github.com/ai-dynamo/dynamo)) | **AIBrix** ([vllm-project/aibrix](https://github.com/vllm-project/aibrix)) | **Mooncake / sgl-router** ([kvcache-ai/Mooncake](https://github.com/kvcache-ai/Mooncake)) | **Nebula（当前主干）** |
|---|---|---|---|---|---|
| **核心设计哲学** | K8s 云原生标准底座 + vLLM 深度协同 | 全栈极致性能 + NVIDIA 硬件通信原语 | 高密度云原生 + 大规模多 LoRA / 混部 | 以 KV Cache 为中心的解耦存储与极速路由 | **无需 K8s、单二进制起栈 + 用声明式约束替代强耦合 + 企业级多租户治理** |
| **网关与路由规范** | K8s **GAIE** (`InferencePool` / `InferenceModel`) + Envoy `ext_proc` → EPP | Dynamo Smart/KV Router（支持 Envoy 集成与独立部署） | Envoy `ext_proc` 插件 + 语义/前缀路由 | Rust `sgl-router`（影子 Radix 树 + 联合代价） | Rust **Gateway + 嵌入式/独立 Router**（`PrefixAffinity` + 角色路由，热路径 ~0.1ms） |
| **全局 KV 状态同步** | 块哈希 + 事件流 → EPP 全局索引 | ZMQ KV 事件流 → 分布式 Radix 树（`kv-router` / `kv-hashing`） | 节点指标 + 前缀/语义路由 | 预测型影子前缀树 + 节点队列/KV 水位联合打分 | ✅ **16-token `sha256_cbor`（[`kv_hash.rs`](../../crates/nebula-router/src/kv_hash.rs)）+ ZMQ `BlockStored` + [`nebula-indexd`](../../crates/nebula-router/src/bin/indexd.rs) 快照扇出** |
| **跨节点 KV 传输/存储** | 集成 **NIXL** + `vLLM KVConnector` | 原生 **NIXL**（UCX/RDMA/NVLink/GDS）+ `kvbm-*` | 集成 InfiniStore / Vineyard / LMCache | **Mooncake Transfer Engine**（RDMA 拓扑优化） | ✅ **硬件中立 `LMCacheConnectorV1`（[LMCache/LMCache](https://github.com/LMCache/LMCache)）+ `lmcache_server`（`lm://` 纯 TCP）**，兼容 CUDA 与昇腾 |
| **PD 分离与弹性调度** | Variant Autoscaler + 双阶段直通 + **短请求旁路** | **Dynamo Planner**（按 ISL/OSL 实时重平衡 P:D）+ 短请求旁路 | 基于滑动窗口 SLO 的混部弹性伸缩器 | 动态 Prefill / Decode 实例池与防过载预测 | ✅ **`pd_split` v1 共享 KV 角色化路由已真机验收**（[`pool.md`](../arch/pool.md)，TTFT p99 −21%）；短请求旁路与二元曲面建议代码与单测已合入（⏳ 待真机 A/B 验收，见 [`direction.md` §11](direction.md#11-2026-llm-serving-技术趋势参照与-nebula-落点映射)） |
| **多租户与企业治理** | 依赖外部 K8s 网关生态 | 非核心（偏底层算力基础设施） | 侧重多 LoRA 密度与 K8s 混部 | 侧重底层 KV 传输与路由 | ✅ **内建租户配额、有界公平队列、审计、成本归因、兼容矩阵、L3 选型与 Canary** |

## 2. Dynamo 组件 vs Nebula 现状映射

| Dynamo crate | Nebula 对应组件 | 当前状态 |
|---|---|---|
| `kv-router` | `nebula-router` + [`nebula-indexd`](../../crates/nebula-router/src/bin/indexd.rs) | ✅ 已落地（P1c + Batch 2 全局索引扇出） |
| `kv-hashing` | [`nebula_router::kv_hash`](../../crates/nebula-router/src/kv_hash.rs) + [`prompt_hash`](../../crates/nebula-router/src/prompt_hash.rs) | ✅ 已落地（`sha256_cbor` + `chat_template` token 级对齐） |
| `kvbm-*`（KV Block Manager） | `lmcache_server` + `lmcache_controller`（由 [`nebula-stack.sh`](../../scripts/nebula-stack.sh) 编排，Router [`controller_warm`](../../crates/nebula-router/src/lib.rs#L707) 查询） | ✅ 已落地 `pd_split` v1（借力外部 S3 组件，KV 字节旁路 Nebula） |
| `router-plugins` | [`RoutingStrategy`](../../crates/nebula-router/src/strategy.rs) trait | ✅ 代码与单测已合入（⏳ 待真机 A/B 验收）：多 owner 联合代价打分（[`direction.md` §11.2](direction.md#2-多-owner-前缀索引--节省-prefill-vs-排队等待联合代价打分joint-cost-function)） |
| `runtime` / `sidecar` | `nebula-scheduler` / `nebula-node` / `nebula-k8s-controller` | ✅ 已落地（含 `ReplicaRole` 注入与配置指纹重建） |
| `rl` / `gpu_memory_service` / `memory` | （无） | ⏸ 非当前范围 |

## 3. 能力缺口演进（从 2026-09 初评到 2026-10 现状）

1. **KV 全局索引 + 跨副本前缀亲和**：✅ **已闭环**（P1c + `nebula-indexd` + 客户端口径 A/B 验证：冷启动拒绝率 12%→0%、goodput 2.4×，暖态 goodput +23%，见 [`benefit-verification.md`](benefit-verification.md)）。
2. **基于共享 KV 的 PD 角色化编排（`pd_split` v1）**：✅ **已闭环**（LMCache TCP 共享存储 + controller `/lookup` + 四场景故障注入 0 拒绝 + 暖态 TTFT p99 −21%，见 [`../arch/pool.md`](../arch/pool.md)）。
3. **Workload-Aware 路由与控制面演进（Batch 5.1–5.7，详见 [`direction.md` §11.2–§11.3](direction.md#112-zone-c--s2-数据面与路由层4-项高价值可吸收设计-代码与单测已合入-待真机-ab-验收batch-5154)）**：✅ **代码与单测已合入（⏳ 待真机 A/B 验收）**
   - **`pd_split` 短请求智能旁路（Short-Prompt Bypass）**：`< 256 tokens` 短冷请求在 Decode 池有新鲜 `/stats/` 且未饱和时跳过 `/lookup` 直走 Decode 池原地计算；
   - **多 Owner 前缀索引 + 联合代价打分（`SavedPrefill - QueueWait`）**：同时覆盖 `pd_split` Decode 池内本地命中优先（评审 #5）与热点前缀单点拥塞溢流（`owner_congested`）；
   - **Coding Agent `Tool-Pause` 感知会话保护与 `/v1/score` 透传**：服务端观测 `finish_reason="tool_calls"` 后登记 `(tenant, session)` 租约并优先唤醒续接请求，同时开放 SGLang `/v1/score` 能力门控与前缀哈希；
   - **L3 并发感知量化选型 + L2 投机采样参数化 + L4 二元曲面（TTFT vs TPOT/KV）池配比建议（`scale_prefill_pool` / `scale_decode_pool`）**。
4. **仍保持暂缓（⏸）的外部能力**：闭环 SLO 全自动扩缩（现只输出结构化建议）、OpenAI Batch / async API、RL 权重同步、GAIE `InferencePool` 直接互操作。

## 4. 最值得注意：连 llm-d 都在考虑「非 K8s 模式」，而高前缀复用下“亲和优于盲目拆 PD”

1. **非 K8s 模式需求真实**：llm-d 的 proposal 列表里有 **`non-kubernetes-mode.md`**，反向验证了 Nebula 的 etcd-native / 不依赖 K8s 定位。
2. **高前缀复用负载优先吃透 Prefix Affinity**：在多轮 Agent 高前缀复用场景下，先把 **Prefix Affinity + 投机采样参数化 + 轻量角色化路由**做透，比过早陷入复杂跨机单请求流式交接更具工程性价比。

## 5. Nebula 保留的核心差异化

1. **etcd-native 控制面 + 单二进制起栈**（不强制依赖 K8s）。
2. **引擎与硬件无关**（OpenAI passthrough + 纯 TCP LMCache 共享 KV，不绑 NVIDIA 专属 NIXL / `p2p_nccl`）。
3. **用声明式约束替代强耦合**（统一 `NEBULA_KV_HASH_SEED` 哈希不变量 + 镜像 Digest 门禁白名单 + 故障随时可重算的 Hint 语义）。
4. **内建企业治理闭环**（多租户准入、有界公平队列、SLO 诊断、L3 智能选型、Canary 与成本归因）。
