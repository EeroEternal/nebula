# 引擎排队准入（Gateway Queue Admission）

> 日期：2026-09-21。定位：**P0-#2 设计**——把每引擎并发钳在甜点区，过载时保护 P99。
> 依据：[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §1（C=8 81ms → C=256 2464ms）；
> 反模式参考：xrouter `refactor(pool): remove AIMD adaptive concurrency entirely`（v4.2.0，`7e9080ac`）。

## 1. 问题

引擎 P99 TTFT 随并发爆炸（[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §1）：

| 并发 | 8 | 16 | 32 | 256 |
|------|---|----|----|-----|
| P99 TTFT | 81 ms | 146 ms | 370 ms | **2464 ms** |

gateway 当前只有**租户级静态配额**（`crates/nebula-common/src/admission.rs` 的 `TenantAdmission`：RPS / `max_concurrency` / token budget），**没有引擎级在途并发闸门**——高并发直接灌进引擎排队。

## 2. 为什么不做 AIMD

xrouter 的教训（commit 原文）：

> 网关首先是 **pass-through proxy**；**AIMD 把「排队引发的 TTFT 升高」误判为「上游拥塞」**，在突发负载下形成 **自锁振荡**（TTFT **0.5s → 6.6s p50 @ 10k VUs**）。已整体移除。

我们的引擎 TTFT **天然随负载上升**——任何用 TTFT / 成功率反推并发的控制器都会被误导、形成自锁。→ **P0-#2 不做 AIMD。**

## 3. 设计：有界公平队列

| # | 组件 | 说明 |
|---|------|------|
| 1 | **有界公平队列**（per-model，后续 per-endpoint） | 并发 permit 上限 = 引擎甜点区（默认 32）；满则**入队**（`fair`，默认）或**立即 429**（`fast_fail`） |
| 2 | **公平调度** | 按 tenant/key 轮询出队，防单租户霸占 |
| 3 | **三重上限** | `MAX_SIZE`（全局有界）、**per-tenant ≤ 50%**、`MAX_WAIT_MS`（超时 429） |
| 4 | **TTFT 组成分离（关键）** | 响应头 `x-nebula-queue-wait-ms` + `x-nebula-upstream-ttft-ms`；metrics 分开统计。避免「排队等待」被压测误算成「网关/模型慢」 |
| 5 | **静态上限优先** | 首版纯静态；若日后要「自适应」，**只允许用 429/5xx**（绝不用 TTFT） |
| 6 | **Per-model ETA**（可选） | EMA 时长，做排队预估 |
| 7 | **可观测** | `queue_depth` / `queue_wait_ms` / `admitted` / `queued` / `rejected{queue_full,tenant_queue_full}` / `upstream_ttft_ms` |

**生命周期约束**：准入 guard 必须覆盖**整个流式响应**（从入队许可到 SSE 流结束）。现有 `TenantAdmission` 的 `ConcurrencyGuard` 在 handler 返回时释放，SSE 尚未流完——P0-#2 需把 guard 移进 SSE 转发任务，否则计数不准。

**与现状关系**：`TenantAdmission`（租户 RPS/并发/token）**保留**；新增「引擎级有界队列」（model 维度），在**路由之后**生效。

## 4. 配置（默认 off / fair，保持兼容）

```text
NEBULA_GATEWAY_QUEUE_MODE=fair|fast_fail      # 默认 fair
NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY=32        # 引擎甜点区上限
NEBULA_GATEWAY_QUEUE_MAX_SIZE=500
NEBULA_GATEWAY_QUEUE_MAX_WAIT_MS=2000          # 0 = fast-fail
NEBULA_GATEWAY_QUEUE_TENANT_SHARE=0.5
```

## 5. 失败码

区分并可观测：`queue_full`（全局队列满）与 `tenant_queue_full`（单租户队列超份额）——xrouter 的 `b11a1523` 同样做了区分。均返回 429 + `Retry-After`。

## 6. 分步落地

1. **有界公平队列 + 静态上限 + 429/超时**（核心，先拿到「P99 有界」）。
2. **TTFT 组成分离头 + metrics**（避免压测误判）。
3. **`fast_fail` 模式**。
4. **per-model ETA**（可选）。
5. **真机 A/B**：过载（C=256）下 P99 被保护、且**无自锁振荡**（对比 `off`）。

## 7. 非目标

- ❌ **AIMD / 任何用 TTFT-成功率调并发的控制器**（自锁振荡）。
- ❌ 跨网关全局队列（后续）。
- ❌ 改引擎（vLLM）。

## 8. 参考

- 实测：[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §1（引擎天花板与甜点区）、§2（+19% P99 的归因演变）。
- 反模式与替代方案：xrouter `7e9080ac`（删 AIMD）、`89cc9d47`（TTFT 头分离 + fast-fail）、`b11a1523`（区分 tenant/global queue full）、`docs/manual/queue.md`（有界公平队列 + 短排队 + per-model ETA）。
