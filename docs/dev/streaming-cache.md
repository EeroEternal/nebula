# 流式可靠性与 KV 缓存治理：当前行为与待办

> 状态：2026-10-18。性质：**现状与边界说明**（不是历史回顾；改动逐条在 `CHANGELOG.md` `[Unreleased]`，设计见 [`kv-governance.md`](kv-governance.md)、[`benefit-verification.md`](benefit-verification.md)、[`../arch/pool.md`](../arch/pool.md)）。

## 1. 一句话现状

流式请求的失败**不会再伪装成正常结束**，`pd_split` 的暖信号与 `/stats/` 缺失**都会出现在指标里**，KV 缓存有了**可声明、可操作、可记账**的最小闭环 —— 但 `pd_split` 在本仓验证过的单机小模型配置下**没有收益**，它的收益条件见 §4。

## 2. 流式请求的行为契约

| 失败时刻 | 现在的行为 | 计数器 |
|---|---|---|
| 副本接了请求但**首字节前**不出字 | 换副本重试；全部失败才 `503 + Retry-After: 1`（客户端从未见过状态码） | `retry_total`、`first_byte_retry_total` |
| **首字节后**中途 stall / 上游错误 | 发一个 OpenAI 形状 error 帧后关闭，**不发** `[DONE]` | `upstream_stream_error_total` |
| 索引/统计信号缺失导致无法决策 | 记"无法决策"而不是静默回落 | `stage_stats_missing_total{decision}` |
| 客户端断开 | 计 abort，不算 5xx | `requests_aborted_total` |

两条路径同一契约：standalone `nebula-router` 与 `NEBULA_ROUTER_MODE=embedded`（计数分别加前缀区分）。
预算旋钮：单次 `NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS`（60s，`0` 关闭该档）、跨尝试 `NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS`（90s）、`NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS`（120s）。

## 3. 运维必须知道的三件事（缺一则功能静默退化）

1. **`lmcache_controller` 必须先于引擎就绪。** 引擎先起会"注册成功但永不上报 KV 键"，`/lookup` 对该引擎恒冷，**只能重启引擎恢复**。产品侧已有门禁（`nebula-node` 启动前 / `k8s-controller` 建 Pod 前等 controller 端口），手工验证要等 controller 端口可连再起引擎。信号：`pd_lookup_warm_total` 恒 0 + `stage_stats_missing_total{decision="warm_signal_cold"}`。
2. **`/stats/` 管道必须通**（`nebula-node` / `k8s-controller` 抓引擎 `/metrics` → `/stats/{model}/{replica}`）。它决定短请求旁路、Prefill 溢流、以及 `least_pending` 在副本间分摊。信号：`stage_stats_missing_total{decision="short_bypass"|"prefill_spillover"}`、`route_stale_stats_dropped_total`。
3. **KV L1 配额要显式声明。** 未声明时每副本仍按 5 GB 预留宿主机内存（历史行为，保持兼容）。节点上报宿主机内存后，容量快照会给出 `kv_l1_oversubscribed`。

## 4. `pd_split` 的收益边界（实测结论）

单机 7B / TP=1 / 1:1 池配比 / 96×1500 词前缀、前置条件齐备（controller 先就绪 + `/stats/` 通）时：**吞吐 −17%、TTFT p50 与 p99 均更差**（12.89 → 10.75 rps、107 → 257 ms、219 → 343 ms）。

因此 `pd_split` 的适用条件收窄为：**取回明显比重算便宜**（更大模型、更长 prefill、本地 tier 命中）或需要拆开 prefill/decode 的故障域与扩缩容。它**不是**"开箱即更快"。数据与两次自我更正见 [`benefit-verification.md`](benefit-verification.md)。

## 5. KV 缓存治理：已落地 / 未落地

| Batch 6 | 已落地 | 未落地 |
|---|---|---|
| 6.1 声明 | `KvSharingSpec.local_cpu_size_gb / local_disk_*` → LMCache env；宿主机内存上报；`kv_l1_oversubscribed` | `replicated` 单池的分级 Offload（`kv_sharing_env(Mixed)` 仍返空） |
| 6.2 隔离 | — | 租户 `cache_salt`（需引擎侧 hash 域配合） |
| 6.3 预热/钉住 | `LmcacheController`（lookup/pin/clear）+ BFF `POST /api/v2/models/{uid}/cache/{lookup,pin,clear,warmup}` | 控制台入口、定时预热、warmup 后自动 pin（缺 token ids） |
| 6.4 记账 | `nebula_router_saved_prefill_tokens_total`（保守口径：被服务副本实际持有块 × 16） | 租户报表、`$` 折算（缺每模型单价输入） |
| 6.5 运维/门禁 | — | lmcache 守护自愈、CacheBlend 仅 Dense 的兼容门禁 |

缓存写入器（哪些副本该声明/预热/清理）在 `kv-governance.md` 的「声明·隔离·预热·记账」四段里定义；本页只说**当前是否可用**。

## 6. 如何复现上面的数字

测试机保留：`nebula/vllm:0.23.0-lmcache0.5.5-probe`（vLLM 0.23.0 + lmcache 0.5.5）、`~/models/Qwen2.5-7B-Instruct`、`~/nebula-tok/qwen/`（router 分词器）、`~/ab3.sh`（A/B：controller 先就绪 + `/stats/` 写入 + C 扫描）、`~/stats_writer.py`（替代 `nebula-node` 的 stats 抓取，**仅 harness 用**）。

顺序：etcd → `lmcache_server` → `lmcache_controller`（等端口）→ 引擎 → stats 写入 → router。任何一步顺序错，`pd_lookup_warm_total` 与 `stage_stats_missing_total` 会立刻告诉你是哪一步。

## 7. 门禁

`./scripts/ci.sh`（含 `RUN_SMOKE=1` 时 20 项 smoke：stalled-stream 契约、router 与 embedded 两种模式的首字节前重试、BFF 控制台 API、cancel SSE）。`RUN_SMOKE=1` 需 docker compose + postgres；若本机 5432 被占用，用 `POSTGRES_PORT=<port> RUN_SMOKE=1 ./scripts/ci.sh`。
