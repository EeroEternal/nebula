# UniGateway

Gateway 已用 `unigateway-protocol` / `unigateway-core` 做协议适配（`/v1/responses`、`/v1/messages` → OpenAI chat 再走 Router）。见 `crates/nebula-gateway/src/protocol_adapt.rs`。

**边界：** UG 只处理「怎么跟上游说话」；集群选路仍归 Nebula Router（etcd Placement / Endpoint）。禁止用 UG provider pool、TOML 权威或库内选路替换 Router；禁止 Scheduler / Node 依赖 UG。

失败时回退 Native HTTP Passthrough。有明确协议缺口再扩薄适配；不为「集成完整度」扩 UG。
