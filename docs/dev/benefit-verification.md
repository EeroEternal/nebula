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

## 2026-10-18 复测：`pd_split` vs `replicated`（LMCache 0.5.5 真机，含暖信号有效性证据）

> 环境：**101**（8×RTX 5090 / 32GB），Qwen2.5-7B-Instruct（bf16，TP=1），vLLM `v0.23.0` + **LMCache 0.5.5**，仅用 GPU 0/1。
> 镜像：`nebula/vllm:0.23.0-lmcache0.5.5-probe`（= `vllm/vllm-openai:v0.23.0` + `pip install lmcache==0.5.5`）。**这是探针镜像，不是 `NEBULA_APPROVED_PD_IMAGES` 门禁产物**，数字用于工程判断，不能当作发版口径。
> LMCache 门禁：`check_lmcache_compat.sh --full` **8/8 PASS**（含 `Retrieved 512 out of 512 required tokens` 跨引擎 store→retrieve）。
> 路由：standalone `nebula-router`，`--routing-strategy prefix_affinity`；负载 `scripts/verify_bench.py`，C=8，每轮 192 请求，`max_tokens=32`，SLO 阈值 TTFT 2s / e2e 5s。
> 工具：[`scripts/verify_bench.py`](../../scripts/verify_bench.py)；原始样本与每轮 `/metrics` 快照保留在测试机 `/tmp/ab/`。

### 结果

| arm | 负载 | rps | goodput | TTFT p50 | TTFT p99 | e2e p50 | 失败 |
|-----|------|-----|---------|----------|----------|---------|------|
| **replicated**（prefix_affinity + 引擎 kv-events） | 高复用 96×1500 词 | **9.17** | **9.17** | **155 ms** | **1164 ms** | 572 ms | 0 |
| **pd_split**（controller 判暖生效） | 同上 | **1.07** | 1.01 | **1073 ms** | 2646 ms | 2631 ms | **15** |
| replicated | 冷短请求 192×32 词 | **18.39** | 18.39 | 91 ms | 326 ms | 433 ms | 0 |
| pd_split | 同上 | 14.21 | 14.21 | 139 ms | 363 ms | 553 ms | 0 |

### 有效性证据（否则这些数字不成立）

- replicated 臂：`nebula_router_prefix_lookup_total=192`、`nebula_router_prefix_index_entries=11652`（kv-events 真的在流）、`strategy_fallback_total{reason="no_signal"}=97`（正好 96 个前缀首次为冷 + 1 次预热）。
- pd_split 臂：`pd_lookup_total=192`、**`pd_lookup_warm_total=46`**、decode 引擎日志 `Retrieved 1536 out of 1536 required tokens` —— 暖信号与跨机检索都确实生效。
- 冷短请求臂：`pd_lookup_total=0`（prompt < `NEBULA_PD_BYPASS_TOKENS=256`，按设计短路），但 **`pd_short_bypass_total=0`** —— 见发现 3。

### 关键发现

1. **`pd_split` v1 在高复用暖态下仍比 `replicated` 慢 8.6 倍（9.17 → 1.07 rps），TTFT p50 155 → 1073 ms。**
   暖请求确实命中并取回了 KV（1536/1536 tokens），但 v1 是**整请求级**路由：每个"暖"请求仍要经 `lm://` TCP 把 1536 个 token 的 KV 拉一遍（约 0.6 ms/token），而在 5090 上直接重新 prefill 这 1536 token 只需 ~150 ms。**当 TCP 取回比重新计算还贵时，PD 分离是负收益** —— 这与 [`../arch/pool.md`](../arch/pool.md) 早前"1:1 pd_split 冷启 3.15 vs replicated 3.72 rps"的方向一致，本次把**暖态**也量化了。结论不变：`pd_split` v1 不是通用吞吐加速手段。
2. **`pd_split` 暖态下 7.3%（14/192）的流被截断**：`502 SSE stream ended without [DONE]`，另 1 例客户端 60s 超时。检索在负载下卡住 → 流在 `[DONE]` 前断开。这是可靠性问题（不止是慢），[#37](https://github.com/EeroEternal/nebula/issues/37) 记录复现与证据。
3. **短请求旁路依赖 `/stats/` 采集，缺失时静默退化为"全部走 Prefill"**：本轮仅手工种入 `/endpoints/`、未运行 `nebula-node`/`k8s-controller` 的 scrape 管道，Decode 副本没有新鲜 stats → `pd_short_bypass_total=0`，冷短请求臂的 14.21 rps 实际只用了**一个** GPU（vs replicated 的两个）。旁路的前置条件应在运维文档中显式写出。

### 与旧结论的差异说明

旧章节（2026-10-12）的 `pd_split` 暖态数字是"尾部延迟改善、吞吐 −5%"；本次不再复现该改善。差异来源：本次是 Qwen2.5-7B + 单层 CPU tier 的 `lm://` 远端（无本地 KV 直连），暖请求**每次都要完整 retrieve**；旧测的"暖态 8 前缀"场景命中了引擎侧 prefix cache，走了不同的取回路径。两者不是同一口径，不应相互引用。

### 未覆盖（本次刻意没测）

- 并发 >8 与长上下文（≥8k prompt）下的表现；
- `NEBULA_PD_BYPASS_TOKENS` 生效所需的完整 `/stats/` 管道（需跑 nebula-node）；
- `RUN_SMOKE=1` 的 CI 级回归（截断流目前只有人工发现，没有门禁拦截）。
