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
| **全局 KV 索引 `nebula-indexd`** | ✅ Batch 2 已落地（[`indexd.rs`](../../crates/nebula-router/src/bin/indexd.rs)） | 原始帧转发 + 周期快照扇出；Router 热路径 100% 查本地内存 |
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
   - **做法**：独立二进制 [`nebula-indexd`](../../crates/nebula-router/src/bin/indexd.rs) 作为引擎 KV 事件的唯一稳定汇聚点，维护权威索引并通过“原始帧增量转发 + 周期快照（`!reset` / `!snapshot`）”扇出给多个 Router / 嵌入式 Gateway。
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
  - Nebula 只通过 [`nebula-indexd`](../../crates/nebula-router/src/bin/indexd.rs) 或 `lmcache_controller` 的 `/lookup` 询问一句“这个 token 前缀的 KV 块现在在不在池子里？”，拿到 `warm / cold` 信号后做路由决策，保持控制面与路由层的极度轻量。

---

## 10. 边界抽象与核心工程哲学：用约束替代强耦合

### 10.1 核心哲学：Constraints & Invariants over Tight Coupling

传统的跨节点 PD 分离走的是**运行时强耦合**路线（进程间实时握手、状态交接、流式拼接），任何一端慢、断或版本微调都会引发级联故障。Nebula 的解法是：**各个 Stage 之间不直接对话、不共享运行时会话，而是靠三组严格的“前置约束（不变量）”在各自边界内独立运行，自然对齐**：

1. **哈希同源约束（替代跨进程状态同步）**
   Router 不去问引擎内部怎么分块，引擎也不回调 Router；而是通过环境约束——强行统一 `NEBULA_KV_HASH_SEED == PYTHONHASHSEED`、钉死 `--prefix-caching-hash-algo sha256_cbor`、固定 `block_size=16`、共用同一份 `tokenizer.json`（见 [`../arch/pool.md` §数据模型增量](../arch/pool.md#数据模型增量实现契约)、[`prefix-affinity.md` §5.2](prefix-affinity.md#52-hashsha256_cbor-链式需显式固定-seed--algo)）。只要满足这组数学约束，Router 算出的 8 字节 key、Prefill 写进存储的 key、Decode 去拉取的 key **天然 100% 逐位一致**，零运行时耦合。
2. **准入与兼容硬约束（替代运行时的 `if-else` 适配）**
   不在数据面代码里写平台或版本分支（见 [`../arch/control-plane-adapters.md` §4.3](../arch/control-plane-adapters.md#43-热路径只认这三件事)），而是用部署期硬约束把不满足前提的组合直接拒绝在门外：
   - 拓扑与兼容约束：[`validate_pd_split`](../../crates/nebula-common/src/model_deployment.rs#L296) 强制校验 `engine_type == "vllm"`、`kv_sharing` 必填（`lm://` + `serde == "naive"`）、全部副本显式指定非 `Mixed` 角色且满足 `>= 1 Prefill + >= 1 Decode`；按副本池平台兼容矩阵（[`validate_compat_for_replica_specs`](../../crates/nebula-control/src/deploy.rs#L314)）与 [`../arch/pool.md` §兼容性矩阵](../arch/pool.md#L201) 约束 v1 适用边界（同硬件族、同 `kv_cache_dtype`、`TP=1`）；
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

---

## 11. 2026 LLM Serving 技术趋势参照与 Nebula 落点映射

> 参照基线：2026 一线推理工程实测与分布式 Serving 框架演进报告（涵盖 `llm-d`、`NVIDIA Dynamo`、`AIBrix`、`Mooncake / sgl-router`、Google TPUv7 AgentX 基准、DSpark/DFlash2 投机采样、NVIDIA TriAttention、DeepSeek EPLB / V4.1-Flash、Continuum / Autellix Agent 调度、Quail AI-SQL 等）。
> 目的：用 **Zone A/B/C** 与 **Stage S1/S2/S3** 标尺，将外部最新技术趋势拆解为 **（1）战略印证、（2）Zone C / S2 数据面可直接吸收的高 ROI 改进、（3）L2/L3/L4 控制面物理规则、（4）Zone A/B 仅靠镜像与兼容矩阵纳管的边界清单**。

### 11.1 战略印证：分布式 Serving 的“趋同演化”与 AgentX 基准背书

1. **分布式 Serving 层六大趋同方向与 Nebula 的同构性**
   工业界在单机引擎（vLLM / SGLang / TRT-LLM）之上已全面放弃传统的“无状态网关 + 轮询负载均衡 + GPU SM 利用率扩缩容”，收敛为与 Nebula 高度同构的范式：

   | 业界趋同方向（`llm-d` / `Dynamo` / `AIBrix` / `Mooncake`） | 解决的核心物理墙 | Nebula 当前状态与对应实现 |
   |---|---|---|
   | **1. 网关与选路解耦**（Envoy `ext_proc` + GAIE `InferencePool` / EPP） | L7 网关对显存状态失明；选路需感知 Token/KV | ✅ 已具备：Gateway/嵌入式 Router 消费规范化 `/endpoints/` 与 `/stats/`（见 [`../arch/control-plane-adapters.md`](../arch/control-plane-adapters.md)） |
   | **2. 固定块哈希 + 异步事件流前缀索引**（16/64-token Block Hash + ZMQ/gRPC 事件 + 本地无锁树/表） | 集中式查表网络延迟爆炸；Round-Robin 破坏 Prefix Cache 局部性 | ✅ 已落地：[`kv_hash.rs`](../../crates/nebula-router/src/kv_hash.rs)（16-token `sha256_cbor`）+ [`kv_events.rs`](../../crates/nebula-router/src/kv_events.rs)（ZMQ `BlockStored`）+ [`indexd.rs`](../../crates/nebula-router/src/bin/indexd.rs) |
   | **3. 跨机 KV 传输接口归一**（`vLLM KVConnector` + NIXL / LMCache / Mooncake Store） | 跨机 HBM/DRAM/远端池缺乏统一异步原语 | ✅ 已落地：选用硬件中立的 `LMCacheConnectorV1` + `lmcache_server`（`lm://` 纯 TCP），不绑 NVIDIA 专属 NIXL（见 [`engines.md` §7](engines.md#7-pd-分离传输选型与引擎版本钉死batch-3-第一步2026-10-11)） |
   | **4. PD 角色分池与短请求旁路**（Prefill 池小 TP 算力密集 vs Decode 池带宽密集 + Threshold Gating） | 长 Prefill 阻塞 Decode TPOT；短请求跨机传输得不偿失 | ✅/⏳ `pd_split` v1 角色路由已落地（[`../arch/pool.md`](../arch/pool.md)）；**短请求旁路与多 owner 联合打分见 §11.2** |
   | **5. SLO 驱动的二元曲面规划**（看 `ISL/OSL + TTFT/TPOT` 而非 GPU `SM_UTIL`） | Decode 受限于显存带宽，带宽 100% 满载时 `SM_UTIL` 仅 ~25%，传统 HPA 失效 | ✅/⏳ [`slo.rs`](../../crates/nebula-common/src/slo.rs) 已评估 TTFT/错误率；**按 TTFT vs TPOT/KV 水位指导 P:D 池配比见 §11.3** |
   | **6. 分级 KV 池与网关侧主动削峰**（Tiered KVBM + 基于 KV 饱和度的 QoS Load Shedding） | KV 触顶触发引擎内 Recompute 导致吞吐雪崩 | ✅ 已落地：Gateway 有界公平队列（[`queue.rs`](../../crates/nebula-common/src/queue.rs)）+ Router `KV_CACHE_OVERLOAD_THRESHOLD`（[`lib.rs`:L856-L873](../../crates/nebula-router/src/lib.rs#L856-L873)） |

2. **AgentX 基准（Google TPUv7 FP8 + `llm-d` + vLLM vs GB200/GB300 NVL72 FP4）的反直觉启示**
   - **基准特征**：真实 Agent 负载具有四大特征——**多轮交互（Multi-turn）**、**长上下文累积（Long context）**、**极高前缀复用（High prefix reuse，读写比高达 12:1）**、**子 Agent 突发（Sub-agent bursts）**。
   - **核心结论**：在中高交互速率区间，仅使用**单池聚合服务（Aggregated Serving + Prefix Affinity + 投机采样）**的 TPUv7 FP8，每美元性价比反超了**动用复杂单请求跨机 PD 分离**的 GB200 NVL72 FP4 达 **3.16×**（仅在极低延迟区间 GB200 FP4 有 1.2× 优势）。
   - **对 Nebula 的直接印证**：这从外部独立证实了我们在 [`benefit-verification.md`](benefit-verification.md) 与 [`../arch/pool.md` §pd_split](../arch/pool.md#v1-定位评审-1-修正角色化完整请求路由不是单请求-pd) 中的判断——**在 Agent 高前缀复用场景下，Prefix Affinity 是第一收益源；不应盲目把所有流量都塞进高复杂度的单请求跨机 PD 接力，而应坚持“前缀亲和优先 + 共享 KV 角色化路由”**。

---

### 11.2 Zone C / S2 数据面与路由层：4 项高价值可吸收设计（待实现）

以下四项均严格落在 **Zone C（引擎黑盒之上）**，不侵入引擎内核，可直接作为 Router / Gateway 的增量演进：

#### 1. `pd_split` 的“短请求智能旁路（Short-Prompt Bypass / Threshold Gating）”

- **技术原理与行业参照（`llm-d` / `NVIDIA Dynamo`）**：
  并非所有冷请求都值得走 PD 分离或远端 KV 检索。当一个请求的未命中 Prompt 长度很短（`Uncached_Prompt_Length < Threshold`）且 Decode 节点算力有富余时，走远端 lookup/传输的固定开销（或把短请求全部挤到 Prefill 池排队）反而得不偿失；此时应直接旁路到 Decode 节点做原地 **Chunked Prefill + Decode**。
- **Nebula 现状痛点**：
  1. 在 [`engines.md` §7.4](engines.md#74-cuda-侧-lmcache-tcp-pd-实测2026-10-1185090) 中我们已实测发现：LMCache 默认 `chunk_size=256`，**< 256 tokens 的短 Prompt 根本不会触发远端 chunk 命中**；
  2. 在 [`../arch/pool.md` §收益 A/B](../arch/pool.md#收益-ab-2026-07-13客户端口径评估建议-3) 中，1:1 的 `pd_split` 在冷启动场景比 `replicated` 慢（3.15 vs 3.72 rps），正是因为所有 `warm == false` 的请求被一刀切塞进单个 `Prefill` 副本串行排队。
- **对应代码触点与改造方案**：
  - **触点**：[`crates/nebula-router/src/lib.rs`](../../crates/nebula-router/src/lib.rs#L341-L398)（`Router::controller_warm`）与 [`lib.rs`:L898-L935](../../crates/nebula-router/src/lib.rs#L898-L935)（`Router::route_internal` 的 `pd_split` 分支）。
  - **设计**：引入阈值配置 `NEBULA_PD_BYPASS_TOKENS`（默认 `256`，对齐 LMCache chunk 大小）：
    1. 当请求分词长度 `token_ids.len() < NEBULA_PD_BYPASS_TOKENS` 时，`controller_warm` 直接短路返回，**省掉一次对 `lmcache_controller:9000/lookup` 的 HTTP POST**；
    2. 在 `route_internal` 中，若属于短请求且 `Decode` 池候选副本的平均/最小 `pending_requests` 未饱和，允许短冷请求直接进入 `ReplicaRole::Decode`（或全池 `LeastPending`）原地完成，不再单点打爆 `Prefill` 池。
    3. 暴露计数器 `nebula_router_pd_short_bypass_total` 保持可观测。

#### 2. 多 Owner 前缀索引 + “节省 Prefill vs 排队等待”联合代价打分（Joint Cost Function）

- **技术原理与行业参照（`NVIDIA Dynamo` KV Router / `sgl-router` / `AIBrix`）**：
  多副本下，同一个热门 System Prompt 或文档前缀往往同时驻留在多个副本的 HBM/DRAM 中，或者不同副本持有不同深度的前缀匹配。主流路由器不再采用“单赢家硬跳转”，而是维护每个候选节点 $w$ 的匹配深度 $\text{HitRate}(w)$，并与节点实时排队负载联合计算无锁代价函数（决策延迟 < 1ms）：
  $$\text{Score}(w) = \alpha \cdot \text{SavedPrefillTokens}(w) - \beta \cdot \text{QueueWaitCost}(w) - \gamma \cdot \text{KvUsagePenalty}(w)$$
- **Nebula 现状痛点**：
  1. 目前 [`PrefixIndex`](../../crates/nebula-router/src/kv_events.rs#L119) 的表结构为 `DashMap<(String, Vec<u8>), (u32, u64)>`（**8 字节块哈希单 key 单 owner**）：在 `pd_split` 拓扑下，`Prefill` 副本产生 `BlockStored` 事件会覆盖 `Decode` 副本的 owner 记录，导致角色过滤后无法在 `Decode` 池内识别谁已经本地持有该前缀（即 [`../arch/pool.md`:L194](../arch/pool.md#L194) 的 **评审 #5 待办项**）；
  2. 目前 [`PrefixAffinity::select`](../../crates/nebula-router/src/strategy.rs#L173-L198) 是二元选择：只要 `ctx.prefix_owner` 存在且未触发 95% KV 过载熔断，哪怕该副本已有大量 `pending_requests` 排队、而另一台副本完全空闲，也会硬打过去，在热点前缀突发时易制造单点排队 Straggler。
- **对应代码触点与改造方案（合并 `pd_split v1.1` 评审 #5 与 `Filter→Score→Pick`）**：
  - **触点**：[`crates/nebula-router/src/kv_events.rs`](../../crates/nebula-router/src/kv_events.rs)（`PrefixIndex`）、[`crates/nebula-router/src/strategy.rs`](../../crates/nebula-router/src/strategy.rs)（`SelectionCtx` / `PrefixAffinity`）。
  - **设计**：
    1. 将 `PrefixIndex` 升级为**多 owner 索引** `(model_uid, block_hash) -> SmallVec<[(replica_id, last_seen_ms); 4]>`；查询接口改为传入当前通过角色/健康过滤的 `candidate_replica_ids`，返回每个候选副本命中的块数 `HashMap<u32, usize>`（天然同时解决 `replicated` 多副本共有前缀与 `pd_split` Decode 池内本地命中优先）；
    2. 在 `PrefixAffinity::select` 中引入联合打分：用候选副本命中的 token 数（`matched_blocks * 16`）折算节省的 Prefill 代价，减去 `pending_requests` 排队惩罚；当原 owner 排队过深且未命中前缀很短时，自动溢流到空闲副本（记入 `nebula_router_strategy_fallback_total{reason="owner_congested"}`）。

#### 3. 面向 Coding Agent 的“Tool-Pause 感知”会话亲和与准入保护（Program- & Tool-Pause Aware）

- **技术原理与行业参照（UC Berkeley **Continuum** & **Autellix / Agentix**）**：
  - **痛点**：Coding Agent（Claude Code、Cursor、Devin 等）单次任务包含数十轮 `LLM 思考 -> 发出 Tool Call -> 停顿 2~30 秒等待本地编译/测试/搜索 -> 携带 Tool Result 继续推理`。传统推理栈将每次请求视为独立 FCFS 调用，导致 Agent 在工具停顿期间 KV Cache 被驱逐（产生严重的 **Scheduling Bubble**），或在工具返回时被网关限流队列当作普通新请求拦截，使前序已投入的累计 GPU 服务量（**PLAS, Program-Level Attained Service**）白白受损。
  - **Continuum / Autellix 解法**：感知响应中的 `tool_calls` 语义，估算工具返回窗口 $\hat{t}_{\text{tool}}$，为处于工具挂起态的 Agent 会话提供动态 TTL 锁存与关键路径高优先级调度（实测缩短端到端 Agent 完成时间 4～15×）。
- **Nebula 现状与对应代码触点**：
  - **触点**：Gateway 流式/非流式响应解析（[`crates/nebula-gateway/src/proxy_common.rs`](../../crates/nebula-gateway/src/proxy_common.rs)）、有界公平准入队列（[`crates/nebula-common/src/queue.rs`](../../crates/nebula-common/src/queue.rs)）、Router 会话亲和（[`crates/nebula-router/src/lib.rs`:L754-L778](../../crates/nebula-router/src/lib.rs#L754-L778)）。
  - **设计（Zone C 纯网关/路由层实现，零引擎侵入）**：
    1. **Tool-Pause 状态标记**：当 Gateway 转发响应检测到 `finish_reason == "tool_calls"`（或流式 `delta.tool_calls`）时，在进程内记录该会话/前缀指纹处于 `ToolPaused` 状态（默认租约 TTL `60s`）；
    2. **续接请求识别与准入优先**：当后续请求携带 `role: "tool"` 消息（或命中 `ToolPaused` 会话租约）在 TTL 窗口内返回时，判定为**活跃 Agent 续接请求**，在 [`EngineQueue`（`QueueMode::Fair`）](../../crates/nebula-common/src/queue.rs#L97) 中赋予优先于全新冷请求的放行水位（避免长链路 Agent 在第 $N$ 轮半途 429 夭折），并在 Router 中强锚定原持有副本。

#### 4. SGLang `/v1/score` 极速打分端点的 L1 能力声明与前缀亲和透传

- **技术原理与行业参照（SGLang `/v1/score` + JEV 压缩门控）**：
  在 Agent 路由分类、RAG 相关性重排与上下文语义清理（`fast-jev-compaction`）中，应用层不再让模型逐字自回归生成（耗时数秒），而是调用 SGLang 原生的 **`/v1/score`** 端点（对推理模型配合注入闭合 `</think>` 标签），仅通过 **1 次 Prefill 前向传播**直接获取各候选答案 token 的概率分布，将耗时降至 **几十毫秒**。
- **Nebula 对应代码触点与改造方案**：
  - **触点**：[`crates/nebula-common/src/capability.rs`](../../crates/nebula-common/src/capability.rs)（`EngineCapability`）、[`crates/nebula-router/src/prompt_hash.rs`](../../crates/nebula-router/src/prompt_hash.rs)、`nebula-gateway` 路由表。
  - **设计**：
    1. 在 `EngineCapability` 中增加 `score_api: SupportLevel`（SGLang 静态表标为 `Supported`，vLLM 按能力探测）；
    2. Gateway / Router 开放 `POST /v1/score` 透传，并提取请求体中的共享上下文文本计算块哈希走 `PrefixAffinity`——因为 `/v1/score` 是 **100% 纯 Prefill 负载**，命中副本本地 Prefix Cache 可直接省去绝大部分计算。

---

### 11.3 Zone C 控制面（L2/L3/L4）：3 条物理规律的参数化与规则化

报告中揭示的底层硬件与引擎物理规律，可以直接转化为 Nebula **L3 智能选型（[`selection.rs`](../../crates/nebula-common/src/selection.rs)）、L2 部署参数（[`engine_args.rs`](../../crates/nebula-common/src/engine_args.rs)）和 L4 SLO 诊断（[`slo.rs`](../../crates/nebula-common/src/slo.rs)）** 的内建规则：

#### 1. L3 智能选型定律：4-bit vs FP8 的并发拐点（Batch Size Roofline 衰减定律）

- **实测物理规律（DGX Spark，Qwen3.8-27B 对照实验）**：

  | 并发请求数 (Batch Size) | 4-bit 相比原生 FP8 的解码提速 | 物理瓶颈状态（Roofline 模型） |
  |---|---|---|
  | **1 并发（单流）** | **+27%** | 纯显存带宽受限（Memory-bound），权重字节减半近似减半访存时间 |
  | **4 并发** | **+20%** | 一次权重读取分摊给 4 条序列，带宽红利开始被计算稀释 |
  | **8 并发** | **+10%** | 逼近算术强度拐点 |
  | **16 并发** | **< 0.2%（完全相同）** | **计算受限（Compute-bound）**：一次权重读取服务 16 条序列，权重字节数不再是瓶颈 |
  | **长上下文 Prefill** | **−7% ～ −9%（4-bit 反而更慢）** | 非原生 FP4 通路下，每次 GEMM 前需在寄存器解包（Unpack）4-bit 权重，增加额外计算开销 |

- **Nebula L3 对应触点**：[`crates/nebula-common/src/selection.rs`](../../crates/nebula-common/src/selection.rs#L30-L98)（`WorkloadHint.concurrency`、`ModelProfile.quantization`、`SelectionConstraints`）。
- **落地规则**：在 `Selection` 推荐打分与可解释 `reasons` 中编码该 Roofline 拐点：
  - 当 `workload.concurrency <= 4`（单流/低并发边缘池）或硬件平台具备原生 W4A4 通路（如 `NVFP4`）时，优先加分推荐 4-bit 量化镜像；
  - 当 `workload.concurrency >= 16`（多租户/多 Agent 并发服务端）且显存容量允许时，优先推荐原生 **FP8**，并明确给出原因提示（`"concurrency >= 16 enters compute-bound regime where FP8 matches 4-bit decode throughput without 4-bit prefill unpack penalty"`）。

#### 2. L2 投机采样套件（DSpark / DFlash2 + Context Lookup）声明式装配与 Hybrid 模型 Prefix Cache 体检

- **实测物理规律**：
  1. **廉价草稿远胜昂贵猜测（Cheap passes beat good guesses）**：模型内置 MTP 每预测 1 个草稿 token 需串行跑一次完整词表投影（`hidden_dim -> 150k+ vocab`，单 token 成本 `0.153`）；而独立半自回归草稿模型 **DSpark**（1B 模型 + 非因果滑动窗口注意力，单次前向并行吐 7 个草稿 token，单 token 成本仅 `0.046`）端到端解码速度比 MTP **快 46%**。
  2. **真指标是“单次前向平均接受 Token 数 $\tau$”而非“接受率百分比”**：将草稿窗口从 $k=7$ 扩至 $k=14$，接受率从 `98.7%` 跌至 `68.7%`，但因 GPU Tensor Core 验证 14 个与 7 个 token 耗时几乎相同（后段槽位相当于“免费彩票”），实际生成速度反而 **提升 27%**。
  3. **DFlash2 + 上下文查找填补（Context Lookup Augmentation）**：在 RAG 与 Coding Agent 改代码场景中，输出大量复用输入上下文片段。利用 DFlash2 提议前几个 token，再直接从长输入上下文匹配复制后续词元凑满 **16-token 大验证块**，单张 RTX 3090 运行 Qwen3.8-27B 从基线 `82 tok/s` 冲至 **`381 tok/s`**（每步验证 16 个平均通过 15 个）。开源界（如 Reflection AI **Beam**）已开始将 `主模型 + 原厂 DFlash2 草稿器` 作为标准成套制品发布。
  4. **Hybrid Attention 模型的 Prefix Caching 静默关闭陷阱**：vLLM 对报告 `is_hybrid=True` 的混合注意力模型（如 Qwen3.8）曾默认静默禁用 Prefix Caching 且无启动警告；显式强制开启后，19K/53K 共享前缀 Prefill 提速 **14×～22×**。
- **Nebula L2/L4 对应触点**：
  - [`crates/nebula-common/src/model_spec.rs`](../../crates/nebula-common/src/model_spec.rs) / [`crates/nebula-common/src/engine_args.rs`](../../crates/nebula-common/src/engine_args.rs)：在 `ModelSpec` 中支持声明式投机采样配置（`speculative_config`：支持绑定伴生草稿模型 `dspark`/`dflash2`、验证块大小 `num_speculative_tokens` 以及上下文查找 `prompt_lookup_max`），由 Node / K8s Controller 统一渲染为引擎启动参数；
  - [`crates/nebula-common/src/engine_metrics.rs`](../../crates/nebula-common/src/engine_metrics.rs) + BFF 诊断：解析引擎暴露的平均接受长度指标（计算有效加速比 $\tau$），并在部署启用了 `prefix_affinity` 但引擎 `prefix_cache_queries` 长期为 0 时生成 `DiagnosticEvent` 告警，防止 Hybrid 模型因引擎默认行为静默失去前缀缓存。

#### 3. L4 SLO 闭环：以 `ISL/OSL + TTFT/TPOT` 二元曲面指导 `pd_split` 弹性配比

- **技术原理与行业参照（`NVIDIA Dynamo Planner` / `llm-d SLO Autoscaler`）**：
  由于 Prefill 是计算受限（反映在 **TTFT** 与输入长度 **ISL**），Decode 是显存带宽与容量受限（反映在 **TPOT/ITL**、输出长度 **OSL** 与 **HBM KV 水位**，此时 GPU `SM_UTIL` 仅 ~25% 完全失真），控制面必须以 `(TTFT, TPOT/KV_Usage)` 二元曲面作为扩缩容与角色配比的决策依据。
- **Nebula L4 对应触点**：[`crates/nebula-common/src/slo.rs`](../../crates/nebula-common/src/slo.rs)（`ModelSlo` 评估与建议生成）、[`crates/nebula-common/src/capacity.rs`](../../crates/nebula-common/src/capacity.rs)。
- **落地规则**：在 `pd_split` 拓扑的 SLO 违约诊断与容量建议中：
  - 当 **TTFT p99 突破阈值** 且 `ReplicaRole::Prefill` 池 `pending_requests` 饱和时 ⇒ 输出结构化建议 `scale_prefill_pool`（增加 Prefill 副本）；
  - 当 **TPOT/ITL 突破阈值** 或 `ReplicaRole::Decode` 池平均 `kv_cache_usage > 0.85` 时 ⇒ 输出结构化建议 `scale_decode_pool`（增加 Decode 副本）。

---

### 11.4 边界定力：属于 Zone A/B（S1）与模型架构层、Nebula“只纳管不自造”的前沿技术清单

遵循 [`AGENTS.md`](../../AGENTS.md) 第 13 条，以下在 2026 报告中带来显著单机收益的前沿技术均位于 **Zone A/B（引擎内核与模型架构层）**。Nebula **绝不编写相关算子或侵入引擎内存管理**，而是通过 **`EngineImage` 镜像版本演进 + `CompatibilityRule` 兼容矩阵（[`compat.rs`](../../crates/nebula-common/src/compat.rs)）+ `HardwarePool` 异构池标签（[`pool.rs`](../../crates/nebula-common/src/pool.rs)）** 零耦合吃进红利：

| 底层技术突破 | 所属层 | 核心物理机制（摘自 2026 报告） | Nebula 的正确对接方式（Zone C 边界） |
|---|---|---|---|
| **全模型巨核（Megakernels）**<br>（Inferact TPU Pallas / Lithos-Metal） | Zone A/B (S1) | 将数十层 Transformer 编译进单一常驻 Kernel，激活值全驻片上 SRAM/VMEM，消灭算子发射气泡与 HBM 往返（TPU 上 Kimi K3 达 709 tok/s；M5 Max 上 27B 达 200+ tok/s） | 作为新 `EngineImage` / `platform`（如 `tpu`、`apple-metal`）接入 `HardwarePool`，由 L1 OpenAI 契约透传 |
| **Warp 角色特化（TileRT）与并行验证通道（TensorFold）** | Zone A/B (S1) | Producer Warps（TMA 异步搬运 NVFP4）与 Consumer Warps（MMA 计算）100% 重叠达 `1ms/token`；TensorFold 用 Micro-GEMM 并行通道一次读权重验证多条草稿 | 在 L3 `PerformanceProfile` 中记录新引擎/镜像的实测吞吐与延迟，供选型层推荐 |
| **NVIDIA TriAttention**<br>（KV 物理块滑动紧缩） | Zone B (S1) | 解决长思维链稀疏驱逐后“每个 16-token PagedAttention 物理块残留 1～2 个幸存者导致 0 块可回收 OOM”及 FlashAttention 无注意力分数死结：用 **Pre-RoPE Q/K 几何聚类打分 + 每 128 步滑动物理紧缩**，KV 显存降 **10.7×**、解码快 **2.5×** | 由 vLLM/TRT-LLM 引擎升级内置；Nebula 通过 `/stats/` 的 `kv_cache_usage` 自动感知压缩后的真实物理显存水位 |
| **DeepSeek EPLB**<br>（专家并行动态负载均衡） | Zone B (S1) | 针对 WideEP（EP32/64）下不同负载激活热门专家造成 All-to-All 短板：将逻辑专家（256）与物理槽位（288）解耦，后台滑动窗口统计频次并异步热复制高热专家到冗余槽位，二级装箱零停机消除热点 | 已合入 vLLM/SGLang 上游；Nebula 在 `multi_node_tp` / WideEP 放置时通过 `engine_args` 注入 `--enable-eplb` 等声明式开关 |
| **非对称编解码与选择性专家量化**<br>（DeepSeek-V4.1-Flash / Meta Muse Spark Open / Bonsai 2） | Zone A | V4.1-Flash（552B MoE）Prefill 仅激活 8B、Decode 激活 16B + Bounded Replay 将 HBM KV 压至 1/4；Muse Spark（816B-A42B）仅对路由专家做 NVFP4 压至 530GB；Bonsai 2 三值化 `{-1,0,+1}` 端侧极小体积 | 在 `ModelProfile` 中登记模型架构特征与显存足迹，用于 L2 节点显存准入估算与 L3 成本/吞吐选型 |
| **AI-SQL 查询-推理联合规划**<br>（Quail Tree Attention） | 应用/框架层 | 针对 `AI.IF` / `AI.JOIN` 百万次调用，重排算子 + 行级 KV 驻留流水线 + 树状注意力，避免 vLLM 盲目 LRU 驱逐重算 5000 万 token（快 14×） | Nebula 在北向通过 `PrefixAffinity` + 多 owner 索引 + 可选 `x-nebula-replica-id` / 会话锚定配合上层批处理框架锁住热点前缀所在副本 |
