//! Benchmark profiles, recommendation, and canary release contracts (Product P5).

use serde::{Deserialize, Serialize};

/// Standard workload identity used for apples-to-apples comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadClass {
    ShortChat,
    LongContext,
    CodeCompletion,
    Custom,
}

/// Declarative benchmark workload (also mirrored under `scripts/benchmark/workloads/`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkWorkload {
    pub id: String,
    pub class: WorkloadClass,
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default = "default_requests")]
    pub request_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_max_tokens() -> u32 {
    128
}
fn default_concurrency() -> u32 {
    1
}
fn default_requests() -> u32 {
    20
}

/// Dimensions that must match before online SLI may correct a benchmark profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProfileKey {
    pub model_name: String,
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_name: Option<String>,
    pub workload_id: String,
    /// Fingerprint of key params (tp, max_model_len, gpu_util, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    InsufficientData,
}

/// One reproducible benchmark run.
///
/// etcd: `/benchmarks/runs/{run_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkRun {
    pub run_id: String,
    pub profile_key: ProfileKey,
    pub workload: BenchmarkWorkload,
    pub status: BenchmarkRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,
    /// TTFT p50/p95 in ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p50_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms: Option<f64>,
    /// Inter-token / TPOT p95 in ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpot_p95_ms: Option<f64>,
    /// Successful output tokens per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps: Option<f64>,
    /// Error ratio in `[0,1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    /// Peak observed GPU memory MB (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_vram_mb: Option<u64>,
    /// Relative cost unit: seconds of GPU-time per 1k successful tokens (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_1k_tokens: Option<f64>,
    #[serde(default)]
    pub started_at_ms: u64,
    #[serde(default)]
    pub finished_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_notes: Option<String>,
}

/// Aggregated performance profile for recommend.
///
/// etcd: `/benchmarks/profiles/{hash}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceProfile {
    pub profile_key: ProfileKey,
    pub run_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_ttft_p95_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_throughput_tps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_error_rate: Option<f64>,
    #[serde(default)]
    pub sample_count: u32,
    #[serde(default)]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendRequest {
    pub model_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Optional SLO targets used for filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_cost_per_1k: Option<f64>,
    #[serde(default)]
    pub max_candidates: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RecommendConfidence {
    High,
    Medium,
    Low,
    InsufficientData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendCandidate {
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub confidence: RecommendConfidence,
    pub evidence_run_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_1k_tokens: Option<f64>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendResponse {
    pub model_name: String,
    pub status: RecommendConfidence,
    pub candidates: Vec<RecommendCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CanaryState {
    Pending,
    Running,
    Promoting,
    RolledBack,
    Completed,
    Failed,
}

/// Canary release of a candidate image/engine against a stable baseline.
///
/// etcd: `/canaries/{canary_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanaryRelease {
    pub canary_id: String,
    pub model_uid: String,
    pub stable_image_id: Option<String>,
    pub candidate_image_id: String,
    #[serde(default)]
    pub traffic_weight_percent: u32,
    pub state: CanaryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slo_breach: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_reason: Option<String>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

/// Built-in short workloads for scripts + unit tests.
pub fn builtin_workloads() -> Vec<BenchmarkWorkload> {
    vec![
        BenchmarkWorkload {
            id: "short-chat-v1".into(),
            class: WorkloadClass::ShortChat,
            prompt: "Say hello in one short sentence.".into(),
            max_tokens: 64,
            concurrency: 1,
            request_count: 20,
            notes: Some("Default short chat workload".into()),
        },
        BenchmarkWorkload {
            id: "long-context-v1".into(),
            class: WorkloadClass::LongContext,
            prompt: "Summarize the following text in 3 bullets: ".to_string()
                + &"lorem ipsum ".repeat(200),
            max_tokens: 256,
            concurrency: 1,
            request_count: 10,
            notes: Some("Longer prompt for context pressure".into()),
        },
    ]
}

fn profile_matches_request(key: &ProfileKey, req: &RecommendRequest) -> bool {
    if !key
        .model_name
        .eq_ignore_ascii_case(req.model_name.trim())
    {
        // Allow suffix / contains match for HF ids vs short names.
        if !key.model_name.contains(&req.model_name)
            && !req.model_name.contains(&key.model_name)
        {
            return false;
        }
    }
    if let Some(ref wid) = req.workload_id {
        if &key.workload_id != wid {
            return false;
        }
    }
    if let Some(ref plat) = req.platform {
        if key.platform.as_deref() != Some(plat.as_str()) {
            return false;
        }
    }
    true
}

/// Recommend engines from stored profiles. Never invent a silent default engine.
pub fn recommend_from_profiles(
    req: &RecommendRequest,
    profiles: &[PerformanceProfile],
    runs: &[BenchmarkRun],
) -> RecommendResponse {
    let mut candidates: Vec<RecommendCandidate> = Vec::new();

    for profile in profiles {
        if !profile_matches_request(&profile.profile_key, req) {
            continue;
        }
        if profile.sample_count == 0 {
            continue;
        }

        if let Some(max_ttft) = req.ttft_p95_ms_max {
            if profile
                .best_ttft_p95_ms
                .map(|v| v > max_ttft)
                .unwrap_or(true)
            {
                continue;
            }
        }
        if let Some(min_tps) = req.throughput_tps_min {
            if profile
                .best_throughput_tps
                .map(|v| v < min_tps)
                .unwrap_or(true)
            {
                continue;
            }
        }

        let related: Vec<&BenchmarkRun> = runs
            .iter()
            .filter(|r| profile.run_ids.contains(&r.run_id))
            .collect();
        let cost = related.iter().filter_map(|r| r.cost_per_1k_tokens).fold(
            None,
            |acc: Option<f64>, v| Some(acc.map(|a| a.min(v)).unwrap_or(v)),
        );
        if let Some(budget) = req.budget_cost_per_1k {
            if cost.map(|c| c > budget).unwrap_or(false) {
                continue;
            }
        }

        let confidence = match profile.sample_count {
            0 => RecommendConfidence::InsufficientData,
            1 => RecommendConfidence::Low,
            2..=4 => RecommendConfidence::Medium,
            _ => RecommendConfidence::High,
        };

        let image_id = related.iter().find_map(|r| r.image_id.clone());
        candidates.push(RecommendCandidate {
            engine_type: profile.profile_key.engine_type.clone(),
            engine_version: profile.profile_key.engine_version.clone(),
            image_id,
            platform: profile.profile_key.platform.clone(),
            confidence,
            evidence_run_ids: profile.run_ids.clone(),
            ttft_p95_ms: profile.best_ttft_p95_ms,
            throughput_tps: profile.best_throughput_tps,
            cost_per_1k_tokens: cost,
            rationale: format!(
                "matched workload={} samples={} platform={:?}",
                profile.profile_key.workload_id,
                profile.sample_count,
                profile.profile_key.platform
            ),
        });
    }

    candidates.sort_by(|a, b| {
        let ta = a.ttft_p95_ms.unwrap_or(f64::MAX);
        let tb = b.ttft_p95_ms.unwrap_or(f64::MAX);
        ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let max_n = req.max_candidates.unwrap_or(5).max(1) as usize;
    candidates.truncate(max_n);

    if candidates.is_empty() {
        return RecommendResponse {
            model_name: req.model_name.clone(),
            status: RecommendConfidence::InsufficientData,
            candidates: vec![],
            message: Some(
                "insufficient benchmark profiles for this model/workload/platform; \
                 run scripts/benchmark/run_benchmark.py and ingest results first"
                    .into(),
            ),
        };
    }

    let status = candidates
        .iter()
        .map(|c| c.confidence)
        .min_by_key(|c| match c {
            RecommendConfidence::High => 0,
            RecommendConfidence::Medium => 1,
            RecommendConfidence::Low => 2,
            RecommendConfidence::InsufficientData => 3,
        })
        .unwrap_or(RecommendConfidence::Low);

    RecommendResponse {
        model_name: req.model_name.clone(),
        status,
        candidates,
        message: None,
    }
}

/// Rebuild a profile from succeeded runs sharing the same ProfileKey.
pub fn build_profile_from_runs(key: &ProfileKey, runs: &[BenchmarkRun], now_ms: u64) -> PerformanceProfile {
    let matched: Vec<&BenchmarkRun> = runs
        .iter()
        .filter(|r| {
            r.profile_key == *key && r.status == BenchmarkRunStatus::Succeeded
        })
        .collect();
    let run_ids: Vec<String> = matched.iter().map(|r| r.run_id.clone()).collect();
    let best_ttft = matched.iter().filter_map(|r| r.ttft_p95_ms).fold(
        None,
        |acc: Option<f64>, v| Some(acc.map(|a| a.min(v)).unwrap_or(v)),
    );
    let best_tps = matched.iter().filter_map(|r| r.throughput_tps).fold(
        None,
        |acc: Option<f64>, v| Some(acc.map(|a| a.max(v)).unwrap_or(v)),
    );
    let mut errors: Vec<f64> = matched.iter().filter_map(|r| r.error_rate).collect();
    errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_error = if errors.is_empty() {
        None
    } else {
        Some(errors[errors.len() / 2])
    };

    PerformanceProfile {
        profile_key: key.clone(),
        run_ids,
        best_ttft_p95_ms: best_ttft,
        best_throughput_tps: best_tps,
        median_error_rate: median_error,
        sample_count: matched.len() as u32,
        updated_at_ms: now_ms,
    }
}

/// Decide whether canary should auto-rollback given SLO evaluation.
pub fn canary_should_rollback(slo_breaching: bool, traffic_weight_percent: u32) -> bool {
    slo_breaching && traffic_weight_percent > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_run(engine: &str, ttft: f64, tps: f64) -> BenchmarkRun {
        BenchmarkRun {
            run_id: format!("run-{engine}"),
            profile_key: ProfileKey {
                model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                engine_type: engine.into(),
                engine_version: Some("0.8.0".into()),
                platform: Some("nvidia-cuda".into()),
                gpu_name: Some("A100".into()),
                workload_id: "short-chat-v1".into(),
                param_fingerprint: None,
            },
            workload: builtin_workloads()[0].clone(),
            status: BenchmarkRunStatus::Succeeded,
            base_url: Some("http://127.0.0.1:8000".into()),
            image_id: Some(format!("{engine}-cuda")),
            software_version: Some("nebula-dev".into()),
            ttft_p50_ms: Some(ttft * 0.7),
            ttft_p95_ms: Some(ttft),
            tpot_p95_ms: Some(20.0),
            throughput_tps: Some(tps),
            error_rate: Some(0.0),
            peak_vram_mb: Some(8000),
            cost_per_1k_tokens: Some(0.5),
            started_at_ms: 1,
            finished_at_ms: 2,
            error_message: None,
            evidence_notes: None,
        }
    }

    #[test]
    fn recommend_returns_insufficient_without_profiles() {
        let req = RecommendRequest {
            model_name: "unknown".into(),
            workload_id: None,
            platform: None,
            ttft_p95_ms_max: None,
            throughput_tps_min: None,
            budget_cost_per_1k: None,
            max_candidates: None,
        };
        let resp = recommend_from_profiles(&req, &[], &[]);
        assert_eq!(resp.status, RecommendConfidence::InsufficientData);
        assert!(resp.candidates.is_empty());
        assert!(resp.message.unwrap().contains("insufficient"));
    }

    #[test]
    fn recommend_ranks_by_ttft() {
        let runs = vec![sample_run("sglang", 800.0, 40.0), sample_run("vllm", 500.0, 35.0)];
        let profiles: Vec<_> = runs
            .iter()
            .map(|r| build_profile_from_runs(&r.profile_key, &runs, 10))
            .collect();
        let req = RecommendRequest {
            model_name: "Qwen2.5".into(),
            workload_id: Some("short-chat-v1".into()),
            platform: Some("nvidia-cuda".into()),
            ttft_p95_ms_max: Some(2000.0),
            throughput_tps_min: None,
            budget_cost_per_1k: None,
            max_candidates: Some(5),
        };
        let resp = recommend_from_profiles(&req, &profiles, &runs);
        assert!(!resp.candidates.is_empty());
        assert_eq!(resp.candidates[0].engine_type, "vllm");
        assert!(!resp.candidates[0].evidence_run_ids.is_empty());
    }

    #[test]
    fn canary_rollback_on_breach() {
        assert!(canary_should_rollback(true, 10));
        assert!(!canary_should_rollback(false, 10));
        assert!(!canary_should_rollback(true, 0));
    }
}
