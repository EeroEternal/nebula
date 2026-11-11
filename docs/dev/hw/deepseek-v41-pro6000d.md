# DeepSeek-V4.1-Flash 单机部署与实测 — rtx6kpro（8× RTX 6000D）

> 日期：2026-11-11。主机 `rtx6kpro`（8× NVIDIA RTX 6000D，单卡 85,651 MiB，PCIe，无 NVLink）。
> 镜像 `ghcr.io/local-inference-lab/vllm:karmic-kraken-beta`（manifest `sha256:bc48d33e…`）。
> 权重 `deepseek-ai/DeepSeek-V4.1-Flash-1017`（510,311,053,413 B）。原始日志留在真机 `~/…`，不入库。
> 目的：把上游 `rtx6kpro` wiki 的 TP4 `ds41-flash` 配方在本机跑通，并记录**必须偏离上游的三处**及其证据。

## 1. 环境

| 项 | 值 |
| --- | --- |
| GPU | 8× RTX 6000D，85,651 MiB/卡，拓扑 GPU0/1 同 PCIe switch（PXB）、GPU2/3 另一个 switch（互访 NODE） |
| 驱动 | 595.99.02，`nvidia-smi` 报 **CUDA 13.2**（镜像内 runtime 为 **13.4**，靠镜像自带 compat 库运行） |
| OS | Ubuntu 24.04.4 LTS，kernel 提供的 ifb/tbf 用于限速 |
| Docker | `docker.io` 29.1.3（containerd image store），data-root 在数据盘 |
| GPU runtime | `nvidia-container-toolkit` 1.20.2，`default-ulimits.memlock=-1` |
| 容器内 | torch `2.14.0a0+4fdf77b940.nv26.08`，CUDA runtime `13.4`，vLLM `0.1.dev21836+g6d66eed35`，NCCL `2.31.2` |

冒烟（容器内）：`nvidia-smi -L` 8 卡可见，`torch.cuda` 1024³ 矩阵乘正常 → **驱动 CUDA 13.2 + 镜像 13.4 的组合可用**，wiki 里「必须 ≥13.4」的告警在推理路径上未复现。

## 2. 权重

- 来源：同机房另一台机器已有的 `deepseek-ai_DeepSeek-V4.1-Flash-1017/main`（`aria2-x8+sha256sum` 下载，`.checksums.sha256` 齐全），rsync 过局域网：510,311,053,413 B / 84 文件 / **10 min 33 s / 768 MB/s**，随后 `sha256sum -c` **83/83 OK**。
- 与官方 repo 的关系：`config.json` 的 sha256 **完全一致**；48 个 `safetensors` 分片 sha256 **全部不同**（字节数相同）。属同一模型的另一次打包 → 本文吞吐数字**不可直接与 wiki 基准对标**。
- 布局：以软链壳挂成 HF 缓存（`refs/main` + `snapshots/<rev>/` → 真实文件），容器 `HF_HUB_OFFLINE=1` 直接从本地路径加载，全程无网络。

## 3. 最终启动命令（可用）

```bash
docker run -d --name ds41 --init --restart unless-stopped \
  --gpus '"device=0,1,2,3"' --network host --ipc host --shm-size 32g \
  --ulimit memlock=-1 --security-opt seccomp=unconfined \
  -v <hf-cache>:/root/.cache/huggingface \
  -v <weights>:/models/DeepSeek-V4.1-Flash:ro \
  -v <jit-cache>:/cache \
  -e PROFILE=ds41-flash -e HARDWARE_PROFILE=rtx-pro-6000-pcie \
  -e TP=4 -e PORT=8000 \
  -e MODEL=/models/DeepSeek-V4.1-Flash \
  -e HF_HUB_OFFLINE=1 -e TRANSFORMERS_OFFLINE=1 \
  -e GPU_MEMORY_UTILIZATION=0.96 \
  -e MAX_MODEL_LEN=32768 \
  -e MAX_NUM_SEQS=8 \
  -e VLLM_MEMORY_PROFILER_ESTIMATE_CUDAGRAPHS=0 \
  -e NCCL_P2P_LEVEL=LOC \
  -e VLLM_ENABLE_PCIE_ALLREDUCE=0 \
  -e NCCL_DEBUG=WARN \
  ghcr.io/local-inference-lab/vllm:karmic-kraken-beta
```

`MODEL` 接受**本地目录**，profile 的其余默认（`dtype=bf16`、`kv_cache_dtype=fp8`、B12X attention/MoE/linear、DSpark K7 投机、`engram-table-memory=ram`）保持不变。

## 4. 必须偏离上游配方的三处（含证据）

### 4.1 `NCCL_P2P_LEVEL=LOC` — 默认全量 P2P 会死锁

现象：4 个 worker 全部停在同一处，GPU 利用率 100%、功耗仅 ~95 W、无任何日志输出，`/health` 一直 000。

`py-spy` 抓栈（对容器内 worker 用宿主 PID）：

```
Stream.synchronize
  ← PyNcclCommunicator._init_comm   (vllm/distributed/device_communicators/pynccl.py:191)
  ← CudaCommunicator.__init__ → init_model_parallel_group → Worker.init_device
```

容器内 `torchrun` 隔离实验（4 进程 all_reduce）：

| 配置 | 结果 |
| --- | --- |
| 默认（`NCCL_P2P_LEVEL` 未设，镜像内为 SYS） | **挂死**（60 s 超时被杀） |
| `NCCL_P2P_LEVEL=LOC` | `ALLREDUCE OK` |
| `NCCL_P2P_DISABLE=1` | `ALLREDUCE OK` |

`nvidia-smi topo -p2p r` 对全部 pair 报 `OK`，但**跨 switch 的 P2P 实际不可用**。取 `LOC`：同 switch 内仍走 P2P，跨 switch 退化为主机内存。

### 4.2 `VLLM_ENABLE_PCIE_ALLREDUCE=0` — B12X 的 `comm.pcie` 同样挂

修好 NCCL 后，B12X 预热停在 `b12x waiting for ranks comm.pcie: 3620/3633`，45 分钟无进展；栈显示 `flashinfer.experimental.b12x.preparation.session` 内部在 `_synchronize` 处挂住 —— 与 4.1 同一根因（该 collective 也走 P2P）。关掉后改用 NCCL。副作用：`fuse_allreduce_rms` 由 True 变 False（少一层融合优化）。

### 4.3 显存预算按 **85.7 GiB** 卡重算

上游配方假设 4×96 GiB。实测：

| 设置 | 结果 |
| --- | --- |
| `gpu_memory_utilization=0.90`，`max_model_len=-1` | 启动失败：`Cannot auto-fit max_model_len: not enough GPU memory available to serve even a single token` |
| `0.95` + `max_model_len=131072` | 启动失败：131,072 需要 0.32 GiB KV，可用仅 **0.24 GiB**；引擎报「估计最大长度 29,952」 |
| **`0.96` + `max_model_len=32768` + `max_num_seqs=8` + 关 cudagraph 显存预估** | 启动成功，KV **429,380 tokens**，32K 请求并发 **13.10×** |

即：TP4 下权重+workspace 已吃掉约 83.5 GiB/卡，KV 只剩 ~0.3 GiB。**要拿回真实容量必须上 TP8**（见 §7）。

另：`engram-table-memory` 保持 profile 默认 `ram` → 2 层 × 23.60 GiB × 4 rank = **188.8 GiB 固定主机内存**，实测 `free` 符合。

## 5. 实测数据

| 指标 | 值 | 说明 |
| --- | --- | --- |
| 首次启动（JIT 缓存空） | ≈ 20 min | B12X 自动调优 3633 个 op 组合、31.6 万次基准、1.7 万次编译；产物落 JIT 缓存 |
| 二次启动（命中缓存） | ≈ 5 min | 同样到 CUDA graph 捕获完成只用 4 min |
| 显存占用 | 83.5 / 83.6 / 83.5 / 83.6 GiB（4 卡） | `gpu_memory_utilization=0.96` |
| 主机内存 | 242 GiB | 含 188.8 GiB Engram 表 |
| KV cache | 429,380 tokens | 32K 上下文 13.10× 并发 |
| 单请求 decode | **123 tok/s** | 22 prompt → 130 completion，1.06 s，temperature 0.6、thinking off |
| 短问答 | 15 prompt → 43 completion，1.67 s | thinking off |
| thinking 模式 | `13*17` → 18 reasoning tokens，答案 `221` ✅ | 默认 `thinking=true, reasoning_effort=high` |

正确性抽查（非基准）：MoE 解释、GPU 显存分层说明、算术，均通顺且答案正确。

## 6. 性能对比（本机 vs 上游 wiki vs 无投机）

方法：镜像内自带 `vllm bench serve`（与 `docs/dev/hw/vllm-serving-perf.md` 同工具同口径），
`--backend openai-chat`，random 数据集，**输入 512 / 输出 128 tokens**，24 条请求，`thinking=false`，
`temperature` 用服务端默认。KV 只有 0.3 GiB，`max_num_seqs=8`，所以 C8 已是本配置的实际上限。

| 场景 | 并发 | req/s | out tok/s | Mean TTFT | P99 TTFT | Mean TPOT |
| --- | --- | --- | --- | --- | --- | --- |
| 本机 TP4 + DSpark K7 | C1 | 0.99 | **126.3** | 262 ms | 283 ms | **5.91 ms** |
| 本机 TP4 + DSpark K7 | C4 | 1.90 | **242.7** | 362 ms | 780 ms | 13.34 ms |
| 本机 TP4 + DSpark K7 | C8 | 2.99 | **382.7** | 432 ms | 819 ms | 16.64 ms |
| 本机 TP4，`--mode off` | C1 | 0.74 | 94.6 | 256 ms | — | 8.63 ms |
| 本机 TP4，`--mode off` | C8 | 3.35 | 428.6 | 521 ms | — | 14.48 ms |

结论：

- **DSpark K7 在单请求上净赚 +33%**（126.3 vs 94.6 tok/s，TPOT 5.91 vs 8.63 ms），这是投机解码该有的形状。
- **但在 C8 反而 −11%**（382.7 vs 428.6）：投机把 KV 从 800,964 tokens 压到 429,380（草稿模型吃掉 ~0.24 GiB），
  加上每步多一次验证前向，批量场景下得不偿失。**本机这种「KV 极小」的配置里，投机只在低并发有价值**。
- 关掉投机后 `b12x` 只需要 444 个 op 组合（有投机时 3633），JIT 预热时间随之下降。

与上游 wiki 的口径差距（不构成缺陷结论，只说明条件不同）：

| 维度 | 上游 wiki（4×RTX PRO 6000 96 GiB，Max-Q） | 本机（4×RTX 6000D 85.7 GiB） |
| --- | --- | --- |
| C1 输出 | 249 tok/s | 126 tok/s |
| C8 聚合输出 | 805 tok/s | 383 tok/s |
| 上下文 | 自动（10⁶ 量级），KV 4,727,748 tokens | 32,768，KV 429,380 tokens |
| 互联 | 全量 P2P + `VLLM_ENABLE_PCIE_ALLREDUCE=1`（b12x oneshot） | `NCCL_P2P_LEVEL=LOC`，PCIe allreduce 关闭 |
| Engram | 同（ram） | 同（ram，188.8 GiB） |

差距的主因不是单一因素：单卡少 10 GiB → KV 少一个数量级（并发上不去）、跨 switch 走主机内存、
`fuse_allreduce_rms` 因关掉 PCIe allreduce 而失效、权重是重打包版。

**与「原生 vLLM」的对比无法给出**：这个 checkpoint 需要镜像里的 `deepseek_v41` 模型实现与 B12X /
Engram / DSpark 内核，上游 vLLM 不具备，换镜像根本起不来（`--tokenizer-mode deepseek_v41`、
`quantization=deepseek_v41_fp8` 都是该分支特有）。所以本文的「对比」只能是**同一镜像内的参数 A/B** +
**与上游同配方记录的并排**，而不是跨引擎横评。

## 7. 挂在 GPU 上时的诊断指纹

「GPU 利用率 100% + 功耗仅 ~90–95 W + 日志静默 + worker 进程 CPU 100%」= **NCCL/B12X collective 挂住**，不是编译、不是磁盘、不是权重加载。定位顺序：

1. `docker logs … | tail` 看最后一条是 `pynccl` 还是 `b12x …` 阶段；
2. `nvidia-smi --query-gpu=utilization.gpu,power.draw` 看功耗是否与利用率不匹配；
3. `py-spy dump --pid <宿主 PID>`（宿主装 `py-spy`，容器内因缺 `CAP_SYS_PTRACE` 会 Permission denied）；
4. 用容器内 `torchrun` 小脚本二分 NCCL 环境变量。

## 8. 未完成 / 风险

- **容量**：32K 上下文、8 请求槽、KV 0.3 GiB，只够功能验证，不能做并发压测。TP8 可把权重摊薄一半，但 profile 的 `speculative_config.draft_tensor_parallel_size=4` 需要一并调（或先 `--mode off` 验证）。
- **`VLLM_MEMORY_PROFILER_ESTIMATE_CUDAGRAPHS=0`** 是绕过 KV 分配失败的手段，引擎自身会警告「cudagraph 显存未被计入，长跑可能 OOM」。压测前应改回默认并重算 utilization（引擎建议 0.9759）。
- **权重是 `-1017` 打包版**：与官方分片字节不同，数值/吞吐不保证与 wiki 一致。
- **驱动 CUDA 13.2 vs 镜像 13.4**：本次推理路径正常，但 NCCL/B12X 的 P2P 异常是否与此相关未定论；升级驱动后应重测 `NCCL_P2P_LEVEL=SYS` 与 `VLLM_ENABLE_PCIE_ALLREDUCE=1` 是否可用。
- `VLLM_DISABLE_CUSTOM_ALL_REDUCE=1` **未生效**（pynccl 仍被初始化），真正生效的是 `NCCL_P2P_LEVEL=LOC`。

## 9. 原始证据（真机，不入库）

一次验收跑（2026-11-11 22:10）后落盘于真机 `/data01/ds41/evidence/`：

| 文件 | 内容 |
| --- | --- |
| `torch_smoke.txt` | `2.14.0a0+4fdf77b940.nv26.08 13.4 8` |
| `models.json` | `/v1/models` 原始响应（`max_model_len: 32768`） |
| `infer_smoke.txt` | 短问答请求/响应全文 |
| `bench_out.json` / `bench_out2.json` | 256-token 生成、thinking 模式的原始响应 |
| `bench.txt` | 上两者的耗时与 tok/s 摘要 |
| `bench/b_c{1,4,8}.log` | `vllm bench serve` 三档并发（含投机）原始输出 |
| `bench/n_c{1,8}.log` | `--mode off` 同口径原始输出 |
| `container.log` | 本次容器完整启动日志（含两处挂死与修复后的成功路径） |

## 10. 复现清单

```bash
# 1) 环境
apt-get install -y docker.io nvidia-container-toolkit   # Ubuntu 24.04 + NVIDIA repo
nvidia-ctk runtime configure --runtime=docker

# 2) 镜像（ghcr 直连不稳，镜像站速度差 → 用并行可续传的 OCI 拉取 + docker load）

# 3) 权重校验
sha256sum -c .checksums.sha256     # 期望 83/83 OK

# 4) 启动：§3 命令

# 5) 验收
curl -s localhost:8000/health
curl -s localhost:8000/v1/models
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{
  "model":"DeepSeek-V4.1-Flash",
  "messages":[{"role":"user","content":"say ok"}],
  "max_tokens":8, "chat_template_kwargs":{"thinking":false}}'
```
