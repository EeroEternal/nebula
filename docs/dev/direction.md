# Nebula 发展方向与编排边界备忘

> 日期：2026-10-16（初版 2026-09-20）。性质：**战略与架构边界备忘**（结论性判断，批次勾选以 [`../arch/roadmap.md`](../arch/roadmap.md) 为准）。
> 依据：[`landscape.md`](landscape.md)（Dynamo / llm-d 对照）、[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md)（真机测量 + A/B/C 分区）、[`prefix-affinity.md`](prefix-affinity.md)（P1c + `nebula-indexd`）、[`benefit-verification.md`](benefit-verification.md)（客户端口径 A/B）、[`../arch/pool.md`](../arch/pool.md)（`pd_split` v1 角色化路由 + 故障注入）。

## 1. 核心定位

> **Nebula = 不依赖 K8s 的多引擎推理编排 + 多租户治理控制面**（引擎之上、K8s 之外）。
> **核心演进命题：把多副本推理从“无状态的盲目均摊（Stateless Load Balancing）”，变成“围绕 KV Cache 中间状态的数据亲和与阶段化解耦调度（State- & Stage-Aware Orchestration）”。**

单实例推理引擎（vLLM/SGLang）有吞吐天花板（如 RTX 5090 单卡 ~3.4k tok/s），单纯加副本用 `LeastPending` 轮询会导致每个副本重复计算、重复存储相同的 Prefix KV（有效缓存容量除以 $N$）。Nebula 作为引擎之上的编排层，唯一能真正超越“单实例直连”的系统级手段，就是**管理好跨副本的 KV 状态与请求阶段**。

「引擎之上编排」这一层（[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §10 的 Zone C）已被 Dynamo（NVIDIA，Rust）与 llm-d（CNCF，K8s）占据，**正面拼广度必输**。Nebula 唯一未被占据的位置是**轻量、无需 K8s、引擎无关、带企业治理 + 基于约束解耦的 KV/Stage 感知编排**。

## 2. 三条主线

### 主线 1（守）：把「不依赖 K8s」做成产品级护城河

Nebula 是**单二进制 + etcd 权威 + 快速起栈**；Dynamo / llm-d 都要求 K8s。

- 落地：一键起栈、etcd HA / fencing、生产 etcd 切入、无 K8s 运维 runbook。
- 衡量：**起栈门槛**（依赖数、时间）、无 K8s 环境的可运维性。
- 对应现有 roadmap：Phase 0 收尾 + Phase 2/3 的运维面。

### 主线 2（攻）：补齐「引擎之上」的核心能力（State- & Stage-Aware）

| 能力 | 依据 / 状态 | 价值 |
|------|-------------|------|
| 吃 **vLLM KV events** | ✅ P1c 已落地（[`kv_events.rs`](../../crates/nebula-router/src/kv_events.rs)） | 消费 ZMQ `BlockStored` 实时构建块链索引 |
| **跨副本前缀亲和** | ✅ P1c 已落地（[`kv_hash.rs`](../../crates/nebula-router/src/kv_hash.rs) + `chat_template`） | **唯一能真超过单实例直连的系统级手段**（冷启动拒绝 12%→0%，暖态 goodput +23%） |
| **全局 KV 索引 `nebula-indexd`** | ✅ Batch 2 已落地（[`nebula-indexd.rs`](../../crates/nebula-router/src/bin/nebula-indexd.rs)） | 原始帧转发 + 周期快照扇出；Router 热路径 100% 查本地内存 |
| **`pd_split` 共享 KV 角色化路由** | ✅ Batch 3 v1 已落地（[`pool.md`](../arch/pool.md) §pd_split） | LMCache `lm://` 共享 KV + controller `/lookup`；暖态 TTFT p99 −21% |
| 选路升级 **Filter→Score→Pick + 插件** | llm-d EPP、Dynamo `router-plugins` | 单阶段 `select()` → 可插拔管线（后续演进） |

### 主线 3（拉大）：深化企业治理

Dynamo / llm-d 偏基础设施；**多租户 / SLO / 成本 / 审计 / canary** 是 Nebula 已建成（P4/P5/P6 Batch 1）、对手相对弱的企业平台能力。

- 补：**SLO 闭环自动扩缩**（现只到建议）、**成本与选型联动**、**Batch / async API**（对手都有）。

## 3. 优先级（结合真机数据）

| 优先 | 动作 | 依据 | 类型 |
|------|------|------|------|
| **P0** | **嵌入式 Router（单跳）+ 共享 KV 运行时接线** | 消除独立进程跳数，共用 `start_kv_runtime`（✅ 已完成） | 纯自研 |
| **P0** | **引擎级有界公平队列**（[`queue-admission.md`](queue-admission.md)） | 实测 C=256 时 P99 2.5s（✅ 已完成） | 准入控制 |
| **P1** | **KV events 消费 + 跨副本前缀亲和 + `nebula-indexd`** | 超过单实例直连的核心手段（✅ 已完成） | 主线 2 |
| **P1** | **`pd_split` v1：基于共享 KV 的角色化放置与路由** | 隔离冷 Prefill 长尾，TTFT p99 −21%（✅ 已完成） | 主线 2 |
| **P2** | 多 owner 索引（`pd_split` v1.1 Decode 池内本地命中优先） | [`pool.md`](../arch/pool.md) 评审 #5 | 主线 2 |
| **P2** | 选路 Filter→Score→Pick + 插件 | 对齐 EPP 语义 | Router 演进 |
| **P3** | SLO 闭环扩缩、Batch API、GAIE 互操作 | 对手已有 | 产品补齐 |

## 4. 明确不做（已排除）

| 不做 | 原因 |
|------|------|
| 引擎内核 / 前端（Zone A/B、S1） | `vllm-rs` 已做，且上游免费维护（§4） |
| 自造 KV 传输实现 / Rust 重写 LMCache（S3） | KV 字节永远旁路 Nebula；LMCache 是引擎进程内 KV 面 |
| fork vLLM 插件碰内核 | Zone A/B：耦合 + 随模型升级变（§11） |
| 单请求跨节点实时接力的严格 PD（v1） | 涉及流式切换、首 token 归属、取消传播等脆弱强耦合；先用共享 KV 角色化路由拿到尾延迟收益 |
| ZMQ 调参 / CPU 绑核 | 实测无收益（§3/§6） |

## 5. 风险与时间窗

1. **窗口有限**：llm-d `non-kubernetes-mode` 一旦落地，「无 K8s」护城河被正面侵入。
2. **资源不对等**：Dynamo 是 NVIDIA + Rust，正面拼广度必输。
3. **对策：窄而深**——把「无 K8s + 治理 + KV/Stage 感知编排」三个点做透，而不是「vLLM 有什么我都要有」。

## 6. 衡量指标（换 KPI）

不要再用**单请求引擎侧 TTFT** 作主 KPI（由引擎决定，且保护引擎不等于降低客户端延迟）。改看客户端全栈口径（见 [`benefit-verification.md`](benefit-verification.md)）：

1. **满足 SLO 的有效吞吐（Goodput）与拒绝率**（前缀亲和收益）；
2. **客户端 P99 尾延迟（TTFT p99 / e2e p99）**（准入 + `pd_split` 阶段隔离）；
3. **部署门槛与恢复时间**（起栈时间、依赖数、故障注入降级与自愈）；
4. **多租户隔离与可问责**（治理）。

## 7. 性能与产品重点分层

1. **层次 1：把 Nebula 自己的开销降到 0**（单跳嵌入式 Router、请求/响应流式透传；实测 Gateway 自身处理开销 ~0.1ms，见 [`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §12.2–12.3）。
2. **层次 2：把跨副本/跨请求/跨租户的编排增益做出来**（有界公平准入 + P1c 前缀亲和 + `nebula-indexd` + `pd_split` 共享 KV 角色化路由）。

---

## 8. 最近核心落地的三步优化（State- & Stage-Aware 闭环）

1. **P1c 真前缀亲和（移动计算到数据，见 [`prefix-affinity.md`](prefix-affinity.md)）**
   - **做法**：不再用“聚合命中率最高”的伪启发式，而是消费 vLLM 的 ZMQ `BlockStored` 事件，固定全栈 `PYTHONHASHSEED` 与 `sha256_cbor` 块链式哈希（[`kv_hash.rs`](../../crates/nebula-router/src/kv_hash.rs) / [`kv_events.rs`](../../crates/nebula-router/src/kv_events.rs)），并在 Router 侧支持 `chat_template` 精确分词对齐。
   - **收益**：让每个前缀的 KV 只驻留一份，集群有效缓存容量线性叠加。在 96 前缀过载压测下（见 [`benefit-verification.md`](benefit-verification.md)），冷启动拒绝率从 **12% 降到 0%**（goodput **2.4×**），暖态有效吞吐 **+23%**。
2. **`nebula-indexd` 全局 KV 索引服务（元数据与热路径解耦，见 [`prefix-affinity.md` §5.8](prefix-affinity.md#58-全局索引服务-nebula-indexdbatch-2-2026-10-11)）**
   - **做法**：独立二进制 [`nebula-indexd`](../../crates/nebula-router/src/bin/nebula-indexd.rs) 作为引擎 KV 事件的唯一稳定汇聚点，维护权威索引并通过“原始帧增量转发 + 周期快照（`!reset` / `!snapshot`）”扇出给多个 Router / 嵌入式 Gateway。
   - **收益**：Router 热路径 100% 查本地内存（零外部 RPC），重启可在单个快照周期内自愈，indexd 挂掉只退化为本地存量，不阻塞推理。
3. **`pd_split` v1：基于共享 KV 的角色化放置与路由（见 [`../arch/pool.md` §pd_split](../arch/pool.md#pd_split-拓扑与角色化放置p2c-pd-设计稿-v112026-10-12经独立评审-rework-修订)、[`engines.md` §7](engines.md#7-pd-分离传输选型与引擎版本钉死batch-3-第一步2026-10-11)）**
   - **做法**：不绑 NVIDIA 专属的 NIXL / `p2p_nccl`，选型硬件无关的 **LMCache（`lm://` 纯 TCP 共享 KV 存储 + `lmcache_controller`）**。副本按角色划分为 `prefill`（`kv_producer`，负责算冷前缀并写入共享存储）和 `decode`（`kv_both`，命中暖前缀直接从共享存储 Retrieve）。Router 通过查询 controller `/lookup` 判断冷暖（[`Router::controller_warm`](../../crates/nebula-router/src/lib.rs#L341)）：**冷流量进 Prefill 池生产 KV，暖流量进 Decode 池消费 KV**。
   - **收益**：消除了重度冷 Prefill 对稳态请求尾延迟的阻塞，暖态下客户端 **TTFT p99 降低 21%、e2e p99 降低 18%**。

---

## 9. 从 MapReduce 学到的五条系统铁律

在 [`../arch/roadmap.md`](../arch/roadmap.md#L258-L309) 的演进拆解与 `pd_split` 设计中，LLM 推理的 **Prefill → KV Cache → Decode** 本质上就是一次微型的 **Map → Intermediate State (Shuffle) → Reduce**。我们从 MapReduce 经典系统设计中吸收并落地了五条原则：

### 9.1 数据局部性优先（Data Locality：Move Compute to Data）

- **MapReduce 教训**：网络带宽远比本地内存慢，GFS + MapReduce 的第一原则是把计算调度到持有 Input Split 的节点上，而不是到处拉数据。
- **Nebula 的落地**：
  - 在 `replicated` 模式下，KV Cache 就是已经算好的百 MB 级中间数据。[P1c 前缀亲和](prefix-affinity.md#56-验收-真机完成2026-10-1185090) 就是把后续请求“调度到已经持有该 Prefix KV 的副本上”。
  - 在 `pd_split` 的 A/B 实测（[`../arch/pool.md` §收益 A/B](../arch/pool.md#收益-ab-2026-07-13客户端口径评估建议-3)）中同样验证了这条铁律：当 96 个宽前缀冷启动主导时，1:1 的 `pd_split` 反而比 `replicated` 慢（3.15 vs 3.72 rps），因为 **TCP 远端拉取 KV（~0.2 GB/s）永远慢于本地 GPU 显存直接命中**——所以不能盲目拆单请求 PD，且需要在 v1.1 补齐 Decode 池内的多 owner 本地命中优先（[`../arch/pool.md`:L194](../arch/pool.md#L194)）。

### 9.2 Map（Prefill）与 Reduce（Decode）分池隔离，解决 Straggler（长尾拖慢）

- **MapReduce 教训**：Map 是计算密集的批吞吐阶段，Reduce 是汇聚阶段；如果混跑且没有隔离，一个重度大任务就会制造长尾（Straggler）拖慢全场。
- **Nebula 的落地**：Prefill 是 **Compute-bound**（算力打满、耗时随长度暴涨），Decode 是 **Memory-bandwidth-bound**（逐 token 延迟敏感）。在 `replicated` 混跑时，一个长冷 Prefill 插队就会把同卡正在生成的请求的 TTFT/TPOT p99 顶高。`pd_split` 把冷请求隔离到 `prefill` 池“干脏活”写远端 KV，保护 `decode` 池专注于命中缓存后的低延迟服务，从而拿到了 **TTFT p99 −21%** 的尾延迟收益。

### 9.3 用“外部共享中间存储”解耦 Stage，而不是点对点硬绑（External Shuffle Service）

- **MapReduce / Spark 演进教训**：早期 Map 与 Reduce 点对点直传，一旦上下游耦合或节点抖动，整个流水线就会卡死；后来演进出 **External Shuffle Service / 远端中间层**，Producer 只管写、Consumer 按确定性 Key 去拉。
- **Nebula 的落地**：
  - **明确拒绝在 v1 做“单请求跨节点接力”的严格 PD 分离**（即同一个请求在 A 算完立刻 handoff 给 B 续接——那需要处理流式切换、首 token 归属、取消传播等高脆弱状态，见 [`../arch/pool.md`:L97-L99](../arch/pool.md#L97-L99)）。
  - **改用 `lmcache_server` 充当共享 KV 中间层**：Prefill 副本（Mapper）作为 `kv_producer` 把算好的 KV block 按确定性 hash key 写入共享存储；后续请求到来时，Decode 副本（Reducer）直接按相同的 deterministic hash（统一 `PYTHONHASHSEED` + `sha256_cbor`）从共享层取回 KV。上下游在单次请求的生命周期上**完全解耦**。

### 9.4 中间数据丢失 = 重算（Recompute on Failure），用幂等换高可用

- **MapReduce 核心哲学**：为什么 MapReduce 能跑在廉价机器上？因为中间数据不是事务数据库——**中间结果丢了、节点挂了，重算（Recompute）即可，绝不牺牲正确性与可用性**。
- **Nebula 的落地**：KV Cache 本质上是纯函数的确定性中间产物（给定 token 序列，重算一遍结果完全一样）。我们在 [`../arch/pool.md` §故障注入](../arch/pool.md#lmcache_server-与故障注入-2026-10-13-实测评审-2-关闭) 中实测验证了这一设计：
  - **杀掉 `lmcache_server`**、**`iptables DROP` 制造网络黑洞**、甚至 **长请求传输中途杀掉存储**：全部 **0 拒绝（HTTP 200）**。
  - Decode 副本拉不到远端 KV 时，直接回退本地自 Prefill（Recompute）；Router 连不上索引或 controller `/lookup` 超时（200ms）时，直接降级按冷请求路由。**任何缓存/中间层故障只降级性能（退化回普通重算），绝不挂起或报错**。

### 9.5 单一位置协调者（Master Location Directory）与数据面旁路

- **MapReduce 教训**：Master 只维护中间块在哪里的目录（Location Directory），真正的大块数据流直接在 Worker 和存储间走，绝不经过 Master 转发。
- **Nebula 的落地**：
  - KV 字节流只走引擎与 `lmcache_server` 之间的 `lm://` 通道，**永远不经过 Nebula Gateway/Router**（见 [`engines.md`:L93](engines.md#L93)）。
  - Nebula 只通过 [`nebula-indexd`](../../crates/nebula-router/src/bin/nebula-indexd.rs) 或 `lmcache_controller` 的 `/lookup` 询问一句“这个 token 前缀的 KV 块现在在不在池子里？”，拿到 `warm / cold` 信号后做路由决策，保持控制面与路由层的极度轻量。

---

## 10. 边界抽象与核心工程哲学：用约束替代强耦合

### 10.1 核心哲学：Constraints & Invariants over Tight Coupling

传统的跨节点 PD 分离走的是**运行时强耦合**路线（进程间实时握手、状态交接、流式拼接），任何一端慢、断或版本微调都会引发级联故障。Nebula 的解法是：**各个 Stage 之间不直接对话、不共享运行时会话，而是靠三组严格的“前置约束（不变量）”在各自边界内独立运行，自然对齐**：

1. **哈希同源约束（替代跨进程状态同步）**
   Router 不去问引擎内部怎么分块，引擎也不回调 Router；而是通过环境约束——强行统一 `NEBULA_KV_HASH_SEED == PYTHONHASHSEED`、钉死 `--prefix-caching-hash-algo sha256_cbor`、固定 `block_size=16`、共用同一份 `tokenizer.json`（见 [`../arch/pool.md` §数据模型增量](../arch/pool.md#数据模型增量实现契约)、[`prefix-affinity.md` §5.2](prefix-affinity.md#52-hashsha256_cbor-链式需显式固定-seed--algo)）。只要满足这组数学约束，Router 算出的 8 字节 key、Prefill 写进存储的 key、Decode 去拉取的 key **天然 100% 逐位一致**，零运行时耦合。
2. **准入与兼容硬约束（替代运行时的 `if-else` 适配）**
   不在数据面代码里写平台或版本分支（见 [`../arch/control-plane-adapters.md` §4.3](../arch/control-plane-adapters.md#43-热路径只认这三件事)），而是用部署期硬约束把不满足前提的组合直接拒绝在门外：
   - 拓扑约束：`pd_split` 必须 `TP=1`、同 `kv_cache_dtype`、同硬件族、且至少 1 Prefill + 1 Decode（[`validate_pd_split`](../../crates/nebula-common/src/model_deployment.rs#L296)）；
   - 产物约束：镜像 Digest 必须先过 `scripts/check_lmcache_compat.sh` 真机正数传输门禁，再写入 `NEBULA_APPROVED_PD_IMAGES` 白名单（[`validate_pd_split_image`](../../crates/nebula-common/src/model_deployment.rs#L258)，见 [`../arch/pool.md` §部署产物绑定](../arch/pool.md#部署产物绑定评审-4-修正)）。
   **把复杂度全部收敛为启动前的“静态校验约束”，让运行时的热路径极度简单。**
3. **“只发 Hint + 随时可重算”的弱耦合契约（替代分布式事务）**
   所有跨组件信号（ZMQ KV 事件、`nebula-indexd` 快照、controller `/lookup`）**只做路由偏置（Hint），不做正确性依赖**。哪怕中间存储被杀、网络黑洞、索引清空，Decode 副本无非是 Miss 后自己做一次本地 Prefill（Recompute），**把分布式强耦合的“可用性风险”降级成了单纯的“延迟退化”**。

### 10.2 纵向边界抽象：Zone A / B / C（按“是否随模型升级而变”切一刀）

对照 [`hw/vllm-serving-perf.md` §10](hw/vllm-serving-perf.md#10-引擎边界分解哪些能优化哪些已被上游吃掉分析)，整个推理栈按变化率切为三层：

| 分区 | 包含内容 | 随模型/引擎升级变吗？ | Nebula 的定位与边界 |
|------|----------|------------------------|---------------------|
| **Zone A（模型耦合层）** | 模型结构、Attention/MoE Kernel、量化 | **剧烈变化**（每个新模型都变） | **绝不碰**（交给 vLLM/SGLang 上游） |
| **Zone B（引擎半稳定层）** | 引擎内 Scheduler、KV Block Manager、CUDA Graph、进程内 IPC | **随引擎版本变**（内部 API 脆弱） | **不侵入、不 fork**（只通过 CLI 参数与插件口注入配置） |
| **Zone C（跨实例稳定层）** | 跨副本前缀亲和、角色化池编排、集群准入队列、多租户治理 | **与模型无关（稳定）** | **Nebula 的唯一主战场**（把引擎当黑盒，天然对模型升级免疫） |

### 10.3 横向演进抽象：S1 / S2 / S3 三层 Stage 边界

对照 [`../arch/roadmap.md`](../arch/roadmap.md#L258-L309)，阶段化（Stage）演进拆为三层职责边界：

- **S1（引擎内 Stage 重构）—— 不做**：引擎进程内部的 Prefill/Decode 线程拆分、Chunked Prefill、Kernel 级流水线，属 vLLM 上游在 Zone B 的职责，Nebula 不插手。
- **S2（跨副本 Stage 角色化编排）—— Nebula 核心做**：在控制面通过 `HardwarePool` 与 `ReplicaRole`（`prefill` / `decode`）把副本分池，用声明式 `Placement` 约束谁生产 KV、谁消费 KV，并在 Router 侧按 `warm / cold` 信号做阶段分流。
- **S3（共享/分层 KV 中间存储）—— 借力标准组件，不自造传输**：用硬件无关的 `lmcache_server` 充当 External Shuffle Store，Nebula 只管下发连接配置（`remote_url: lm://...`）和读取块位置目录（`/lookup`），**KV 字节流永远旁路 Nebula**。

> **一句话收敛结论**：Nebula 死守 **Zone C（引擎黑盒之上）**，在 **S2（跨副本角色与路由）** 发力：不造引擎内核（S1）、不搬 KV 字节（S3）、不做单请求跨进程硬绑交接；而是用**“部署期硬约束 + 统一哈希不变量 + 可重算的共享中间层”**，以零强耦合的方式拿到跨副本 KV 复用与 PD 隔离的收益。

