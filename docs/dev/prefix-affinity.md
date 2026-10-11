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

## 5. P1c（已落地代码，待真机验收）：KV-event 驱动的真前缀亲和

启发式不够。要真做前缀亲和，需**按请求前缀匹配**。以下为**已校准并落地**的设计（对照 vLLM v0.11 源码，`kv_events.py` / `kv_cache_utils.py` / `utils/__init__.py`）：

### 5.1 传输：zmq PUB→SUB + msgpack（不是 HTTP）

- vLLM 核心（v0.11 与 main）只内置 `null` 与 `zmq` 两种 publisher，**http publisher 不在核心里**——§5 旧设想作废。
- 拓扑：**Router 绑定 SUB**（`NEBULA_ROUTER_KV_EVENTS_BIND`，如 `tcp://0.0.0.0:5557`），引擎 connect 过来；每副本发布在 topic `{model_uid}/{replica_id}`（由 node / k8s-controller 注入 `--kv-events-config`）。
- 消息为 `(topic, seq, msgpack)` 三帧；msgpack 为 msgspec `array_like+tag` 格式：`[ts, ["BlockStored", block_hashes, parent, token_ids, block_size, lora_id, medium], ...]`。
- PUB 慢订阅者丢消息——索引是缓存，丢只损失提示，不损正确性。

### 5.2 hash：sha256_cbor 链式（需显式固定 seed + algo）

**真机校准（v0.23.0 部署镜像，2026-10）**：

- 块哈希：`sha256(cbor2.dumps((parent_hash, tuple(token_ids), None), canonical=True))`，首块 parent 为 `NONE_HASH`；与 v0.11 逐位一致。
- **v0.23 默认 `--prefix-caching-hash-algo=sha256`（pickle 序列化）**——Rust 无法复刻，引擎必须显式设 `--prefix-caching-hash-algo sha256_cbor`。
- **`PYTHONHASHSEED` 未设时 `NONE_HASH = os.urandom(32)`，每进程随机**——跨副本匹配必然失败。引擎必须固定 `PYTHONHASHSEED`，router 用同值（双方读 `NEBULA_KV_HASH_SEED`，默认 `"42"`；v0.11 未设时默认 `"vllm-none-hash"`）。
- Rust 端口 `kv_hash.rs` 的测试向量：v0.23 部署容器内直接调用 `vllm.utils.hashing` 生成（seed=42：`NONE_HASH=9fa6eb…`、`H0=3f9c79…`、`H1=45e3af…`）；真实 zmq 帧已固化为 `kv_events.rs` 回归测试。
- 块哈希链式⇒任意一块命中即证明 token 级前缀完全一致；索引按最深链投票选副本。

### 5.3 索引与请求侧

- `PrefixIndex`：内存 DashMap `(model_uid, hash) → (replica, last_seen)`；TTL 默认 300s（`NEBULA_ROUTER_PREFIX_TTL_MS`），30s 扫一次；可丢可重建，不进 etcd。
- 请求侧：`Router::body_prompt_hashes()` 仅对 `/v1/completions` 的 `prompt` 字段精确分词（tokenizer 从 `NEBULA_ROUTER_TOKENIZER_DIR/<model_name|model_uid>/tokenizer.json` 加载，带缓存）。
- **shortcut**: chat bodies 不 hash（引擎侧 chat template 渲染后 token 序列与裸拼接不一致，硬算必 miss）；等 router 能渲染 chat template 时升级。会话内亲和由既有 session_affinity 覆盖。

### 5.4 策略与可见性

- 新策略 `prefix_affinity`：查索引命中→定向；未命中/无 hash→`least_pending`，**全部计入** `nebula_router_strategy_fallback_total{strategy=,reason="no_signal"|"owner_unavailable"}`（任务 1 的计数器体系直接复用）。
- SGLang 无 KV events publisher（capability gap）：SGLang 模型只能回落，计数器持续可见。

### 5.5 引擎侧开关

- 执行面设 `NEBULA_KV_EVENTS_ENDPOINT`（router 的 SUB 地址）→ node 与 k8s-controller 为 vLLM 副本追加 `--kv-events-config {…}` **并**注入 `PYTHONHASHSEED=$NEBULA_KV_HASH_SEED` + `--prefix-caching-hash-algo sha256_cbor`（见 §5.2，缺一则路由永不命中）。

### 5.6 验收（待 GPU 修复后真机）

重跑 §3 协议：`/v1/completions`、`prefix_repetition`（prefix 2048 / suffix 128 / 4 prefixes）、2 副本、C=16、N=256、**交错**对比 `least_pending` vs `prefix_affinity`。预期：命中请求跳过 2048 token prefill，TTFT 显著下降；若无效，这次信号/回退全程可观测，结论可信。

**已在无 GPU 环境完成的真机校准**（部署镜像 v0.23.0，runc 容器）：事件 msgpack 帧格式、hash 链算法与 seed 行为均已用真实库调用/真实 zmq 字节流验证并固化为回归测试。GPU e2e 阻塞于宿主机 nvidia 驱动损坏（`nvidia-smi` NVML mismatch、`/run/nvidia-persistenced/socket` 缺失，需管理员修复/重启）。

## 6. P1 收尾验证（2026-09-21，8×5090）

| 项 | 结果 |
|----|------|
| **多副本** | ✅ 2 副本（各绑 GPU 0/1），2 endpoint + 2 `/stats/` |
| **LeastPending 均衡** | ✅ 256 请求分布 **r0=132 / r1=128（≈50/50）**；吞吐 35.63 req/s、Mean TTFT 93.6ms |
| **缩容清理** | ✅ `replicas 2→1`：replica 1 的 pod / endpoint / stats 全部删除，GPU1 释放 |
| **终止态重建** | 代码修复（`23d9404`）；未真机触发 |

修复：`23d9404` —— 缩容清理（`cleanup_extra_replicas`）、终止态 pod 重建（不再卡住）、非 Running 时清 stale `/endpoints` + `/stats`。

**结论**：**多副本 + LeastPending 已做扎实**（均衡 ≈50/50、扩缩容/生命周期正确）。P1c 代码已落地（见 §5），待真机验收。
