use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use nebula_common::{EndpointInfo, EndpointStats, PlacementPlan};
use nebula_meta::{EtcdMetaStore, MetaStore};

pub async fn endpoints_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/endpoints/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list endpoints, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut snapshot: Vec<EndpointInfo> = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(info) = serde_json::from_slice::<EndpointInfo>(&v) {
                snapshot.push(info);
            }
        }
        router.replace_all_endpoints(snapshot);

        let mut stream = match store.watch_prefix("/endpoints/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch endpoints, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            if let Some(v) = ev.value {
                if let Ok(info) = serde_json::from_slice::<EndpointInfo>(&v) {
                    router.upsert_endpoint(info);
                }
            } else {
                let parts: Vec<&str> = ev.key.split('/').collect();
                if parts.len() >= 4 {
                    if let Ok(replica_id) = parts[3].parse::<u32>() {
                        router.remove_endpoint(parts[2], replica_id);
                    }
                }
            }
        }

        tracing::warn!("endpoints watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn placement_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/placements/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list placements, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut versions = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&v) {
                router.set_model_mapping(&plan.model_uid, &plan.model_name);
                versions.push((plan.model_uid, plan.version));
            }
        }
        router.replace_all_plan_versions(versions);

        let mut stream = match store.watch_prefix("/placements/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch placements, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            match ev.value {
                Some(v) => {
                    let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&v) else {
                        continue;
                    };
                    router.set_model_mapping(&plan.model_uid, &plan.model_name);
                    router.set_plan_version(&plan.model_uid, plan.version);
                }
                None => {
                    // /placements/{model_uid}
                    let model_uid = ev
                        .key
                        .strip_prefix("/placements/")
                        .unwrap_or(&ev.key)
                        .to_string();
                    if !model_uid.is_empty() {
                        router.clear_plan_version(&model_uid);
                    }
                }
            }
        }

        tracing::warn!("placements watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[derive(Debug, serde::Deserialize)]
struct XtraceMetricValue {
    timestamp: String,
    value: f64,
}

#[derive(Debug, serde::Deserialize)]
struct XtraceMetricSeries {
    labels: serde_json::Value,
    values: Vec<XtraceMetricValue>,
}

#[derive(Debug, serde::Deserialize)]
struct XtraceMetricMeta {
    #[serde(default)]
    latest_ts: Option<String>,
    series_count: usize,
    truncated: bool,
}

#[derive(Debug, serde::Deserialize)]
struct XtraceMetricResponse {
    data: Vec<XtraceMetricSeries>,
    meta: XtraceMetricMeta,
}

#[derive(Debug, serde::Deserialize)]
struct XtraceErrorBody {
    #[serde(default)]
    code: Option<String>,
}

enum QueryResult {
    Ok(XtraceMetricResponse),
    RateLimited { retry_after_secs: u64 },
    Err(String),
}

async fn query_metric(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    metric_name: &str,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> QueryResult {
    let mut url = match reqwest::Url::parse(&format!(
        "{}/api/public/metrics/query",
        base_url.trim_end_matches('/')
    )) {
        Ok(u) => u,
        Err(e) => return QueryResult::Err(format!("invalid xtrace url: {e}")),
    };

    url.query_pairs_mut()
        .append_pair("name", metric_name)
        .append_pair("from", &from.to_rfc3339())
        .append_pair("to", &to.to_rfc3339())
        .append_pair("step", "60s")
        .append_pair("agg", "last");

    let req = http.get(url).bearer_auth(token);
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return QueryResult::Err(format!("request failed: {e}")),
    };

    if resp.status().as_u16() == 429 {
        let retry_after_secs = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        return QueryResult::RateLimited { retry_after_secs };
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let code = serde_json::from_str::<XtraceErrorBody>(&text)
            .ok()
            .and_then(|b| b.code)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        return QueryResult::Err(format!("http {} code={} body={}", status, code, text));
    }

    match resp.json::<XtraceMetricResponse>().await {
        Ok(body) => QueryResult::Ok(body),
        Err(e) => QueryResult::Err(format!("decode failed: {e}")),
    }
}

pub async fn stats_sync_loop(
    xtrace_url: String,
    xtrace_token: String,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};

    const POLL_INTERVAL: Duration = Duration::from_secs(10);
    const VIRTUAL_TOTAL: u64 = 1_000_000;
    let freshness_ms: u64 = std::env::var("NEBULA_XTRACE_METRIC_MAX_AGE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60_000);

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    'outer: loop {
        let now = chrono::Utc::now();
        let from = now - chrono::Duration::seconds(60);
        let now_ms = now.timestamp_millis() as u64;

        let mut pending_map: HashMap<(String, u32), u64> = HashMap::new();
        let mut kv_usage_map: HashMap<(String, u32), f64> = HashMap::new();
        let mut prefix_hit_map: HashMap<(String, u32), f64> = HashMap::new();

        for metric_name in [
            "pending_requests",
            "kv_cache_usage",
            "prefix_cache_hit_rate",
        ] {
            match query_metric(&http, &xtrace_url, &xtrace_token, metric_name, from, now).await {
                QueryResult::RateLimited { retry_after_secs } => {
                    router.inc_xtrace_rate_limited();
                    tracing::warn!(
                        metric=%metric_name,
                        retry_after_secs,
                        "xtrace rate limited stats query; backing off"
                    );
                    tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                    continue 'outer;
                }
                QueryResult::Err(e) => {
                    router.inc_xtrace_query_errors();
                    tracing::warn!(metric=%metric_name, error=%e, "failed to query xtrace metrics");
                    continue;
                }
                QueryResult::Ok(resp) => {
                    if resp.meta.truncated {
                        router.inc_xtrace_truncated();
                        tracing::warn!(
                            metric=%metric_name,
                            series_count=resp.meta.series_count,
                            "xtrace metric response truncated"
                        );
                    }

                    if resp.meta.series_count > 0 {
                        let latest_ms = resp
                            .meta
                            .latest_ts
                            .as_deref()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.timestamp_millis() as u64);

                        let stale = latest_ms
                            .map(|ts| now_ms.saturating_sub(ts) > freshness_ms)
                            .unwrap_or(true);
                        if stale {
                            router.inc_xtrace_stale();
                            tracing::warn!(
                                metric=%metric_name,
                                latest_ts=?resp.meta.latest_ts,
                                freshness_ms,
                                "xtrace metric response considered stale; skipping"
                            );
                            continue;
                        }
                    }

                    for series in &resp.data {
                        let model_uid = series
                            .labels
                            .get("model_uid")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let replica_id: u32 = series
                            .labels
                            .get("replica_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);

                        if model_uid.is_empty() {
                            continue;
                        }

                        let Some(last) = series.values.last() else {
                            continue;
                        };
                        let _ = &last.timestamp;
                        let key = (model_uid, replica_id);
                        match metric_name {
                            "pending_requests" => {
                                pending_map.insert(key, last.value.max(0.0) as u64);
                            }
                            "kv_cache_usage" => {
                                kv_usage_map.insert(key, last.value.clamp(0.0, 1.0));
                            }
                            "prefix_cache_hit_rate" => {
                                prefix_hit_map.insert(key, last.value.clamp(0.0, 1.0));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let mut all_keys: HashSet<(String, u32)> = HashSet::new();
        all_keys.extend(pending_map.keys().cloned());
        all_keys.extend(kv_usage_map.keys().cloned());
        all_keys.extend(prefix_hit_map.keys().cloned());

        for key in all_keys {
            let pending = pending_map.get(&key).copied().unwrap_or(0);
            let kv_usage = kv_usage_map.get(&key).copied();
            let prefix_hit = prefix_hit_map.get(&key).copied();

            let (kv_used, kv_free) = match kv_usage {
                Some(usage) => {
                    let used = (usage * VIRTUAL_TOTAL as f64) as u64;
                    (Some(used), Some(VIRTUAL_TOTAL - used))
                }
                None => (None, None),
            };

            let stats = EndpointStats {
                model_uid: key.0,
                replica_id: key.1,
                last_updated_ms: now_ms,
                pending_requests: pending,
                prefix_cache_hit_rate: prefix_hit,
                prompt_cache_hit_rate: None,
                kv_cache_used_bytes: kv_used,
                kv_cache_free_bytes: kv_free,
            };

            router.upsert_stats(stats);
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
