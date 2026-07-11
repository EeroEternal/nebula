# Gateway P0 执行 Runbook

> 状态：P0 防护能力（重试、请求体上限、错误分类指标）已基本落地。本文现在作为灰度、回归验证和回滚 runbook 使用。

## 1. 已落地变更范围

- Router：
  - 一次快速重试（仅可重试错误）
  - 重试排除首个失败 endpoint
  - 请求体上限（默认 4MB）
  - 指标：`retry_total` / `retry_success_total` / `upstream_error_total{kind}` / `request_too_large_total`
  - endpoint 短时熔断与熔断指标
  - stale stats 过滤与 route-time freshness 指标
- Gateway：
  - 请求体上限（默认 4MB）
  - 指标：`upstream_error_total{kind}` / `request_too_large_total`
  - 推理/admin 路由共享鉴权 middleware

---

## 2. 发布前检查

1. 代码编译通过：
   - `cargo check --workspace`
2. 确认监控已采集以下路径：
   - `router: /metrics`
   - `gateway: /metrics`
3. 确认当前基线：
   - 近 24h `5xx` 比例
   - P99 延迟
   - 请求量峰值

---

## 3. 配置建议（灰度）

## 3.1 Router

- `NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES=4194304`
- `NEBULA_ROUTER_RETRY_MAX=1`
- `NEBULA_ROUTER_RETRY_BACKOFF_MS=75`

## 3.2 Gateway

- `NEBULA_GATEWAY_MAX_REQUEST_BODY_BYTES=4194304`
- `NEBULA_AUTH_TOKENS=<token>:operator,<token>:viewer`

> 鉴权默认已改为 fail-closed。本地开发如需免鉴权应显式设置 `NEBULA_AUTH_DISABLED=true` 或 `NEBULA_DEV_AUTH_DISABLED=true`，生产环境不要使用该配置。

## 3.3 灰度节奏

1. 先 Router 单实例灰度（若有多实例，先 1 台）
2. 观察 30 分钟
3. 再全量 Router
4. 最后 Gateway 灰度与全量

---

## 4. 上线后验证

## 4.1 功能验证

1. 正常请求：应保持成功。
2. 人工制造上游瞬时失败：应出现 `retry_total` 增长。
3. 大于 4MB 请求：应返回 `413`。

## 4.2 指标验证

- Router：
  - `nebula_router_retry_total`
  - `nebula_router_retry_success_total`
  - `nebula_router_upstream_error_total{kind}`
  - `nebula_router_request_too_large_total`
  - `nebula_router_route_circuit_skipped_total`
  - `nebula_router_circuit_open_total`
  - `nebula_router_route_stale_stats_dropped_total`
- Gateway：
  - `nebula_gateway_upstream_error_total{kind}`
  - `nebula_gateway_request_too_large_total`
  - `nebula_gateway_auth_missing`
  - `nebula_gateway_auth_invalid`
  - `nebula_gateway_auth_forbidden`
  - `nebula_gateway_auth_rate_limited`

## 4.3 SLO 观察窗口

- 观察 1 小时：
  - 5xx 比例不高于基线 + 20%
  - P99 延迟不高于基线 + 15%
  - 无持续性错误尖峰

---

## 5. 回滚条件

满足任一条件，立即回滚：

1. 5xx 比例持续超过基线 + 20%，持续 10 分钟。
2. P99 延迟持续超过基线 + 15%，持续 10 分钟。
3. 出现无法解释的请求失败模式（非上游故障导致）。

---

## 6. 回滚步骤

## 6.1 软回滚（优先）

1. Router 关闭重试：
   - `NEBULA_ROUTER_RETRY_MAX=0`
2. 保留请求体上限与错误分类指标（低风险功能）
3. 重启 Router 进程

## 6.2 硬回滚（必要时）

1. 回滚到上一个稳定镜像/版本
2. 重启 Router 与 Gateway
3. 验证 `/healthz` 与核心推理路径

---

## 7. 验收记录模板

- 上线时间：
- 变更批次：
- 灰度范围：
- 指标对比（前/后）：
  - 5xx：
  - P99：
  - retry_success_rate：
- 是否触发回滚：
- 结论：
