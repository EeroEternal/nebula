# 多引擎对齐（vLLM / SGLang）

> 日期：2026-09-21。定位：让 **node（裸机）** 与 **k8s-controller（集群）** 两条执行面对多引擎行为一致。

## 1. 目标

Nebula 引擎无关。两条执行面都要能按 `ModelSpec.engine_type` 起 **vLLM / SGLang**，并把能力、指标、健康统一到同一控制面。

## 2. 实现

- **`nebula_common::engine_metrics`**：`parse_vllm_metrics_text` / `parse_sglang_metrics_text` + **`parse_engine_metrics(engine_type, …)`** 分派；node 复用（不再各自一份实现）。
- **`nebula-k8s-controller`**：`create_engine_pod(engine_type)` 选镜像与命令：
  - **vLLM**：`vllm/vllm-openai:latest`（`vllm serve` 入口，`--model /models/weights …`）；
  - **SGLang**：`lmsysorg/sglang:latest`，`python3 -m sglang.launch_server --model-path /models/weights --served-model-name … --host 0.0.0.0 --port 43537 --trust-remote-code --enable-metrics`；
  - `EndpointInfo.engine_type` 取 `spec.engine_type`；`/stats/` 走 `parse_engine_metrics` 分派。

## 3. 真机验证（2026-09-21，8×5090）

以 `engine_type = sglang` 起 Pod：

| 项 | 结果 |
|----|------|
| Pod 镜像 | `lmsysorg/sglang:latest` ✅ |
| endpoint `/endpoints/qwen2.5-7b-nebula/0` | `engine_type: "sglang"`、`status: ready`、**持续刷新** ✅ |
| `/stats/qwen2.5-7b-nebula/0` | 由 **SGLang 解析器**产出（`kv_cache_usage` ← `token_usage`；无 `prefix_cache_hit_rate`）✅ |
| 端到端 | `curl <gateway>/v1/chat/completions` → SGLang 正常回答 ✅ |

## 4. 真机踩到的坑（已修）

1. **SGLang 默认绑 `127.0.0.1`**（vLLM 是 `0.0.0.0`）→ pod IP 不可达 → 健康探测失败 → endpoint 不注册。**必须 `--host 0.0.0.0`**。
2. **SGLang `/health` 响应 >800ms**（`curl -m 0.8` 5/5 超时，`-m 5` 才 200）→ controller 原 **800ms** 就绪探测失败 → endpoint 靠 TTL 过期、永不刷新。**已调到 3s**。
3. SGLang `/metrics` 需 **`--enable-metrics`**，否则 404（`/stats/` 抓不到）。

## 5. 参考

- 能力/契约：[`contracts.md`](contracts.md)（C4 Capability 三态、C6「引擎差在 scrape」）。
- 执行面边界：[`k8s.md`](k8s.md)。
