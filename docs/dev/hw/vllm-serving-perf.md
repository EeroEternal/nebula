# vLLM 服务层与 Nebula 数据面性能实验（RTX 5090）

> 日期：2026-09-20。真机：8× RTX 5090（32 GB，PCIe，**无 NVLink**）+ 2× Xeon Gold 6530。
> 引擎：vLLM `0.26.0`（Pod 镜像 `vllm/vllm-openai:latest`），Qwen2.5-7B-Instruct，TP=1，`max_model_len=32768`，bf16，prefix caching 开，`gpu_memory_utilization=0.92`（KV 249,616 tokens）。
> 控制面：Nebula `v1.8.0`（gateway `:8081` → router `:18081`），`nebula-k8s-controller` 管引擎 Pod。
> 工具：官方 `vllm bench serve`（`--backend openai-chat`，random 数据集，输入 128 / 输出 32 tokens）。
> 目的：量化「引擎天花板 / Nebula 开销 / vLLM 服务层收益」，并排除若干伪优化。

## 1. 引擎并发爬坡（直连引擎，Python 前端）

| 并发 | req/s | out tok/s | Mean TTFT | P99 TTFT | Mean TPOT | P99 ITL |
|------|-------|-----------|-----------|----------|-----------|---------|
| 1 | 3.0 | 96 | 36 ms | 45 ms | 9.6 ms | 11 ms |
| 4 | 11.3 | 361 | 49 ms | 65 ms | 9.8 ms | 16 ms |
| 8 | 21.2 | 677 | 63 ms | 81 ms | 10.0 ms | 18 ms |
| 16 | 34.5 | 1103 | 98 ms | 146 ms | 11.6 ms | 32 ms |
| 32 | 53.7 | 1719 | 214 ms | 370 ms | 12.1 ms | 71 ms |
| 64 | 78.4 | 2509 | 326 ms | 631 ms | 15.5 ms | 132 ms |
| 128 | 95.2 | 3047 | 438 ms | 878 ms | 27.9 ms | 140 ms |
| 256 | 105.7 | 3384 | 1111 ms | 2464 ms | 40.0 ms | 150 ms |

**读法**：吞吐天花板 ≈ **105 req/s / 3.4k tok/s**（C≈128–256 饱和）；**延迟拐点在 C≈8–16**，再往上纯粹是排队（C=256 首字 2.5 s、TPOT 40 ms）。引擎是 **decode 吞吐受限**，并发不是越高越好。

## 2. 直连 vs 经 Nebula（交错 A/B，C=32，5 轮均值）

| 指标 | 直连 | 经 Nebula | Δ |
|------|------|-----------|---|
| req/s | 65.53 | 65.13 | −0.6% |
| Mean TTFT | 141.9 ms | 148.2 ms | +4.4% |
| **P99 TTFT** | 197.0 ms | 234.7 ms | **+19%** |
| P99 ITL | 22.9 ms | 21.3 ms | 噪声 |

- **吞吐打平**，Nebula 只增加 **+19% 的 TTFT P99**——代价来自 gateway→router **两跳**。
- 方法论警告：最早用「先全跑直连、再全跑 Nebula」得到过「Nebula 快 30%」的**假象**（引擎冷启动 + 顺序偏差）。**必须交错测量（A/B/A/B）**。

## 3. CPU/NUMA 绑核：无收益（已排除）

引擎 Pod 默认 `affinity=0-127`（跨 4 NUMA）、k8s QoS=BestEffort。把 vLLM 进程绑到 GPU 所在 NUMA：

| C=32 | 基线（全核） | 绑 NUMA |
|------|--------------|---------|
| QPS | 89.3 | 85.2 |
| P99 TTFT | ~148 ms | ~157 ms |

差异在噪声内且方向略负 → **CPU/NUMA 不是瓶颈**（引擎在等 GPU），k8s 未绑核不是低垂果实。

## 4. vllm-rs（Rust 前端）vs Python 前端：最大的一颗果实

受控实验：两个**全新**同镜像、同参数的 Pod，唯一差别是 `VLLM_USE_RUST_FRONTEND=1`；交错 3 轮。

| 并发 | 指标 | Python 前端 | **Rust 前端** | Δ |
|------|------|-------------|---------------|---|
| C=8 | req/s | 21.40 | **23.11** | +8.9% |
| | Mean TTFT | 66.3 ms | **43.0 ms** | −35.0% |
| | P99 TTFT | 88.7 ms | **59.2 ms** | −33.2% |
| | P99 ITL | 18.0 ms | **10.8 ms** | −40.4% |
| C=32 | req/s | 66.9 | **77.0** | +15.1% |
| | Mean TTFT | 134.8 ms | **73.0 ms** | −45.9% |
| | P99 TTFT | 184.4 ms | **105.5 ms** | −42.8% |
| C=64 | req/s | 105.2 | **126.5** | +20.2% |
| | Mean TTFT | 216.1 ms | **115.7 ms** | −46.5% |
| | P99 TTFT | 361.5 ms | **212.0 ms** | −41.4% |

**原因不是日志**：再做一个 Python 前端对照，加 `--disable-uvicorn-access-log --no-enable-log-requests`，三档并发与默认**完全一致**。瓶颈是 Python 前端本身（uvicorn + tokenize/detokenize + SSE 组装）在并发下的处理开销。

## 5. vLLM 内部事实（决定后续方向）

- **`vllm serve` 的前端可拔插**：`APIServerProcessManager`（Python）↔ `RustFrontendProcessManager`（`vllm-rs` 子进程）。切到 Rust 前端只需 `VLLM_USE_RUST_FRONTEND=1` + 二进制（默认 `site-packages/vllm/vllm-rs`，已随包发布），**不改 vLLM Python 代码**。
- Rust 前端与引擎之间用 **`ipc://`（Unix socket）+ MessagePack**，不是 TCP loopback（即“ZMQ 换 IPC”在上游已落地）。
- 上游 v0.29.0：Model Runner V2 全线默认；内建准入 `--max-num-queued-reqs/tokens`；KV-event 发布；tiered KV offload。

## 6. 已排除的方向

- **单机 PD 分离**：本机 8×5090 无 NVLink（纯 PCIe），Qwen2.5-7B KV ≈ 56 KB/token（8K=448 MB，32K=1.75 GB）。单机 P/D 跨卡 KV 传输是**净增成本**：短对话收益为负；长上下文高并发才有意义，且 **chunked prefill** 已能拿到大部分干扰隔离收益。
- **ZMQ/IPC 调参**：vLLM 已把 HWM=0、SND/RCVBUF 调到 0.5 GB，`envs.py` 无 ZMQ 旋钮；Rust 前端已改用 `ipc://`。收益为微秒级，对 60 ms 级 TTFT 是噪声。
- **“拆 vLLM、只用内核”**：收益是恒定小延迟，却破坏引擎解耦；不做。

## 7. 结论与建议

1. **引擎天花板 ~3.4k tok/s，甜点区 C≈8–16**；准入控制应把每引擎并发钳在此区间，避免用 P99 换吞吐。
2. **Nebula 不损吞吐，只加 +19% TTFT P99（两跳）** → 嵌入式 Router（单跳）是明确的回收目标。
3. **vLLM 服务层确有肉，但已被官方 `vllm-rs` 吃掉**：TTFT 砍 1/3~1/2、吞吐 +8~20%；关日志无效，属结构性问题。
4. **Nebula 定位**：不在服务层与 `vllm-rs` 竞争；价值在**集群级编排 / 缓存感知路由 / 多租户 / 生命周期**。服务层收益归 vLLM——**建议引擎侧开启 `VLLM_USE_RUST_FRONTEND=1`**。

## 8. 复现要点

官方压测（random 数据集**必须**给 tokenizer；`--skip-tokenizer-init` 与之不兼容）：

```bash
vllm bench serve --backend openai-chat --base-url http://<engine>/v1 \
  --endpoint /v1/chat/completions --model <served-name> \
  --tokenizer /models/weights --trust-remote-code \
  --dataset-name random --random-input-len 128 --random-output-len 32 --random-range-ratio 0 \
  --num-prompts N --max-concurrency C --num-warmups 4 --disable-tqdm \
  --percentile-metrics ttft,tpot,itl,e2el --metric-percentiles 50,95,99
```

对照实验注意：两个同镜像/同参数 Pod，仅差 `VLLM_USE_RUST_FRONTEND`；**交错**跑；`--num-warmups ≥ 4`，第一轮常有冷启动偏差，需剔除。

## 9. 后续：零代码优化清单（分析）

约束：只改**启动参数 / 环境变量 / 部署拓扑 / OS / k8s**，不改代码。
已开着、不必再折腾：`prefix caching`、`chunked prefill`、CUDA graphs、FlashInfer、`gpu_memory_utilization=0.92`。

状态：**A1 已实测**；其余为**分析预期，尚未验证**。

### 9.1 vLLM 进程（参数 / env）

| # | 手段 | 状态/预期 | 影响面 |
|---|------|-----------|--------|
| A1 | `VLLM_USE_RUST_FRONTEND=1` | **已实测**：TTFT −33~46%，吞吐 +8~20% | 延迟+吞吐 |
| A2 | `--kv-cache-dtype fp8` | KV 249k→~500k tokens，长上下文并发 7.6→~15 | 长上下文并发 |
| A3 | `--max-num-batched-tokens 16384`（0.26 默认 8192） | prefill 批更大 | 长 prompt |
| A4 | `--async-scheduling` | 调度/执行重叠 | 吞吐/P99 |
| A5 | `--speculative-config`（n-gram，无需 draft） | 重复性输出免费加速 | decode 吞吐 |
| A6 | `--gpu-memory-utilization 0.95` | 更多 KV | 并发 |
| A7 | 升级 vLLM 0.26 → 0.29 | MRV2 默认 + kernel 优化 + `max_num_batched_tokens` 默认 16384 | 全面（上游免费） |
| A8 | `--max-num-queued-reqs/tokens`（0.29 新） | 引擎侧准入，防过订阅 | P99 |

### 9.2 部署拓扑（改部署，零代码）

| # | 手段 | 预期 | 影响面 |
|---|------|------|--------|
| B1 | Nebula gateway/router 与引擎同机（或每 GPU 机一个边缘网关） | 收回跨节点一跳（对应 §2 的 +19% TTFT P99） | 延迟 |
| B2 | **多副本**（当前只用 1/8 张卡） | 吞吐**线性扩展**；也是 HA 前提 | 吞吐 |
| B3 | k8s QoS：`BestEffort` → Burstable/Guaranteed | 主机压力下不被抢/被逐 | 尾延迟稳定 |
| B4 | GPU ↔ NIC ↔ NUMA 对齐 | 小 | 延迟 |

### 9.3 Nebula 控制面（env / etcd）

| # | 手段 | 预期 | 影响面 |
|---|------|------|--------|
| C1 | 多副本后 `--routing-strategy prefix_cache_aware` | 前缀命中率↑ → 聚合 TTFT↓ | 延迟+吞吐 |
| C2 | 并发上限设在甜点区 **C≈8–16** | 阻止“吞吐换 2.5s 尾巴”（§1 实测） | P99 |
| C3 | 热路径关掉非必要观测（OBSERVE/审计/高基数 metrics） | 省 CPU | P99 |
| C4 | gateway/router upstream keep-alive 调优 | 小 | 延迟 |

### 9.4 模型 / 工作负载

| # | 手段 | 预期 | 影响面 |
|---|------|------|--------|
| D1 | 共享 system prompt（让 prefix cache 真命中） | 长 system prompt 可整段跳过 prefill | TTFT |
| D2 | FP8/INT4 权重量化（5090 支持 FP8） | decode 显存流量减半 → 接近 2× decode | decode 吞吐 |
| D3 | 客户端连接复用 / HTTP2 / 批量化 | 中小 | 吞吐 |

### 9.5 优先级（ROI）

| 优先 | 手段 | 类型 | 量级 |
|------|------|------|------|
| 高 | **A1** `VLLM_USE_RUST_FRONTEND=1` | 零代码 | TTFT 砍 1/3~1/2（已实测） |
| 高 | **B2** 多副本（用上空闲卡） | 改部署 | 吞吐线性 ×N |
| 中 | **C2** 并发准入钳在 C≈8–16 | etcd/env | P99 从秒级回落 |
| 中 | **B1** 网关与引擎同机 | 改部署 | 收回部分 +19% TTFT P99 |
| 中 | **D2** FP8 权重 | 换模型 | decode ~2× |
| 低 | A2/A3/A4 | 改参数 | 个位数~几十% |
| 低 | A7 升 0.29 | 升版本 | 上游白送 |

**判断**：降延迟优先 A1 → B1 → C2 → D1；提吞吐优先 B2 → D2 → A5。最被低估的是 **B2**（单副本只用 1/8 卡）与 **C2**（别过载比调快更值）。已排除：CPU 绑核（§3）、单机 PD（§6）、ZMQ 调参（§6）。

## 10. 引擎边界分解：哪些能优化、哪些已被上游吃掉（分析）

低垂果实（§9）基本摘完。若考虑「解构 vLLM、保留核心」，先按**是否随模型升级而变**分区：

| 区 | 内容 | 随模型升级 |
|----|------|-----------|
| **A. 模型耦合层** | 模型定义、attention/MoE kernel、量化、新架构 | **剧烈变** |
| **B. 引擎半稳定层** | 调度器、KV 管理、executor、CUDA graph、IPC 协议 | 随引擎版本变，不随模型 |
| **C. 稳定层** | HTTP/OpenAI 前端、路由、编排、多租户、缓存、准入 | **与模型无关** |

「保留核心」= 保留 A+B；优化 C。**C 就是「模型升级不影响」的地方**，也正是 Nebula 的位置。

### 10.1 C 区：上游已吃掉 vs 仍是空白

已被 `vllm-rs` 吃掉（不重写）：OpenAI 前端 + tokenizer/detokenizer、`ipc://` 传输。

仍是空白（vLLM 单实例不做，因为都是**跨实例 / 跨请求 / 跨客户端**）：

1. **响应缓存 / 请求去重**（真正绕过引擎）
2. 跨副本 **前缀亲和路由**
3. **集群级准入 / 背压 / QoS**
4. **PD 分离的跨节点编排**
5. 多引擎 / 多模型异构编排
6. 弹性 / sleep / scale-to-zero
7. 优先级 / 公平调度 / 租户隔离
8. SLO 驱动路由（送到健康、低负载副本）

### 10.2 B 区：唯一还值得想的，也最危险

调度 / KV 策略（prefix-aware 调度、priority、PD、KV 分层 offload）——这是 vLLM 的**核心价值**，上游正全力优化（MRV2 / tiered offload / KV events）。自研 = 高风险 + 持续追平 + 破坏引擎零侵入。**不值得**。

### 10.3 判断

- 「在引擎内替换前端 / IPC」→ 已被 `vllm-rs` 吃掉，不划算。
- 「把引擎当黑盒、在边界之上优化」→ 空间大，且天然对模型升级免疫。
- **能真正超过单实例直连 vLLM 的，只有跨请求 / 跨副本 / 跨客户端**——这也是 §9 里 B2/C1/C2 是真余地的原因。

### 10.4 最被低估：响应缓存 / 去重（零引擎改动）

| 形态 | 做法 | 命中场景 | 风险 |
|------|------|----------|------|
| **精确缓存** | `hash(model, messages, sampling)` → 缓存响应，命中直接返回，**跳过引擎** | 固定 system prompt、重试、模板问答 | 低（temperature=0 确定） |
| **请求合并** | 同一时刻 N 个相同请求 → 合并为 1 次引擎调用，fan-out | 惊群、批量重复 | 低 |
| **语义缓存** | embedding 相似 → 复用答案 | 近义问法 | 中高（正确性） |

vLLM 不做（无状态推理引擎）；对高重复率 workload 收益是**数量级**的，比任何前端调优都大。难点在流式 SSE 回放与 key 的精确性边界，但全在 C 区。

**排序**：响应缓存 / 去重 > 跨副本前缀亲和 > 集群准入 > PD 编排。
