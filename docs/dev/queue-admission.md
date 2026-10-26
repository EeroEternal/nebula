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
| 2 | **租户等待份额** | 实测实现（`queue.rs`）：**信号量并发 + 等待者总量有界 + per-tenant 等待份额上限（默认 ≤50%）**，单租户最多占满其份额、超出的等待者被拒（`tenant_queue_full`）。⚠️ 不是按租户轮询出队——先到先得在份额内生效 |
| 3 | **三重上限** | `MAX_SIZE`（全局有界）、**per-tenant ≤ 50%**、`MAX_WAIT_MS`（超时 429） |
| 4 | **TTFT 组成分离（关键）** | 响应头 `x-nebula-queue-wait-ms` + `x-nebula-upstream-ttft-ms`；metrics 分开统计。避免「排队等待」被压测误算成「网关/模型慢」 |
| 5 | **静态上限优先** | 首版纯静态；若日后要「自适应」，**只允许用 429/5xx**（绝不用 TTFT） |
| 6 | **Per-model ETA**（可选） | EMA 时长，做排队预估 |
| 7 | **可观测** | `queue_depth` / `queue_wait_ms` / `admitted` / `queued` / `rejected{queue_full,tenant_queue_full}` / `upstream_ttft_ms` |

**生命周期约束**：准入 guard 必须覆盖**整个流式响应**（从入队许可到 SSE 流结束）。`EnginePermit` 一开始就移进了 SSE 转发任务；`TenantAdmission` 的 `ConcurrencyGuard` 以前在 handler 返回时就释放（流式请求只占住 TTFT 那一段），现已同样移进转发任务，见 §7.2。

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

## 7. 非目标与交付边界（2026-10-12 按评估 A3 明确）

- ❌ **AIMD / 任何用 TTFT-成功率调并发的控制器**（自锁振荡）。
- ✅ **租户配额的集群级硬上限**已按 [`ha.md`](../arch/ha.md) §4.1 首版落地：确定性分片（见下）。
- ❌ **跨网关全局引擎队列**：`EngineQueues` 的在途并发/等待者计数仍是进程级，N 个 Gateway 仍是 N 份引擎保护额度。
- ❌ 改引擎（vLLM）。

### 7.1 租户配额：确定性分片（issue #68，2026-10-26）

两种模式，默认走注册表：

- **不设** `NEBULA_GATEWAY_QUOTA_MEMBERS`（默认）：每个 Gateway 注册 `/gateways/{instance_id}`（lease TTL 8s / 刷新 1s，`NEBULA_GATEWAY_MEMBER_TTL_MS` / `..._REFRESH_MS` 可调），按**实时成员数**分片：join/leave 自动重分片，故障副本的分片由存活副本接管，按排位分配余数（Σ 分片 == 限额）。
- **设了** `NEBULA_GATEWAY_QUOTA_MEMBERS=N`：固定分母，不注册（隔离网络 / 固定拓扑 / 不想让 Gateway 写 etcd 时）。

视图超过 `NEBULA_GATEWAY_QUOTA_VIEW_STALE_MS`（默认 3×refresher）未更新即视为不可信：`NEBULA_GATEWAY_QUOTA_DEGRADED=fail_closed`（默认，`503 tenant_quota_unavailable`）或 `fail_open`（按全局限额服务并计入 `nebula_gateway_quota_degraded_total`）。

固定分母模式的算术：

```text
租户 rps=10, max_concurrency=5, members=2
  → 每个 Gateway 实际执行 rps=5, concurrency=2
  → 集群总上限 5+5=10 ≤ 10、2+2=4 ≤ 5（不会出现 20 / 10）
```

实现：`nebula_common::quota_shard`（纯函数 + 单测：Σ 分片 ≤ 全局、`allowed_models` 不被切分、未设限额保持未设）。准入路径零 etcd/PG I/O。

**实测**（`scripts/ha_quota_drill.sh`：真 etcd + 2 个真实 Gateway 进程，10/min 租户配额）：`members=1` 时两副本共放行 20 次（复现 N×），`members=2` 时 5+5=10 次、其余 10 次 `429 tenant_rps_exceeded`；流式请求全程占用并发槽，流结束才释放。证据与未覆盖项见 [`ha/quota-drill.md`](ha/quota-drill.md)。

**多机复测**（3 台 Railway Sandbox VM：3 节点 etcd + 2 台 Gateway + 1 台 mock 引擎）：T1/T2 数字同上（20 vs 10），并覆盖 `limit < members`、kill etcd 单节点、kill Gateway 后重启。演练中发现并修掉一个真缺陷——死 etcd 节点会让逐请求的租户读取失败并**静默放行**（配额被绕过），现在失败重试一次、仍失败才放行且计入 `nebula_gateway_tenant_lookup_failed_total`。详见 [`ha/railway-drill.md`](ha/railway-drill.md)。

已知边界（都是刻意的）：

- **成员数是部署不变量，不是实时成员视图**。扩缩 Gateway 必须同步改 `NEBULA_GATEWAY_QUOTA_MEMBERS`；改小了会真的超卖。自动成员注册 + 自动重分片属 HA-P2 后续（需 `/gateways/` lease 注册表）。
- **`limit < N` 时该租户被拒（分片=0）**：一个槽位无法切开，严格优先于可用性。
- **整数余数丢弃**：`limit=5, N=2` 时全集群只能用 4。
- 单个 Gateway 故障时剩下的副本仍只用自己的分片，可用量掉到 `(N-1)/N`；这就是 [`ha.md`](../arch/ha.md) §4.1 要求的「预留故障接管余量」，不会自动再分配。
- 引擎级队列额度**也会**按成员数分片：`EngineQueues` 跟随同一成员视图，`cluster_cap = QUEUE_MAX_CONCURRENCY / members`，成员变化自动调整（无需重启）。代价是 LB 分发不均时能力不可转移（见 `ha/railway-drill-2.md` §4）。

### 7.2 并发额度释放时机

租户并发 `ConcurrencyGuard` 现在**跟着响应体一起活**（三条入口都把它移进 SSE 转发任务）：正常结束、`[DONE]` 后断开、客户端 abort、上游异常 EOF 都在流结束时释放；非流式在响应体构造完成后释放。修复前它在 handler 返回时就释放，流式请求只占住 TTFT 那一段。

## 8. 参考

- 实测：[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §1（引擎天花板与甜点区）、§2（+19% P99 的归因演变）。
- 反模式与替代方案：xrouter `7e9080ac`（删 AIMD）、`89cc9d47`（TTFT 头分离 + fast-fail）、`b11a1523`（区分 tenant/global queue full）、`docs/manual/queue.md`（有界公平队列 + 短排队 + per-model ETA）。

## 9. 真机验证（2026-09-21，8×5090）

单引擎；客户端 = 独立 bench Pod（带 `x-nebula-request-id`，抓 `queue-wait`/`upstream-ttft` 响应头）。

### 9.1 C=64：队列保护引擎（关键证据）

| | 成功 | client TTFT p50/p99 | queue_wait p50/p99 | **upstream_ttft（引擎）p50/p99** |
|---|------|---------------------|--------------------|-----------------------------------|
| **OFF**（无队列） | 256 | 320 / 360 ms | 0 | **257 / 311 ms** |
| **FAIR**（mc=16, wait=2s） | 256 | 439 / 526 ms | 319 / 379 ms | **61 / 82 ms** |

→ 队列把**引擎侧 TTFT p99 从 311ms 压到 82ms（−74%）**；代价是客户端多了一段**可观测的排队等待**（`x-nebula-queue-wait-ms`）。这正是「排队等待」与「引擎慢」必须分开的原因。

### 9.2 C=256 过载：三种模式的取舍

| 模式 | 成功 | 429 | Mean TTFT | P99 TTFT |
|------|------|-----|-----------|----------|
| **OFF** | 512 | 0 | 1252 ms | 3030 ms |
| **FAIR**（mc=16, wait=2s） | 178 | 334 | 1582 ms | 2213 ms |
| **FAST_FAIL**（mc=16） | 18 | 494 | 503 ms | 618 ms |

→ OFF：**引擎被打垮**（P99 3s，但全部“成功”）；FAIR：引擎受保护，超额请求排队 / 超时 429；FAST_FAIL：立即 429，被服务的请求延迟最低。

**结论**：有界公平队列**不降低客户端延迟**——它做的是**保护引擎**（upstream TTFT 大幅下降）+ **把过载显式化**（429 / 有界等待）。这正是 P0-#2 的目标：**别过载**，而不是更快。
