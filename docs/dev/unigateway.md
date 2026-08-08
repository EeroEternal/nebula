# UniGateway（接口层）

**规范边界：** Nebula **接口层**（协议适配 + C5 tooling 门控）以 UniGateway 为规范底座；**路由 / 选路 / 副本池**仍是 Nebula Router，不引入 UniGatewayEngine 或 UG 内建路由。

**依赖入口：** Gateway 只依赖 [`unigateway-sdk`](https://docs.rs/unigateway-sdk)（`default-features = false`，`features = ["conversion"]`）。类型与转换经 facade：`unigateway_sdk::core` / `unigateway_sdk::protocol`。Node 侧若仍直接依赖 `unigateway` 旧版，与接口层无关。

上游：[EeroEternal/unigateway](https://github.com/EeroEternal/unigateway)。

## 接口层路由表

| 入站 | 适配 | 出站（经 Router） | C5 门控 |
|------|------|-------------------|--------|
| `POST /v1/chat/completions` | 直通 OpenAI JSON | 同左 | 有 `tools`/`tool_choice` 时按引擎 `tool_calling` |
| `POST /v1/responses` | UG → OpenAI chat →（响应）UG Responses | OpenAI chat | 同上（适配前读入站 body） |
| `POST /v1/messages` | UG Anthropic → OpenAI chat →（响应）Anthropic | OpenAI chat | 同上 |
| `POST /v1/embeddings` | 直通 | 同左 | 无 |
| `POST /v1/rerank` | 直通 | 同左 | 无 |

代码：`crates/nebula-gateway/src/interface/`（`adapt` + `gate`），编排在 `handlers` + `proxy_common`。

## C5 门控语义

依据 `EngineCapability.tool_calling`（`SupportLevel`）：

| 级别 | 行为 |
|------|------|
| `Supported` | 放行 |
| `Unknown` | 放行 + warn 日志（兼容未填引擎类型） |
| `Unsupported` | **400**，`error.type=unsupported`，稳定文案；**不**静默剥 `tools` |

引擎类型优先来自模型 spec（`/models/{uid}/spec` 的 `engine`）；未知则 `Unknown`。静态表：vLLM / SGLang 为 `Supported`。

## 不做

- 不把 UniGateway 当 Nebula 的路由或副本池。
- 不在协议适配路径静默丢弃 `tools` / `tool_choice` / `tool_calls` / ToolUse|ToolResult。
- 不把 Node 引擎生命周期绑到 UniGatewayEngine。
