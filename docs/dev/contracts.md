# L1 一致性契约套件（Phase 1）

> 目标：客户写一次代码可换已支持引擎。边界在 Gateway + Adapter；Router 不做协议翻译。总纲见 [`../arch/vision.md`](../arch/vision.md)。

## 强制条目（验收）

| ID | 维度 | 期望 | 现状抓手 |
|----|------|------|----------|
| C1 | OpenAI 路径 | `/v1/chat/completions` 流式/非流式均可 | Gateway e2e / 真机通路径 |
| C2 | Abort | 客户端取消 → 上游 abort；不计 5xx 预算 | 现有 abort metrics |
| C3 | 错误码 | 限流/不兼容/过载/上游故障稳定映射 | Gateway 错误映射 + 租户拒绝码 |
| C4 | Capability 三态 | Supported / Unsupported / Unknown；禁止用 0 装支持 | `EngineCapability` / UI n/a |
| C5 | Tooling | 不支持时显式 `unsupported`，不静默丢字段 | Adapter 能力表；契约测试待补 |
| C6 | KV/指标单位 | 对外统一字段；引擎差在 scrape | `EndpointStats.kv_cache_usage` |

## 怎么跑

单测：`cargo test -p nebula-common -p nebula-gateway`（能力/错误相关）。  
真机：Gateway→Router→vLLM/SGLang 通路径（见 roadmap Phase 0）。  
缺项（C5 全量、跨引擎 fixture CI）继续在本表勾选，不另开散文计划。
