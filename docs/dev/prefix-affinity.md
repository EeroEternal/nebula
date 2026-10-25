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
- **事件外部形式是 int（u64）**：`maybe_convert_block_hash` 默认把内部 32 字节 sha256 转成 `int.from_bytes(h,"big") & (2^64-1)`（即尾 8 字节大端）。Router 索引把两种线上形式（int / bytes）与请求侧 32 字节哈希统一规范到同一 8 字节键空间（碰撞概率 2⁻⁶⁴ 量级，仅影响路由提示质量）。
- 块哈希链式⇒任意一块命中即证明 token 级前缀完全一致；索引按最深链投票选副本。

**真机全链路验证（60001，8×5090，Qwen2.5-7B @ v0.23.0）**：真实 prompt → 真实 tokenizer ids（`[37, 57610, …]`）→ Rust 链式哈希 `2763d1b3…79a6` → 尾 8 字节 = `1015703622044383654` = 引擎 BlockStored 事件值，**精确一致**（已固化为 `kv_hash`/`kv_events` 回归测试）。

### 5.3 索引与请求侧

- `PrefixIndex`：内存 DashMap `(model_uid, hash) → (replica, last_seen)`；TTL 默认 300s（`NEBULA_ROUTER_PREFIX_TTL_MS`），30s 扫一次；可丢可重建，不进 etcd。
- 请求侧：`Router::body_prompt_hashes()` 仅对 `/v1/completions` 的 `prompt` 字段精确分词（tokenizer 从 `NEBULA_ROUTER_TOKENIZER_DIR/<model_name|model_uid>/tokenizer.json` 加载，带缓存）。
- **shortcut**: chat bodies 不 hash（引擎侧 chat template 渲染后 token 序列与裸拼接不一致，硬算必 miss）；等 router 能渲染 chat template 时升级。会话内亲和由既有 session_affinity 覆盖。

### 5.4 策略与可见性

- 新策略 `prefix_affinity`：查索引命中→定向；未命中/无 hash→`least_pending`，**全部计入** `nebula_router_strategy_fallback_total{strategy=,reason="no_signal"|"owner_unavailable"}`（任务 1 的计数器体系直接复用）。
- SGLang 无 KV events publisher（capability gap）：SGLang 模型只能回落，计数器持续可见。

### 5.5 引擎侧开关

- 执行面设 `NEBULA_KV_EVENTS_ENDPOINT`（router 的 SUB 地址）→ node 与 k8s-controller 为 vLLM 副本追加 `--kv-events-config {…}` **并**注入 `PYTHONHASHSEED=$NEBULA_KV_HASH_SEED` + `--prefix-caching-hash-algo sha256_cbor`（见 §5.2，缺一则路由永不命中）。

### 5.6 验收（✅ 真机完成，2026-10-11，8×5090）

**协议**：`/v1/completions`、96 个共享前缀 × 2048 token / suffix 128 token、C=16、N=512、`stream=true` 测 TTFT、**每轮冷启动引擎**（消除预热假象，§3 的教训）、交错 A/B。

| 轮 | 策略 | req/s | TTFT mean | TTFT p50 | TTFT p99 |
|----|------|-------|-----------|----------|----------|
| A3 | least_pending | 13.73 | 568.5 ms | 455.1 ms | 2140 ms |
| B3 | prefix_affinity | **23.86（+74%）** | **308.0（−46%）** | **143.9（−68%）** | 2151 ms |

- B3 期间索引峰值 ~18.8k 条目（`nebula_router_prefix_index_entries` gauge）；no_signal 仅 102/520（冷启动首触期），此后全命中。
- **机制**：亲和让每个前缀的 KV 只驻留一份（least_pending 在两副本各存一份，有效缓存容量减半）→ 高前缀数量下命中率差距放大。这正是 Dynamo/llm-d 的 cache-aware routing 收益来源。
- 过程中修掉的两个真 bug（不看真机不可能发现）：① spec 更新触发 Router DashMap get+remove 同键死锁（整进程冻结）；② **线上引擎实际发 msgpack MAP 形式事件**（`{"type":"BlockStored",...}`），与容器内直接构造 struct 的 tagged-array 形式不同——原 parser 静默丢弃全部事件。两种格式均已支持并各有真实帧回归 fixture。
- **运营要求**（缺一则永不命中）：引擎 `PYTHONHASHSEED` 固定 + `--prefix-caching-hash-algo sha256_cbor`（node/controller 已自动注入）+ router 同 seed（`NEBULA_KV_HASH_SEED`）+ tokenizer 目录（`NEBULA_ROUTER_TOKENIZER_DIR`）。
- **已知边界（2026-10-11 修正）**：router 重启后索引清零，但 **v0.23 在缓存命中（reuse）时也发 BlockStored 事件**——实测重启后 3 个请求内索引从 0 恢复到 9、no_signal 仅 +1，自愈成立（早先"命中不发事件"是 map-form 解析 bug 的误诊）。`replay_endpoint` 重放降级为可选优化（加快恢复窗口）；chat body 已支持 hash（见下）。

### 5.7 Chat 模板哈希（P1c Batch 1.1，✅ 2026-10-11）

- Router 用 minijinja 渲染模型的 `chat_template`（`tokenizer_config.json`，transformers 兼容子集：`tojson`/`loop.first`/空白控制），`add_special_tokens=false` 分词——与引擎 `apply_chat_template(tokenize=True)` **token 级对齐**（真机验证：6 个共享 system prompt 的 chat 请求全部集中到持有副本，no_signal 仅首请求 +1）。
- shortcut：`chat_template_kwargs` 逐请求覆盖与奇异模板扩展 v1 不渲染——不匹配即 miss（可见回落），需要时再升级。
- **带 tools 的模板（2026-10-25 修正，真机 E2E）**：上面的"token 级对齐"当时只在**无 tools** 的 chat 上验过。带 tools 时会走 `{{ tool | tojson }}`（jinja2 语义：`json.dumps` 默认分隔符 + HTML 转义）和 `message.content.startswith('<tool_response>')` 这类 Python 字符串方法，而 minijinja 默认两者都没有 → `chat_ids` 直接返回 `None`，**所有带工具定义的 agent 前缀既不参与亲和也拿不到 Tool-Pause pin**（静默 `no_signal`）。现已在 `prompt_hash.rs` 内置：
  - `tojson` 过滤器 = jinja2 语义（`, ` / `: ` 分隔，`<>&'` 转义成 `\u003c` 等，非 ASCII 走 `\uXXXX`）；
  - `py_str_method` 的 Python 字符串方法子集（`startswith` / `endswith` / `strip` / `split` / `replace` / `find` / `join` 等，遇到未建模的方法会**报错点名**而不是静默 miss）；
  - 键序：Python dict 保插入序，所以本 crate 的 `serde_json` / `minijinja` 均开 `preserve_order`（否则工具 schema 的 key 会被排序，渲染串与引擎不同）。
  - 真机核验：同一 body（1047 tok 不带 tools 的旧口径 / 1182 tok 带 tools）在 router 侧分词得 **1182**，与引擎日志 `Total tokens 1182` 完全一致（此前为 `None`）。
- Chat 负载 A/B（暖引擎、4 前缀、N=256、C=16）：affinity vs least_pending = 36.9 vs 36.6 rps、TTFT mean 211 vs 231 ms（−8.6%）、**p99 342 vs 850 ms（−60%）**——小缓存稳态下收益集中在尾部；大缓存压力场景见 §5.6（+74% rps）。
### 5.8 全局索引服务 `nebula-indexd`（Batch 2，✅ 2026-10-11）

消除 1.4 残余的"单采集 Router"：引擎指向**唯一稳定端点** indexd，indexd 维护权威索引并向所有 Router sink 推送：

- **原始帧逐字转发**（增量新鲜度）+ **周期快照**（`!reset/{model}` + `!snapshot/{model}/{replica}`，复用 map-form 事件编码，Router 端清后重建）——Router 重启不再等 reuse 事件，≤ 快照周期即恢复。
- **热路径零依赖**：Router 只查本地索引副本；indexd 挂 → Router 用存量继续（回落计数器可见），恢复后快照重建。
- 环境变量：`NEBULA_INDEXD_BIND`（引擎 SUB 端点）、`NEBULA_INDEXD_SINKS`（逗号分隔 Router sink）、`NEBULA_INDEXD_SNAPSHOT_SECS`（默认 60）、`NEBULA_ROUTER_PREFIX_TTL_MS`（与 Router 共用）。
- **zeromq 0.4–0.6 的 PUB socket 对 re-bind 对端不重连**（实测三轮），推送方每 30s / 每快照轮**重建 socket**（帧经 channel 缓冲，不丢）。已升级 zeromq 0.6。
- 本地验收 4/4：原始转发 ✓、Router 重启快照自愈 ✓、indexd 宕机降级保有 ✓、双 Router 索引一致 ✓。

## 6. P1 收尾验证（2026-09-21，8×5090）

| 项 | 结果 |
|----|------|
| **多副本** | ✅ 2 副本（各绑 GPU 0/1），2 endpoint + 2 `/stats/` |
| **LeastPending 均衡** | ✅ 256 请求分布 **r0=132 / r1=128（≈50/50）**；吞吐 35.63 req/s、Mean TTFT 93.6ms |
| **缩容清理** | ✅ `replicas 2→1`：replica 1 的 pod / endpoint / stats 全部删除，GPU1 释放 |
| **终止态重建** | 代码修复（`23d9404`）；未真机触发 |

修复：`23d9404` —— 缩容清理（`cleanup_extra_replicas`）、终止态 pod 重建（不再卡住）、非 Running 时清 stale `/endpoints` + `/stats`。

**结论**：**多副本 + LeastPending 已做扎实**（均衡 ≈50/50、扩缩容/生命周期正确）。P1c 代码已落地（见 §5），待真机验收。
