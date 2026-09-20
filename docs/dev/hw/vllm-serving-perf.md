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
