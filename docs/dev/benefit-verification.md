# 收益验证报告：前缀亲和（客户端口径）

> 日期：2026-10-12。基线：评估建议 3（"满足 SLO 的有效吞吐、客户端 P99、拒绝率"——不只用命中率或引擎侧 TTFT）。
> 环境：60001（8×5090），2× vLLM v0.23.0 副本（Qwen2.5-7B），Gateway 准入队列 ON（`MAX_CONCURRENCY=16`，`fair`，`tenant_share=0.5`），Router 策略切换。
> 负载：96 前缀 × 2048 token / suffix 128 token / `stream=true` / C=32（超出单副本甜点区）。
> 工具：[`scripts/verify_bench.py`](../../scripts/verify_bench.py)（客户端侧测量流式 TTFT + e2e + 429/5xx 拒绝率 + SLO goodput，默认阈值 `--slo-ttft-ms 3000 --slo-e2e-ms 10000`，支持 `--raw-out` 导出逐请求 JSONL 原始样本）。
> 交错 4 轮（A→B→A→B→A→B→A→B，引擎从冷到暖逐步演化）。Rig 已清理。

## 结果汇总（暖态=第 3-4 轮，引擎 KV 已稳定）

| 指标 | least_pending (A3/A4 均值) | prefix_affinity (B3/B4 均值) | 差异 |
|------|---------------------------|------------------------------|------|
| **goodput (req/s)** | 27.0 | **33.3** | **+23%** |
| **TTFT p50 (ms)** | 892 | **681** | **−24%** |
| **TTFT p99 (ms)** | 1454 | **1348** | **−7%** |
| **TTFT p999 (ms)** | 1457 | **1367** | **−6%** |
| **e2e p99 (ms)** | 1683 | **1548** | **−8%** |
| **拒绝率** | 0% | 0% | —（暖态无拒绝） |

## 冷启动轮（第 1 轮，引擎 KV 从零开始）

| 指标 | least_pending | prefix_affinity |
|------|---------------|-----------------|
| goodput | 11.9 | **28.7** |
| 拒绝率 | **12.0%**（46/384 429） | **0%** |
| TTFT p99 | 2974 | **1412** |

> 冷启动与顺序偏置说明：首轮采用 A（`least_pending`）→ B（`prefix_affinity`）顺序在同一组引擎上执行，B1 部分受益于 A1 留下的已预热 KV，因此不能将首轮 2.4× goodput 差异全部归因于亲和路由本身。第 3–4 轮暖态（+23% goodput、−7% TTFT p99）消除了冷缓存从 0 填充的预热阶段偏差，但各轮内部仍沿用 A→B 顺序；若需彻底消除轮内顺序偏置，应采用 A→B / B→A 平衡交错（ABBA）或在每轮切换前清空引擎 KV。

## 关键发现

1. **冷启动轮受预热顺序混杂，应以第 3–4 轮暖态作为主参考口径**（首轮 12% 拒绝 → 0%、goodput 2.4× 含 A→B 共用渐暖引擎的顺序收益；第 3–4 轮消除了冷启动填充阶段的混杂，独立冷启动或零顺序偏置基准需配合每轮前清空引擎 KV 或 ABBA 平衡顺序复测）。
2. **暖态收益存在但收窄**（goodput +23%，p99 −7%）——96 前缀 × 2 副本仍超出单副本缓存，affinity 保持不重复存储；一旦前缀量小到全装进单副本缓存（§5.6 早测的 4 前缀场景），两策略等价。
3. **客户端 p99 的改善幅度 < 引擎侧 TTFT 的改善幅度**——评估的警告成立："保护引擎不等于降低客户端延迟"。Gateway 队列自身引入的等待（`x-nebula-queue-wait-ms`）占了客户端延迟的显著份额。**这是下一步优化目标**（动态并发/自适应准入），不是本次验证的缺陷。
4. **p999 改善（−6%）比 p50（−24%）小**——affinity 主要改善中位数附近的"重复 prefill"开销；极端尾部（调度抖动、chunk 逐出）不太受路由策略影响。

## 对 `pd_split` 与严格 PD 决策的输入

- **`pd_split` 实测权衡（保留负面结果，见 [`pool.md`](../arch/pool.md)）**：
  - 暖态（8 前缀）：尾部延迟改善（TTFT p99 −21%，e2e p99 −18%），但**有效吞吐下降约 5%**（28.66 → 27.33 rps）；
  - 宽前缀冷启动主导（96 前缀）：**吞吐与尾延迟均劣于 replicated**（3.72 → 3.15 rps，TTFT p99 47.7s → 54.0s），原因是 1:1 池配比将全部冷请求压向单 prefill 副本且 TCP 远端取回慢于本地显存命中。
- 因此 `pd_split` v1 仅适用于**高前缀复用的暖态尾延迟优化**，不是通用吞吐加速方案。

---

## 2026-10-18 复测：`pd_split` vs `replicated`（LMCache 0.5.5 真机）

> 环境：**101**（8×RTX 5090 / 32GB），Qwen2.5-7B-Instruct（bf16，TP=1），vLLM `v0.23.0` + **LMCache 0.5.5**，仅用 GPU 0/1。
> 镜像：`nebula/vllm:0.23.0-lmcache0.5.5-probe`（= `vllm/vllm-openai:v0.23.0` + `pip install lmcache==0.5.5`）。**这是探针镜像，不是 `NEBULA_APPROVED_PD_IMAGES` 门禁产物**，数字用于工程判断，不能当发版口径。
> LMCache 门禁：`check_lmcache_compat.sh --full` **8/8 PASS**（含 `Retrieved 512 out of 512 required tokens` 跨引擎 store→retrieve）。
> 路由：standalone `nebula-router`，`--routing-strategy prefix_affinity`；负载 `scripts/verify_bench.py`，192 请求 / C=8 / `max_tokens=32`，SLO 阈值 TTFT 2s / e2e 5s。
> 原始样本与每轮 `/metrics` 快照保留在测试机 `/tmp/ab/`。

### 结果（同机同会话，192 请求 @ C=8，高复用 96×1500 词）

| arm | rps | goodput | TTFT p50 | TTFT p99 | e2e p50 | 失败 |
|-----|-----|---------|----------|----------|---------|------|
| replicated（prefix_affinity + 引擎 kv-events） | 9.05 | 9.05 | **131 ms** | 1163 ms | 540 ms | 0 |
| pd_split，**共享存储为空**（首轮冷前缀） | 1.07 | 1.01 | 1073 ms | 2646 ms | 2631 ms | **15** |
| pd_split，**共享存储已预热**（同一栈连跑） | **10.83** | 10.83 | 224 ms | **449 ms** | — | **0** |

### 结论（已修正，见下）

- **吞吐**：`pd_split` 预热后与 `replicated` 持平甚至略高（10.83 vs 9.05 rps）。**不存在"8.6× 变慢"**。
- **延迟形态正是 `pool.md` 原本的表述**：p50 更差（224 vs 131 ms，暖请求仍要走一次 `lm://` 取回），**p99 明显更好（449 vs 1163 ms，−61%）** —— 尾部被 Prefill/Decode 分池隔离掉。
- **`pd_split` 的真实风险是"冷存储 + 单 Prefill 池"的突发**：首轮 96 个前缀从零开始，全部压到单个 Prefill 副本，C=8 下队列积压 → 1.07 rps、15/192 流未收到 `[DONE]`（客户端看到截断的 200）。同一栈预热后重测，**0/192 失败**。

### 先前的错误结论与原因（保留记录）

本节初版（PR #38）写的是"`pd_split` 暖态仍慢 8.6 倍，因为每次整请求取回 1536 token 的 KV 比重新 prefill 还贵"。**该结论是错的**，原因是那一轮把"共享存储为空的第一轮"误当成"暖态"：96 个前缀首次遇到时全部是冷请求，`pd_split` 会把它们串行压到唯一的 Prefill 副本，测到的是**冷启动突发 + 队列积压**，不是稳态暖请求。补齐 C 扫描 + 预热后重测才暴露出差别（同一 C=8、同一 192 请求：1.07 → 10.83 rps，15 → 0 失败）。

取回成本的量级仍然成立（`Retrieved 1536/1536 tokens`，约 0.6 ms/token，与 p50 差 93 ms 同向），但它**不足以让 PD 分离变成负收益**，只解释了 p50 的劣化。

### 有效性证据

- replicated 臂：`prefix_lookup_total=192`、`prefix_index_entries=11652`、`strategy_fallback_total{reason="no_signal"}=97`（96 个前缀首次为冷 + 1 次预热）。
- pd_split 臂：`pd_lookup_total=192`、`pd_lookup_warm_total=46`（首轮）/ 220（预热后累计），decode 引擎日志 `Retrieved 1536 out of 1536 required tokens`。
- 截断流：`nebula_router_upstream_stream_error_total=0` —— 流是以"上游正常 EOF 但无 `[DONE]`"结束的，不是 router 侧错误；该计数器由同批 instrumentation 新增（见 `handlers.rs` 的 `Ok(Some(Err(e)))` 分支）。

### 未覆盖

- C>16、长上下文（≥8k prompt）、多 Prefill 池配比（1:2 / 2:1）；
- `/stats/` 采集管道存在时的短请求旁路（本轮未跑 nebula-node，`pd_short_bypass_total=0`，冷短请求臂因此只用了单个 GPU：14.21 vs replicated 18.39 rps）；
- `RUN_SMOKE=1` 的 CI 级回归。
