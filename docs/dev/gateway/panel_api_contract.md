# Gateway 观测面板 API 契约（BFF ↔ Frontend）

> 目标：定义前端观测面板的最小 API 契约，屏蔽底层指标源（Prometheus/xtrace）差异。

## 1. 设计原则

1. 前端只调用 BFF 聚合接口，不直接拼 PromQL。
2. 返回结构稳定，图表可复用同一时序数据模型。
3. 指标源可替换（Prometheus/xtrace），前端协议不变。

---

## 2. 通用约定

## 2.1 时间窗参数

- `window`: `5m | 15m | 1h | 6h | 24h`
- `step`: 可选，单位秒；缺省由后端按 window 决定

## 2.2 时序点结构

```json
{
  "ts": "2026-02-16T12:00:00Z",
  "value": 12.34
}
```

## 2.3 错误结构

```json
{
  "error": {
    "code": "OBS_QUERY_FAILED",
    "message": "failed to query metrics backend"
  }
}
```

---

## 3. API 列表

## 3.1 概览卡片

`GET /api/v2/observability/gateway/overview?window=15m`

响应：

```json
{
  "window": "15m",
  "rps": 123.4,
  "error_5xx_ratio": 0.012,
  "retry_success_ratio": 0.41,
  "circuit_open_count": 7
}
```

字段说明：
- `rps`: 当前窗口平均请求速率
- `error_5xx_ratio`: `5xx / total`
- `retry_success_ratio`: `retry_success / retry_total`（无重试时返回 `0`）
- `circuit_open_count`: 窗口内熔断开启次数

## 3.2 请求与状态码趋势

`GET /api/v2/observability/gateway/traffic?window=1h`

响应：

```json
{
  "window": "1h",
  "series": {
    "requests_total": [{"ts":"2026-02-16T12:00:00Z","value":100}],
    "responses_2xx": [{"ts":"2026-02-16T12:00:00Z","value":95}],
    "responses_4xx": [{"ts":"2026-02-16T12:00:00Z","value":3}],
    "responses_5xx": [{"ts":"2026-02-16T12:00:00Z","value":2}]
  }
}
```

## 3.3 重试与上游错误趋势

`GET /api/v2/observability/gateway/reliability?window=1h`

响应：

```json
{
  "window": "1h",
  "series": {
    "retry_total": [{"ts":"2026-02-16T12:00:00Z","value":8}],
    "retry_success_total": [{"ts":"2026-02-16T12:00:00Z","value":3}],
    "upstream_error_connect": [{"ts":"2026-02-16T12:00:00Z","value":2}],
    "upstream_error_timeout": [{"ts":"2026-02-16T12:00:00Z","value":4}],
    "upstream_error_5xx": [{"ts":"2026-02-16T12:00:00Z","value":6}],
    "upstream_error_other": [{"ts":"2026-02-16T12:00:00Z","value":1}]
  }
}
```

## 3.4 延迟与 TTFT

`GET /api/v2/observability/gateway/latency?window=1h`

响应：

```json
{
  "window": "1h",
  "series": {
    "latency_p50_ms": [{"ts":"2026-02-16T12:00:00Z","value":42}],
    "latency_p95_ms": [{"ts":"2026-02-16T12:00:00Z","value":280}],
    "latency_p99_ms": [{"ts":"2026-02-16T12:00:00Z","value":560}],
    "ttft_p50_ms": [{"ts":"2026-02-16T12:00:00Z","value":65}],
    "ttft_p95_ms": [{"ts":"2026-02-16T12:00:00Z","value":350}]
  }
}
```

## 3.5 防护命中统计

`GET /api/v2/observability/gateway/protection?window=15m`

响应：

```json
{
  "window": "15m",
  "request_too_large_count": 12,
  "circuit_skipped_count": 44,
  "circuit_open_count": 6
}
```

---

## 4. 指标映射（后端实现参考）

## 4.1 Router

- `nebula_router_requests_total`
- `nebula_router_responses_2xx|4xx|5xx`
- `nebula_router_retry_total`
- `nebula_router_retry_success_total`
- `nebula_router_upstream_error_total{kind}`
- `nebula_router_request_too_large_total`
- `nebula_router_route_circuit_skipped_total`
- `nebula_router_circuit_open_total`
- `nebula_route_latency_seconds`
- `nebula_route_ttft_seconds`

## 4.2 Gateway

- `nebula_gateway_requests_total`
- `nebula_gateway_responses_2xx|4xx|5xx`
- `nebula_gateway_upstream_error_total{kind}`
- `nebula_gateway_request_too_large_total`

---

## 5. 前端展示最小要求

1. 支持 `window` 切换（5m/15m/1h/6h/24h）。
2. 所有趋势图共享时间轴。
3. 概览卡片与趋势图数值口径一致。
4. 查询失败时可展示“局部降级”而非整页失败。
