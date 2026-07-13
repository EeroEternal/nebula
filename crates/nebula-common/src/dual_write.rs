//! Dual-write path: same emit point → local Prometheus counters **and** xtrace batch metrics.
//!
//! Per `docs/manual/module.md`:
//! - Prometheus `/metrics` = customer scrape surface
//! - xtrace = LLM semantic / true quantiles store
//! - No bridge from xtrace → Prometheus
//!
//! Labels must stay low-cardinality (`service`, `model_uid`, `outcome`, `kind`).
//! Never put `request_id` / `user_id` on scrape labels (they may appear only in traces/logs).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::debug;

/// Optional xtrace sink used alongside in-process Prometheus atomics.
#[derive(Clone, Default)]
pub struct DualWriteEmitter {
    inner: Option<Arc<xtrace_client::Client>>,
    service: String,
}

impl DualWriteEmitter {
    /// Build from `OBSERVE_URL` / `OBSERVE_TOKEN`. Returns a no-op emitter if URL is missing.
    pub fn from_env(service: impl Into<String>, url: Option<&str>, token: Option<&str>) -> Self {
        let service = service.into();
        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return Self {
                inner: None,
                service,
            };
        };
        let token = token.unwrap_or("");
        match xtrace_client::Client::new(url, token) {
            Ok(c) => {
                tracing::info!(%url, %service, "dual-write xtrace metrics enabled");
                Self {
                    inner: Some(Arc::new(c)),
                    service,
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "dual-write xtrace client failed; Prometheus-only");
                Self {
                    inner: None,
                    service,
                }
            }
        }
    }

    pub fn enabled(&self) -> bool {
        self.inner.is_some()
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    /// Fire-and-forget push of raw points (best-effort, never blocks the hot path long).
    pub fn push_points(&self, mut points: Vec<xtrace_client::MetricPoint>) {
        let Some(client) = self.inner.clone() else {
            return;
        };
        if points.is_empty() {
            return;
        }
        // Always stamp service for multi-component dashboards.
        for p in &mut points {
            p.labels
                .entry("service".to_string())
                .or_insert_with(|| self.service.clone());
        }
        tokio::spawn(async move {
            if let Err(e) = client.push_metrics(&points).await {
                debug!(error=%e, "dual-write xtrace push_metrics failed");
            }
        });
    }

    /// Counter / gauge sample at "now".
    pub fn emit(
        &self,
        name: &str,
        labels: impl IntoIterator<Item = (String, String)>,
        value: f64,
    ) {
        let mut map: HashMap<String, String> = labels.into_iter().collect();
        map.insert("service".to_string(), self.service.clone());
        self.push_points(vec![xtrace_client::MetricPoint {
            name: name.to_string(),
            labels: map,
            value,
            timestamp: Utc::now(),
        }]);
    }

    /// Request outcome dual-write (status class + optional model).
    ///
    /// Prometheus side is owned by the caller's atomics; this only mirrors to xtrace.
    pub fn emit_request_outcome(
        &self,
        metric_prefix: &str,
        model_uid: Option<&str>,
        status: u16,
        e2e_seconds: Option<f64>,
        aborted: bool,
    ) {
        if self.inner.is_none() {
            return;
        }
        let mut labels = HashMap::new();
        labels.insert("service".to_string(), self.service.clone());
        if let Some(m) = model_uid.filter(|s| !s.is_empty()) {
            labels.insert("model_uid".to_string(), m.to_string());
        }
        let outcome = if aborted {
            "aborted"
        } else if status >= 500 {
            "5xx"
        } else if status >= 400 {
            "4xx"
        } else if status >= 200 {
            "2xx"
        } else {
            "other"
        };
        labels.insert("outcome".to_string(), outcome.to_string());

        let ts = Utc::now();
        let mut points = vec![
            xtrace_client::MetricPoint {
                name: format!("{metric_prefix}_requests_total"),
                labels: labels.clone(),
                value: 1.0,
                timestamp: ts,
            },
            xtrace_client::MetricPoint {
                name: format!("{metric_prefix}_request_outcome"),
                labels: labels.clone(),
                value: 1.0,
                timestamp: ts,
            },
        ];
        if let Some(secs) = e2e_seconds {
            points.push(xtrace_client::MetricPoint {
                name: format!("{metric_prefix}_e2e_latency_seconds"),
                labels: labels.clone(),
                value: secs,
                timestamp: ts,
            });
        }
        self.push_points(points);
    }

    /// TTFT sample (streaming only).
    pub fn emit_ttft(&self, metric_prefix: &str, model_uid: &str, seconds: f64) {
        if self.inner.is_none() {
            return;
        }
        let labels = HashMap::from([
            ("service".to_string(), self.service.clone()),
            ("model_uid".to_string(), model_uid.to_string()),
        ]);
        self.push_points(vec![xtrace_client::MetricPoint {
            name: format!("{metric_prefix}_ttft_seconds"),
            labels,
            value: seconds,
            timestamp: Utc::now(),
        }]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_url() {
        let e = DualWriteEmitter::from_env("test", None, None);
        assert!(!e.enabled());
        // no panic
        e.emit_request_outcome("nebula_router", Some("m"), 200, Some(0.1), false);
    }
}
