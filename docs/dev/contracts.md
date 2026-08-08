# L1 一致性契约套件（Phase 1）

> 目标：客户写一次代码可换已支持引擎。边界在 Gateway + Adapter；Router 不做协议翻译。总纲见 [`../arch/vision.md`](../arch/vision.md)。协议/方言见 [`unigateway.md`](./unigateway.md)。

## 强制条目（验收）

| ID | 维度 | 期望 | 现状抓手 | 状态 |
|----|------|------|----------|------|
| C1 | OpenAI 路径 | `/v1/chat/completions` 流式/非流式均可 | Gateway e2e / 真机通路径 | ✅ |
| C2 | Abort | 客户端取消 → 上游 abort；不计 5xx 预算 | `test_cancel_sse.sh` / `phase0_slo_burn.sh` | ✅ |
| C3 | 错误码 | 限流/不兼容/过载/上游故障稳定 JSON 映射 | `interface/errors` + 租户拒绝码 | ✅ |
| C4 | Capability 三态 | Supported / Unsupported / Unknown；禁止用 0 装支持 | `EngineCapability` / `SupportLevel` | ✅ |
| C5 | Tooling | 含 tools：`Supported` 透传；`Unsupported` → 400 `unsupported`；`Unknown` 放行+warn；不静默丢字段 | `interface/gate` + `tool_calling` | ✅ |
| C6 | KV/指标单位 | 对外统一字段；引擎差在 scrape | `EndpointStats.kv_cache_usage` | ✅ |

流式 tool_calls 完整 SSE 转发仍 ⏸（非流路径已保留 tool_calls）。

## C3 错误码表（Gateway 对外）

| 场景 | HTTP | `error.type` | `error.code` |
|------|------|--------------|--------------|
| 租户配额 | 429 / 403 | `tenant_quota` | `tenant_*`（现有） |
| tooling 不支持 | 400 | `unsupported` | `tool_calling_unsupported` |
| 上游连接失败 | 502 | `upstream_error` | `upstream_connect` |
| 上游超时 | 504 | `upstream_error` | `upstream_timeout` |
| 上游其他 | 502 | `upstream_error` | `upstream_other` |
| 请求体过大 | 413 | `invalid_request_error` | `payload_too_large` |
| Router 过载（非 JSON body） | 429 | `rate_limit` | `endpoints_overloaded` |
| Router 无端点（非 JSON body） | 503 | `server_error` | `no_ready_endpoint` |

引擎已返回 OpenAI JSON 错误时原样转发，不二次包装。

## 协议适配归属

OpenAI chat / Responses / Anthropic Messages / tooling → **[UniGateway](https://github.com/EeroEternal/unigateway)**（`unigateway-sdk` 2.7 `conversion`）。实现：`crates/nebula-gateway/src/interface/`。Gateway 只做鉴权、租户、C5 门控、C3 错误 envelope、注入 model、调 Router。

## 怎么跑

```bash
cargo test -p nebula-common -p nebula-gateway
```

真机：Phase 0 通路径 + `scripts/phase0_tenant_isolation.sh` + `scripts/phase0_slo_burn.sh`（见 roadmap）。
