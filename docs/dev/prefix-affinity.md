# P1：多副本与前缀亲和

> 日期：2026-09-21。定位：P1 进展与实测。依据：[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md)（单实例天花板）、[`landscape.md`](landscape.md)（Dynamo/llm-d 的 KV 感知路由）。

## 1. 目标

单实例引擎天花板 ~3.4k tok/s。P1 要：**多副本**提升聚合吞吐 + **前缀亲和**让共享前缀的请求命中同一副本的 KV cache，降低 TTFT。这是唯一能「超过单实例直连」的方向。

## 2. 已具备（P1a）

| 能力 | 状态 |
|------|------|
| **多副本**：`k8s-controller` 按 `dep.replicas` 建 N 个 pod（replica 0 保留 `nebula-{model}`，其余 `-{replica_id}`），各绑不同 GPU（`replica_specs[i].gpu_indices[0]` 或 replica 索引），注册 `/endpoints/{model}/{replica}` | ✅ 真机验证：2 副本、2 endpoint |
| **`/stats/` 采集**：k8s 执行面下无 `nebula-node`，controller 直接抓每个副本的 `/metrics` 写 `/stats/{model}/{replica}`（pending / kv_cache_usage / prefix_cache_hit_rate） | ✅ 真机验证 |
| **指标解析器共享**：`nebula_common::engine_metrics`（node 与 controller 共用） | ✅ |

## 3. 实测：启发式前缀亲和**无效**

两副本、共享前缀 workload（`prefix_repetition` 数据集，prefix 2048 / suffix 128 / 4 prefixes）、C=16、N=256、交错对比 `least_pending` vs `prefix_cache_aware`：

| 轮 | least_pending (req/s, Mean, P99) | prefix_cache_aware |
|----|----------------------------------|--------------------|
| 1 | 31.11 / 107.68 / 261.97 | 31.04 / 112.57 / 273.60 |
| 2 | 30.97 / 112.65 / 326.41 | 30.87 / 110.45 / 259.38 |

→ **两组在噪声内相同。**

**根因**：`PrefixCacheAware` 选的是「**聚合** `prefix_cache_hit_rate` 最高」的副本，**不做按请求前缀匹配**。两副本命中率相近（0.921 vs 0.929）时，该策略退化为「选较高那个」或在无数据时回落到 `LeastPending`。

> ⚠️ **方法教训**：首轮测量曾得到「prefix_aware 吞吐 +9%、P99 −57%」，**那是缓存预热的假象**（第二组跑在后、cache 已暖）。必须**交错**、并修好信号后才能下结论。

## 4. 顺带修复的真 bug

`engine_metrics::extract_metric` 用子串匹配：找 `prefix_cache_hits_total` 时会命中 `external_prefix_cache_hits_total`（后者排在后面、值为 0），导致 `prefix_cache_hit_rate` **恒为 None** → `PrefixCacheAware` 无信号、静默回落 `LeastPending`。已改为**精确匹配完整指标名**并加回归测试（`4b64a3f`）。

## 5. 下一步（P1c）：KV-event 驱动的真前缀亲和

启发式不够。要真做前缀亲和，需**按请求前缀匹配**：

1. 订阅 vLLM **KV cache events**（block hash 的 `BlockStored`/`BlockRemoved`，见 vLLM `distributed/kv_events.py`，通过 `--kv-events-config` 发布）；
2. 维护 `前缀hash → {endpoint}` 索引；
3. 路由时对请求前缀做 hash（`kv-hashing` 式），命中索引则定向到持有该前缀的副本，否则走 `LeastPending`。

> 这是 Dynamo `kv-router`/`kv-hashing` 与 llm-d EPP 的做法 [`landscape.md`](landscape.md) §3。
