//! `lmcache_controller` lifecycle client (Batch 6.3: warm & pin / clear / lookup).
//!
//! The controller owns the shared KV store's key inventory: `POST /lookup` answers "is this
//! prefix there", `POST /pin` locks keys so eviction cannot drop a hot system prompt, and
//! `POST /clear` wipes an instance's cache. Contracts taken from `lmcache 0.5.5`
//! (`lmcache/v1/api_server/__main__.py`):
//!
//! ```text
//! POST /lookup {tokens, instance_id}            -> {event_id, layout_info}
//! POST /pin    {instance_id, location, tokens}  -> {event_id, num_tokens}
//! POST /clear  {instance_id, location}          -> {event_id, num_tokens}
//! ```
//!
//! Disabled unless a base URL is configured (`NEBULA_LMCACHE_CONTROLLER_BASE_URL`, or
//! `http://` + `NEBULA_LMCACHE_CONTROLLER_URL` when that is a `host:port`), so a control
//! plane without pd_split keeps working unchanged.

use std::time::Duration;

use crate::error::ServiceError;

/// Default KV location for the pinned keys (the L1 host-DRAM tier).
pub const DEFAULT_KV_LOCATION: &str = "LocalCPUBackend";
const CONTROLLER_TIMEOUT_MS: u64 = 5_000;

/// Thin client for the controller's lifecycle endpoints.
#[derive(Clone)]
pub struct LmcacheController {
    base: String,
    instance_id: String,
    http: reqwest::Client,
}

impl LmcacheController {
    /// Build from the environment; `None` when no controller is configured.
    pub fn from_env() -> Option<Self> {
        let base = std::env::var("NEBULA_LMCACHE_CONTROLLER_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                let raw = std::env::var("NEBULA_LMCACHE_CONTROLLER_URL").ok()?;
                let raw = raw.trim();
                if raw.is_empty() {
                    return None;
                }
                Some(
                    if raw.starts_with("http://") || raw.starts_with("https://") {
                        raw.trim_end_matches('/').to_string()
                    } else {
                        format!("http://{}", raw.trim_end_matches('/'))
                    },
                )
            })?;
        let instance_id = std::env::var("NEBULA_LMCACHE_INSTANCE_ID")
            .unwrap_or_else(|_| "lmcache_default_instance".to_string());
        let http = nebula_common::control_plane_http_client().ok()?;
        Some(Self {
            base,
            instance_id,
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ServiceError> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(Duration::from_millis(CONTROLLER_TIMEOUT_MS))
            .send()
            .await
            .map_err(|e| ServiceError::Upstream(format!("lmcache_controller unreachable: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ServiceError::Upstream(format!(
                "lmcache_controller {path} -> {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
            ServiceError::Upstream(format!("lmcache_controller {path} unparseable: {e}"))
        })
    }

    /// Tokens of this prefix the shared store already holds (0 = cold).
    pub async fn lookup(&self, tokens: &[u32]) -> Result<u64, ServiceError> {
        let v = self
            .post(
                "/lookup",
                serde_json::json!({ "tokens": tokens, "instance_id": self.instance_id }),
            )
            .await?;
        Ok(max_matched_tokens(&v))
    }

    /// Pin `tokens` in `location` so eviction cannot drop them.
    pub async fn pin(&self, location: &str, tokens: &[u32]) -> Result<u64, ServiceError> {
        let v = self
            .post(
                "/pin",
                serde_json::json!({
                    "instance_id": self.instance_id,
                    "location": location,
                    "tokens": tokens,
                }),
            )
            .await?;
        Ok(v.get("num_tokens").and_then(|n| n.as_u64()).unwrap_or(0))
    }

    /// Clear this instance's cache in `location`.
    pub async fn clear(&self, location: &str) -> Result<u64, ServiceError> {
        let v = self
            .post(
                "/clear",
                serde_json::json!({ "instance_id": self.instance_id, "location": location }),
            )
            .await?;
        Ok(v.get("num_tokens").and_then(|n| n.as_u64()).unwrap_or(0))
    }
}

/// Deepest `["<Backend>", n]` entry in a `/lookup` response (`layout_info` nesting).
///
/// Same shape the router parses for its warm signal, so the console and the data plane
/// never disagree about "how much of this prefix is cached".
fn max_matched_tokens(v: &serde_json::Value) -> u64 {
    fn scan(value: &serde_json::Value) -> Option<u64> {
        match value {
            serde_json::Value::Array(a) => a.get(1).and_then(|n| n.as_u64()),
            serde_json::Value::Object(inner) => inner
                .iter()
                .filter(|(k, _)| k.as_str() != "event_id")
                .filter_map(|(_, val)| scan(val))
                .max(),
            _ => None,
        }
    }
    scan(v).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_response_yields_the_deepest_matched_token_count() {
        // Shape returned by lmcache 0.5.5.
        let v = serde_json::json!({
            "event_id": "Lookup123",
            "layout_info": { "qwen-0": ["LocalCPUBackend", 512] }
        });
        assert_eq!(max_matched_tokens(&v), 512);

        // Cold: empty layout_info.
        let cold = serde_json::json!({ "event_id": "Lookup1", "layout_info": {} });
        assert_eq!(max_matched_tokens(&cold), 0);

        // Multiple holders: report the deepest, not the first.
        let multi = serde_json::json!({
            "event_id": "Lookup2",
            "layout_info": {
                "qwen-0": ["LocalCPUBackend", 128],
                "qwen-1": ["LocalCPUBackend", 768]
            }
        });
        assert_eq!(max_matched_tokens(&multi), 768);

        // `event_id` must not be mistaken for a token list.
        assert_eq!(max_matched_tokens(&serde_json::json!({"event_id": "x"})), 0);
    }

    #[test]
    fn requests_match_the_controller_contract() {
        // Guards the field names against the lmcache 0.5.5 API (a rename here silently
        // turns every pin/clear into a 422 the operator only sees at runtime).
        let pin = serde_json::json!({
            "instance_id": "lmcache_default_instance",
            "location": DEFAULT_KV_LOCATION,
            "tokens": [1u32, 2, 3],
        });
        assert_eq!(pin["location"], "LocalCPUBackend");
        assert_eq!(pin["tokens"].as_array().unwrap().len(), 3);

        let clear = serde_json::json!({
            "instance_id": "lmcache_default_instance",
            "location": DEFAULT_KV_LOCATION,
        });
        assert!(clear.get("tokens").is_none(), "clear takes no token list");
    }
}
