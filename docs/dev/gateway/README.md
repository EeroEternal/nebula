# Gateway / Router 运维说明

边界：[`../api_ownership.md`](../api_ownership.md)。架构：[`../../arch/architecture.md`](../../arch/architecture.md)。排期：[`../../arch/optimization.md`](../../arch/optimization.md)。

Gateway = 外部入口、鉴权、审计、协议适配、abort；**不做**实例选择。Router = 选路、重试、熔断、代理。

## 已落地能力（摘要）

Router：可配置重试与失败 endpoint 排除、请求体上限、熔断、stale stats 过滤、`/metrics`。  
Gateway：请求体上限、共享鉴权、审计、`x-nebula-model`、`/metrics`、双写 xtrace。

## 常用环境变量

| 变量 | 默认/说明 |
|------|-----------|
| `NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES` | `4194304` |
| `NEBULA_ROUTER_RETRY_MAX` | `1` |
| `NEBULA_ROUTER_RETRY_BACKOFF_MS` | `75` |
| `NEBULA_GATEWAY_MAX_REQUEST_BODY_BYTES` | `4194304` |
| `NEBULA_AUTH_TOKENS` | 生产必配；开发免鉴权用 `NEBULA_AUTH_DISABLED=true` |

## 灰度与回滚

1. `cargo check --workspace`；确认 gateway/router `/metrics` 已采。  
2. 先 Router 单实例灰度 → 观察 30 分钟 → 全量 → 再 Gateway。  
3. 验证：正常请求成功；瞬时上游失败可见 `retry_total`；超大体返回 413。  
4. 软回滚：`NEBULA_ROUTER_RETRY_MAX=0` 重启 Router。硬回滚：回上一镜像并验 `/healthz`。

回滚触发：5xx 相对基线 +20% 持续 10 分钟，或 P99 +15% 持续 10 分钟。

## 观测

热路径指标与双写见 [`../observability.md`](../observability.md)；日志见 [`../loki.md`](../loki.md)。BFF 观测面板只调 BFF `/api/v2/observability/*`，不直连 Prometheus/xtrace。
