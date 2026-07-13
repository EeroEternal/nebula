# SLO / 告警 Runbook（O8）

> 设计初值来自 [`observability.md`](./observability.md) §5.3。  
> Prometheus 规则样例：[`../../deploy/observe/prometheus-alerts.yml`](../../deploy/observe/prometheus-alerts.yml)。  
> 控制台 Gateway 面板数据源当前为 **Router**（`data_source: "router"`）。

## 1. 目标

给运维一套可执行的服务目标与告警处置路径：能判断「是否违约、错误预算是否该扣、从哪一层查起」，且 **abort / 主动 Drain 不计入 5xx 错误预算**。

本 runbook 覆盖接入面（Gateway / Router）与引擎 scrape 健康。模型级 TTFT/TPOT SLO 对象与成本闭环属 Product P4，此处只给可落地的初值与 PromQL。

## 2. SLI 口径

| SLI | 定义 | 主数据源 | 单位 |
|-----|------|----------|------|
| 请求成功率 | `1 - responses_5xx / requests` | Router `/metrics` | 比例 |
| 5xx 错误率 | `responses_5xx / requests` | 同上 | 比例 |
| 重试成功率 | `retry_success_total / retry_total` | 同上 | 比例 |
| 熔断开启 | `increase(circuit_open_total[window])` | 同上 | 次数 |
| 上游 timeout 占比 | `timeout / sum(upstream_error_total)` | 同上 `kind` | 比例 |
| TTFT / 端到端延迟 | `nebula_route_ttft_seconds` / `nebula_route_latency_seconds` | Router histogram | 秒（面板常转 ms） |
| Abort 率（旁路） | `requests_aborted_total / requests` | Gateway + Router | 比例；**不进错误预算** |
| 引擎 scrape 健康 | `nebula_node_engine_scrape_result` | Node `/metrics` | 最新结果标签 |

### 2.1 错误预算（强制）

计入失败（扣错误预算）：

- `nebula_router_responses_5xx` / `nebula_gateway_responses_5xx`
- `nebula_router_upstream_error_total{kind="upstream_5xx|connect|timeout|other"}`（诊断用，预算默认以 `responses_5xx` 为准）

**不计入**错误预算：

- `nebula_router_requests_aborted_total` / `nebula_gateway_requests_aborted_total`（客户端断开 / abort）
- 主动 Drain：副本进入 `Draining` 后被 Router 剔除，期间正常收尾的请求不算 5xx；勿把 Drain 相关 warn 日志当故障预算

Prometheus 上 abort 是独立 counter，**不要**写进 `responses_5xx` 比率公式。

### 2.2 分位口径差

- Prometheus：`histogram_quantile`（近似）
- xtrace：真分位（产品内诊断优先）

告警以 Prometheus scrape 为准（客户 VM/Grafana 可复现）；控制台深挖可用 xtrace，但须在工单中注明数据源。

## 3. 初始 SLO（需按流量校准）

| 目标 | 初值 | 窗口 | 说明 |
|------|------|------|------|
| 可用性（非 5xx） | ≥ 99% | 滚动 30 天或按发布周期 | 对应 5xx 率 ≤ 1%；§5.3 告警用更紧的 2%/5m 早发现 |
| 接入面 5xx 尖刺 | 5m 内 5xx 率 ≤ 2% | 告警用 | 见 §4 |
| TTFT p95（参考） | ≤ 2s（小模型）/ 按模型另定 | 15m | 无流量时勿告警 |
| 端到端延迟 p95（参考） | ≤ 30s（视 max_tokens） | 15m | 仅作容量信号，勿与引擎内部调度混淆 |

低流量（`< 1 req/s` 持续）时，比率类告警易误报：规则内用 `and rate(requests)>0.1` 门禁（样例文件已写）。

## 4. 告警阈值（§5.3 → 可执行）

| ID | 条件 | 持续 | 严重度 | 含义 |
|----|------|------|--------|------|
| `NebulaRouterHigh5xx` | 5xx 率 > 2% | 5m | critical | 错误预算快速消耗 |
| `NebulaRouterLowRetrySuccess` | 重试成功率 < 20% 且有重试 | 10m | warning | 重试无效，坏副本或上游系统性故障 |
| `NebulaRouterCircuitOpenSurge` | 5m 熔断次数 > 1h 均值的 3 倍 | 5m | warning | 候选缩减加剧 |
| `NebulaRouterUpstreamTimeoutShare` | timeout 占上游错误 > 40% | 10m | warning | 引擎/网络慢或过载 |
| `NebulaNodeEngineScrapeFailing` | scrape 非 success | 5m | warning | 实时 `/stats/` 可能陈旧 |

规则 YAML：[`deploy/observe/prometheus-alerts.yml`](../../deploy/observe/prometheus-alerts.yml)。

## 5. 处置步骤

### 5.1 High5xx

1. 控制台 Gateway 面板确认 RPS、5xx、上游错误分类（数据源 Router）。
2. 对比 `upstream_error_total{kind}`：`connect` → 引擎挂了或网络；`timeout` → 过载/慢；`upstream_5xx` → 引擎返回 5xx。
3. 查 etcd endpoints 状态与 Node scrape：`nebula_node_engine_scrape_result`、副本 `Ready`/`Unhealthy`/`Draining`。
4. 用同一 `request_id` / `trace_id` 在 Loki（`NEBULA_LOG_FORMAT=json`）与 xtrace 定位第一层失败（Gateway vs Router vs Engine）。
5. 确认是否刚做过 Drain / 扩缩 / 发版；主动 Drain 导致的短窗口勿计入事故错误预算。
6. 缓解：隔离坏副本（Drain）、降并发、回滚镜像/引擎配置。

### 5.2 LowRetrySuccess

1. 看 `retry_total` 是否上升而 `retry_success_total` 停滞。
2. 查熔断：`circuit_open_total`、`route_circuit_skipped_total` 是否已在挡坏副本。
3. 若熔断未开、重试全失败 → 全副本或依赖故障，转 High5xx 全量排查。

### 5.3 CircuitOpenSurge

1. 确认是否预期（滚动升级、批量 Unhealthy）。
2. 非预期则查 Node 健康、KV 过载（`kv_cache_usage`）、GPU。
3. 核对 Router 熔断参数：`NEBULA_ROUTE_CIRCUIT_FAILURE_THRESHOLD`、`NEBULA_ROUTE_CIRCUIT_OPEN_MS`。

### 5.4 UpstreamTimeoutShare

1. 看引擎 pending / KV、Node GPU 利用率。
2. 区分客户端慢消费（abort 上升）与引擎真超时（abort 不高、timeout 高）。
3. 必要时缩容流量或扩副本；勿把 abort 计入 5xx。

### 5.5 EngineScrapeFailing

1. 看 `result`：`unreachable` / `timeout` / `parse_failed`。
2. scrape 失败时 Router 会降级（无 fresh stats）；优先修引擎 `/metrics` 可达性，再谈路由质量。
3. 上游指标改名应导致 fixture 测试失败——改解析或升级兼容表，禁止用 `0` 伪装。

## 6. 演练清单

- [ ] 人为让一副本返回 5xx：告警触发；熔断/重试是否减少坏副本命中。
- [ ] 客户端中途断开：`*_aborted_total` 增加，`responses_5xx` **不**增加。
- [ ] 主动 Drain：副本离开候选；无额外 5xx 尖刺（或仅可解释的短抖动）。
- [ ] 关掉引擎 `/metrics`：出现 scrape 非 success；路由仍可用但 stats 降级。
- [ ] 面板窗口 5m/15m/1h 均可读；工单写明 Prometheus vs xtrace。

## 7. 校准与演进

- 初值偏敏感，上线 1–2 周后按误报率放宽或按 `model_uid` 拆告警。
- 自动治理（扩缩/切流）默认建议模式，见 [`product_plan.md`](./product_plan.md) P4；本 runbook 只要求人可处置。
- 新增 SLI 时同步：指标名、单位、是否进错误预算、PromQL、本文处置节。
