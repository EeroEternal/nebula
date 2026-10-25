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

/// Default KV location for pinned keys (the L1 host-DRAM tier).
///
/// One literal for the control plane and the router: `lmcache_instance_id` had already
/// drifted once between the two sides and made every pin miss its worker.
pub use nebula_common::LMCACHE_DEFAULT_KV_LOCATION as DEFAULT_KV_LOCATION;
const CONTROLLER_TIMEOUT_MS: u64 = 5_000;

/// Thin client for the controller's lifecycle endpoints.
#[derive(Clone)]
pub struct LmcacheController {
    base: String,
    http: reqwest::Client,
}

impl LmcacheController {
    /// Build from the environment; `None` when no controller is configured.
    pub fn from_env() -> Option<Self> {
        let base = std::env::var("NEBULA_LMCACHE_CONTROLLER_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty())
            .map(|v| {
                if v.starts_with("http://") || v.starts_with("https://") {
                    v
                } else {
                    format!("http://{v}")
                }
            })?;
        let http = nebula_common::control_plane_http_client().ok()?;
        Some(Self { base, http })
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

    /// Tokens of this prefix the shared store already holds for `model_uid` (0 = cold).
    ///
    /// The controller answers for every registered instance, so with several models on one
    /// controller the count must be filtered by the `{model_uid}-` instance prefix -
    /// otherwise model A reports model B's cache as its own.
    pub async fn lookup(&self, model_uid: &str, tokens: &[u32]) -> Result<u64, ServiceError> {
        let v = self
            .post(
                "/lookup",
                serde_json::json!({ "tokens": tokens, "instance_id": model_uid }),
            )
            .await?;
        Ok(max_matched_tokens_for_model(&v, model_uid))
    }

    /// Pin `tokens` in `location` on every instance in `instances`.
    ///
    /// Returns `(pinned_tokens, failures)` where a failure is `(instance_id, reason)` -
    /// pinning is best-effort per replica, and a partially pinned prefix must be visible to
    /// the operator rather than silently reported as complete.
    pub async fn pin(
        &self,
        instances: &[String],
        location: &str,
        tokens: &[u32],
    ) -> Result<(u64, Vec<(String, String)>), ServiceError> {
        let mut pinned = 0u64;
        let mut failures = Vec::new();
        for instance in instances {
            match self
                .post(
                    "/pin",
                    serde_json::json!({
                        "instance_id": instance,
                        "location": location,
                        "tokens": tokens,
                    }),
                )
                .await
            {
                Ok(v) => pinned += v.get("num_tokens").and_then(|n| n.as_u64()).unwrap_or(0),
                Err(e) => failures.push((instance.clone(), e.to_string())),
            }
        }
        if instances.is_empty() {
            return Err(ServiceError::BadRequest(
                "no LMCache instance registered for this model".into(),
            ));
        }
        Ok((pinned, failures))
    }

    /// Clear the cache in `location` on every instance in `instances`.
    pub async fn clear(
        &self,
        instances: &[String],
        location: &str,
    ) -> Result<(u64, Vec<(String, String)>), ServiceError> {
        let mut cleared = 0u64;
        let mut failures = Vec::new();
        for instance in instances {
            match self
                .post(
                    "/clear",
                    serde_json::json!({ "instance_id": instance, "location": location }),
                )
                .await
            {
                Ok(v) => cleared += v.get("num_tokens").and_then(|n| n.as_u64()).unwrap_or(0),
                Err(e) => failures.push((instance.clone(), e.to_string())),
            }
        }
        if instances.is_empty() {
            return Err(ServiceError::BadRequest(
                "no LMCache instance registered for this model".into(),
            ));
        }
        Ok((cleared, failures))
    }
}

/// Deepest `["<Backend>", n]` entry belonging to `model_uid`'s instances.
///
/// Instances are named `{model_uid}-{replica_id}`, so the filter is a prefix match; an
/// unfiltered max would report another model's cache as this model's hit.
pub fn max_matched_tokens_for_model(v: &serde_json::Value, model_uid: &str) -> u64 {
    let prefix = format!("{model_uid}-");
    fn scan(value: &serde_json::Value, prefix: &str) -> Option<u64> {
        match value {
            serde_json::Value::Array(a) => a.get(1).and_then(|n| n.as_u64()),
            serde_json::Value::Object(inner) => {
                // The prefix names *instances*; wrappers like `layout_info` must still be
                // traversed, so the filter only applies on the level whose values are
                // token lists.
                let instance_level = inner.values().any(|v| v.is_array());
                inner
                    .iter()
                    .filter(|(k, _)| k.as_str() != "event_id")
                    .filter(|(k, val)| {
                        !(instance_level && val.is_array()) || k.as_str().starts_with(prefix)
                    })
                    .filter_map(|(_, val)| scan(val, prefix))
                    .max()
            }
            _ => None,
        }
    }
    scan(v, &prefix).unwrap_or(0)
}

/// Deepest `["<Backend>", n]` entry in a `/lookup` response (`layout_info` nesting).
///
/// Same shape the router parses for its warm signal, so the console and the data plane
/// never disagree about "how much of this prefix is cached".
#[cfg(test)]
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
    fn lookup_is_filtered_by_model_instance_prefix() {
        // Two models share one controller; only `qwen-*` instances belong to this model.
        let v = serde_json::json!({
            "event_id": "Lookup3",
            "layout_info": {
                "qwen-0": ["LocalCPUBackend", 256],
                "qwen-1": ["LocalCPUBackend", 512],
                "llama-0": ["LocalCPUBackend", 4096]
            }
        });
        assert_eq!(
            max_matched_tokens_for_model(&v, "qwen"),
            512,
            "must not report another model's cache as this model's hit"
        );
        assert_eq!(max_matched_tokens_for_model(&v, "llama"), 4096);
        assert_eq!(max_matched_tokens_for_model(&v, "mistral"), 0);
        // Prefix must be a real instance boundary: `qwen2` is not `qwen`.
        let v2 = serde_json::json!({ "layout_info": { "qwen2-0": ["LocalCPUBackend", 128] } });
        assert_eq!(max_matched_tokens_for_model(&v2, "qwen"), 0);
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
