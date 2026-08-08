# L1 一致性契约套件（Phase 1）

> 目标：客户写一次代码可换已支持引擎。边界在 Gateway + Adapter；Router 不做协议翻译。总纲见 [`../arch/vision.md`](../arch/vision.md)。协议/方言见 [`unigateway.md`](./unigateway.md)。

## 强制条目（验收）

| ID | 维度 | 期望 | 现状抓手 |
|----|------|------|----------|
| C1 | OpenAI 路径 | `/v1/chat/completions` 流式/非流式均可 | Gateway e2e / 真机通路径 |
| C2 | Abort | 客户端取消 → 上游 abort；不计 5xx 预算 | 现有 abort metrics |
| C3 | 错误码 | 限流/不兼容/过载/上游故障稳定映射 | Gateway 错误映射 + 租户拒绝码 |
| C4 | Capability 三态 | Supported / Unsupported / Unknown；禁止用 0 装支持 | `EngineCapability` / UI n/a |
| C5 | Tooling | 请求含 tools 时：`tool_calling=Supported` 经 UG 透传；`Unsupported` → 400 `unsupported`；`Unknown` 放行+warn；不静默丢字段 | `interface/gate` + `EngineCapability.tool_calling`；见 [`unigateway.md`](./unigateway.md) |
| C6 | KV/指标单位 | 对外统一字段；引擎差在 scrape | `EndpointStats.kv_cache_usage` |

## 协议适配归属

OpenAI chat / Responses / Anthropic Messages / tooling 的解析与互转 → **[UniGateway](https://github.com/EeroEternal/unigateway)**（Gateway 依赖 `unigateway-sdk` 2.7 `conversion`）。实现目录：`crates/nebula-gateway/src/interface/`。Nebula Gateway 只做鉴权、租户、C5 门控、注入 model、调 Router；不自建第二套方言转换，也不用 UG 做选路。

## 怎么跑

单测：`cargo test -p nebula-common -p nebula-gateway`（能力/错误相关）。  
真机：Gateway→Router→vLLM/SGLang 通路径（见 roadmap Phase 0）。  
C5：UG 转换单测 + Gateway「Capability=Unsupported → unsupported」契约；跨引擎 fixture 继续勾选。
