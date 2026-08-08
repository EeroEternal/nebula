# UniGateway

Gateway 协议适配（含 tooling / Responses / Anthropic Messages）统一走 [UniGateway](https://github.com/EeroEternal/unigateway)。

**依赖：** `nebula-gateway` 使用同线 **`unigateway-sdk` 2.7**（`default-features = false, features = ["conversion"]`），经 facade 使用 protocol/core 转换面，不拉起 UniGatewayEngine / provider pool。

**现状：** `/v1/responses`、`/v1/messages` → OpenAI chat 再进 Router（`protocol_adapt.rs`）。C5 tooling 验收继续复用 UG tools 转换与 `ToolCallingCapabilities`。

**边界：** UG 只处理「怎么跟上游说话」；集群选路仍归 Nebula Router（etcd Placement / Endpoint）。禁止用 UG provider pool、TOML 权威或库内选路替换 Router；禁止 Scheduler / Node 依赖 UG。

失败时回退 Native HTTP Passthrough。有明确协议缺口再扩薄适配或提 UG 改动；不为「集成完整度」扩 UG。
