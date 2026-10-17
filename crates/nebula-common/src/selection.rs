//! L3 selection layer (Phase 1 skeleton).
//!
//! Builds on P5 `recommend_from_profiles` evidence; never invents a silent default engine.
//! Does not touch Gateway/Router hot path.

use serde::{Deserialize, Serialize};

use crate::benchmark::{
    recommend_from_profiles, PerformanceProfile, RecommendCandidate, RecommendConfidence,
    RecommendRequest, RecommendResponse,
};
use crate::model_deployment::{DesiredState, ModelDeployment};
use crate::model_request::ModelConfig;

/// Coarse model architecture for filtering / explanations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ModelArchitecture {
    Dense,
    MoE,
    #[default]
    Unknown,
}

/// Static/derived model traits used as selection input.
///
/// etcd (optional): `/model_profiles/{profile_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProfile {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_uid: Option<String>,
    /// HF id or local path label used to match PerformanceProfile keys.
    pub model_name: String,
    #[serde(default)]
    pub architecture: ModelArchitecture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_billions: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SelectionPreference {
    #[default]
    Latency,
    Throughput,
    Cost,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkloadHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_qps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_cost_per_1k: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_engines: Vec<String>,
    #[serde(default)]
    pub preference: SelectionPreference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_candidates: Option<u32>,
}

impl Default for SelectionConstraints {
    fn default() -> Self {
        Self {
            platform: None,
            ttft_p95_ms_max: None,
            throughput_tps_min: None,
            budget_cost_per_1k: None,
            allowed_engines: Vec::new(),
            preference: SelectionPreference::Latency,
            max_candidates: Some(5),
        }
    }
}

/// Current deployment snapshot for switching-cost calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CurrentBackend {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionRequest {
    pub model: ModelProfile,
    #[serde(default)]
    pub workload: WorkloadHint,
    #[serde(default)]
    pub constraints: SelectionConstraints,
    #[serde(default)]
    pub current: CurrentBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackendCandidate {
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub confidence: RecommendConfidence,
    /// 0.0 = no switch cost; higher = more expensive to migrate.
    pub switching_cost: f64,
    /// Final ranking score for the active preference (higher is better).
    #[serde(default)]
    pub score: f64,
    /// Human-readable score components for the console / API clients.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub score_breakdown: Vec<String>,
    pub evidence_run_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_1k_tokens: Option<f64>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionResponse {
    pub model_name: String,
    pub status: RecommendConfidence,
    pub candidates: Vec<BackendCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Unsigned deployment intent produced by selection (confirm via apply).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDraft {
    pub model_uid: String,
    pub model_name: String,
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    pub deployment: ModelDeployment,
    pub candidate: BackendCandidate,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftRequest {
    pub selection: SelectionRequest,
    /// Index into `SelectionResponse.candidates` (default 0).
    #[serde(default)]
    pub candidate_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
}

pub fn switching_cost(current: &CurrentBackend, candidate: &RecommendCandidate) -> f64 {
    let mut cost = 0.0;
    if let Some(ref cur) = current.engine_type {
        if !cur.eq_ignore_ascii_case(&candidate.engine_type) {
            cost += 0.5;
        }
    }
    match (&current.image_id, &candidate.image_id) {
        (Some(a), Some(b)) if a != b => cost += 0.3,
        (Some(_), None) => cost += 0.2,
        (None, Some(_)) => cost += 0.1,
        _ => {}
    }
    if let (Some(ref a), Some(ref b)) = (&current.platform, &candidate.platform) {
        if a != b {
            cost += 0.2;
        }
    }
    cost
}

pub const ROOFLINE_COMPUTE_BOUND_FP8_REASON: &str =
    "concurrency >= 16 enters compute-bound regime where FP8 matches 4-bit decode throughput without 4-bit prefill unpack penalty";
pub const ROOFLINE_MEMORY_BOUND_4BIT_REASON: &str =
    "concurrency <= 4 is memory-bandwidth-bound where 4-bit quantization delivers ~20-27% decode speedup";
pub const ROOFLINE_NATIVE_W4A4_REASON: &str =
    "native W4A4/NVFP4 hardware path preserves 4-bit speedup without register unpack penalty";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuantBucket {
    FourBit,
    Fp8,
    Other,
}

fn classify_quantization(
    model_quant: Option<&str>,
    candidate: &BackendCandidate,
) -> (QuantBucket, bool) {
    let combined = format!(
        "{} {} {} {}",
        model_quant.unwrap_or(""),
        candidate.image_id.as_deref().unwrap_or(""),
        candidate.platform.as_deref().unwrap_or(""),
        candidate.reasons.join(" ")
    )
    .to_ascii_lowercase();

    let native_w4a4 = combined.contains("nvfp4")
        || combined.contains("w4a4")
        || combined.contains("b200")
        || combined.contains("gb200")
        || combined.contains("gb300");

    if combined.contains("fp8") || combined.contains("w8a8") || combined.contains("int8") {
        (QuantBucket::Fp8, native_w4a4)
    } else if combined.contains("4bit")
        || combined.contains("4-bit")
        || combined.contains("awq")
        || combined.contains("gptq")
        || combined.contains("int4")
        || combined.contains("w4a16")
        || combined.contains("nvfp4")
        || combined.contains("fp4")
    {
        (QuantBucket::FourBit, native_w4a4)
    } else {
        (QuantBucket::Other, native_w4a4)
    }
}

/// Roofline quantization score bonus and explainable reason (Batch 5.5):
/// - `concurrency <= 4` or native W4A4 (`NVFP4`): +15.0 bonus for 4-bit candidates.
/// - `concurrency >= 16` without native W4A4: +15.0 bonus for FP8 candidates (and -5.0 unpack
///   penalty on non-native 4-bit candidates, plus the explainable Roofline reason on both).
fn roofline_quantization_adjustment(
    req: &SelectionRequest,
    candidate: &BackendCandidate,
) -> (f64, Option<&'static str>) {
    let (bucket, native_w4a4) = classify_quantization(req.model.quantization.as_deref(), candidate);
    let Some(conc) = req.workload.concurrency else {
        if native_w4a4 && bucket == QuantBucket::FourBit {
            return (15.0, Some(ROOFLINE_NATIVE_W4A4_REASON));
        }
        return (0.0, None);
    };

    if native_w4a4 && bucket == QuantBucket::FourBit {
        return (15.0, Some(ROOFLINE_NATIVE_W4A4_REASON));
    }

    if conc <= 4 {
        match bucket {
            QuantBucket::FourBit => (15.0, Some(ROOFLINE_MEMORY_BOUND_4BIT_REASON)),
            _ => (0.0, None),
        }
    } else if conc >= 16 {
        match bucket {
            QuantBucket::Fp8 => (15.0, Some(ROOFLINE_COMPUTE_BOUND_FP8_REASON)),
            QuantBucket::FourBit => (-5.0, Some(ROOFLINE_COMPUTE_BOUND_FP8_REASON)),
            QuantBucket::Other => (0.0, Some(ROOFLINE_COMPUTE_BOUND_FP8_REASON)),
        }
    } else {
        (0.0, None)
    }
}

fn score_candidate(pref: SelectionPreference, c: &BackendCandidate, roofline_bonus: f64) -> f64 {
    let switch_penalty = c.switching_cost * 10.0;
    let base = match pref {
        SelectionPreference::Latency => {
            let ttft = c.ttft_p95_ms.unwrap_or(f64::MAX);
            -(ttft + switch_penalty)
        }
        SelectionPreference::Throughput => {
            let tps = c.throughput_tps.unwrap_or(0.0);
            tps - switch_penalty
        }
        SelectionPreference::Cost => {
            let cost = c.cost_per_1k_tokens.unwrap_or(f64::MAX);
            -(cost + switch_penalty)
        }
    };
    base + roofline_bonus
}

fn preference_label(pref: SelectionPreference) -> &'static str {
    match pref {
        SelectionPreference::Latency => "latency",
        SelectionPreference::Throughput => "throughput",
        SelectionPreference::Cost => "cost",
    }
}

fn build_score_breakdown(
    pref: SelectionPreference,
    c: &BackendCandidate,
    roofline_bonus: f64,
    score: f64,
) -> Vec<String> {
    let switch_penalty = c.switching_cost * 10.0;
    let mut parts = vec![format!("preference={}", preference_label(pref))];
    match pref {
        SelectionPreference::Latency => {
            parts.push(format!(
                "ttft_p95={}",
                c.ttft_p95_ms
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "missing".into())
            ));
        }
        SelectionPreference::Throughput => {
            parts.push(format!(
                "throughput_tps={}",
                c.throughput_tps
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "missing".into())
            ));
        }
        SelectionPreference::Cost => {
            parts.push(format!(
                "cost_per_1k={}",
                c.cost_per_1k_tokens
                    .map(|v| format!("{v:.4}"))
                    .unwrap_or_else(|| "missing".into())
            ));
        }
    }
    if roofline_bonus.abs() > f64::EPSILON {
        parts.push(format!("roofline_bonus={roofline_bonus:.1}"));
    }
    parts.push(format!("switch_penalty={switch_penalty:.1}"));
    parts.push(format!("switching_cost={:.2}", c.switching_cost));
    parts.push(format!("score={score:.2}"));
    parts
}

fn annotate_scores(req: &SelectionRequest, candidates: &mut [BackendCandidate]) {
    let pref = req.constraints.preference;
    for c in candidates.iter_mut() {
        let (roofline_bonus, roofline_reason) = roofline_quantization_adjustment(req, c);
        let score = score_candidate(pref, c, roofline_bonus);
        c.score = score;
        c.score_breakdown = build_score_breakdown(pref, c, roofline_bonus, score);
        let pref_reason = format!("preference={}", preference_label(pref));
        if !c.reasons.iter().any(|r| r == &pref_reason) {
            c.reasons.insert(0, pref_reason);
        }
        if let Some(reason) = roofline_reason {
            if !c.reasons.iter().any(|r| r == reason) {
                c.reasons.push(reason.to_string());
            }
        }
    }
}

fn to_backend_candidate(current: &CurrentBackend, c: RecommendCandidate) -> BackendCandidate {
    let cost = switching_cost(current, &c);
    let mut reasons = vec![c.rationale];
    if cost > 0.0 {
        reasons.push(format!("switching_cost={cost:.2}"));
    }
    BackendCandidate {
        engine_type: c.engine_type,
        engine_version: c.engine_version,
        image_id: c.image_id,
        platform: c.platform,
        confidence: c.confidence,
        switching_cost: cost,
        score: 0.0,
        score_breakdown: Vec::new(),
        evidence_run_ids: c.evidence_run_ids,
        ttft_p95_ms: c.ttft_p95_ms,
        throughput_tps: c.throughput_tps,
        cost_per_1k_tokens: c.cost_per_1k_tokens,
        reasons,
    }
}

/// Select backends from profiles. Never invents engines without evidence.
pub fn select_backends(
    req: &SelectionRequest,
    profiles: &[PerformanceProfile],
    runs: &[crate::benchmark::BenchmarkRun],
) -> SelectionResponse {
    let recommend_req = RecommendRequest {
        model_name: req.model.model_name.clone(),
        workload_id: req.workload.workload_id.clone(),
        platform: req.constraints.platform.clone(),
        ttft_p95_ms_max: req.constraints.ttft_p95_ms_max,
        throughput_tps_min: req.constraints.throughput_tps_min,
        budget_cost_per_1k: req.constraints.budget_cost_per_1k,
        max_candidates: req.constraints.max_candidates.or(Some(16)),
    };
    let RecommendResponse {
        model_name,
        status,
        candidates,
        message,
    } = recommend_from_profiles(&recommend_req, profiles, runs);

    let allowed = &req.constraints.allowed_engines;
    let mut out: Vec<BackendCandidate> = candidates
        .into_iter()
        .filter(|c| {
            allowed.is_empty()
                || allowed
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&c.engine_type))
        })
        .map(|c| to_backend_candidate(&req.current, c))
        .collect();

    let pref = req.constraints.preference;
    out.sort_by(|a, b| {
        let (ba, _) = roofline_quantization_adjustment(req, a);
        let (bb, _) = roofline_quantization_adjustment(req, b);
        score_candidate(pref, b, bb)
            .partial_cmp(&score_candidate(pref, a, ba))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let max_k = req.constraints.max_candidates.unwrap_or(5) as usize;
    if out.len() > max_k {
        out.truncate(max_k);
    }
    annotate_scores(req, &mut out);

    let status = if out.is_empty() {
        RecommendConfidence::InsufficientData
    } else {
        status
    };

    SelectionResponse {
        model_name,
        status,
        candidates: out,
        message: if status == RecommendConfidence::InsufficientData {
            Some(message.unwrap_or_else(|| {
                "insufficient_data: no matching benchmark profiles for constraints".into()
            }))
        } else {
            message
        },
    }
}

pub fn draft_from_candidate(
    req: &DraftRequest,
    candidate: &BackendCandidate,
    now_ms: u64,
) -> Result<DeploymentDraft, String> {
    if candidate.confidence == RecommendConfidence::InsufficientData {
        return Err("cannot draft from insufficient_data candidate".into());
    }
    let model_uid = req
        .model_uid
        .clone()
        .or_else(|| req.selection.model.model_uid.clone())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "model_uid required for deployment draft".to_string())?;

    let replicas = req.replicas.unwrap_or(1).max(1);
    let deployment = ModelDeployment {
        model_uid: model_uid.clone(),
        desired_state: DesiredState::Running,
        replicas,
        min_replicas: None,
        max_replicas: None,
        node_affinity: None,
        gpu_affinity: None,
        replica_specs: None,
        config_overrides: Some(ModelConfig {
            tensor_parallel_size: Some(1),
            gpu_memory_utilization: None,
            max_model_len: req.selection.model.context_length,
            required_vram_mb: None,
            lora_modules: None,
            ..Default::default()
        }),
        image_id: candidate.image_id.clone(),
        allowed_pools: None,
        image_override_reason: Some("selection_draft".into()),
        compat_rule_ids: Vec::new(),
        topology: None,
        kv_sharing: None,
        version: 1,
        updated_at_ms: now_ms,
    };

    Ok(DeploymentDraft {
        model_uid,
        model_name: req.selection.model.model_name.clone(),
        engine_type: candidate.engine_type.clone(),
        image_id: candidate.image_id.clone(),
        deployment,
        candidate: candidate.clone(),
        note: "semi-automatic draft: call selection/apply to write /deployments/".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::{BenchmarkRun, ProfileKey};

    fn sample_profile(engine: &str) -> PerformanceProfile {
        PerformanceProfile {
            profile_key: ProfileKey {
                model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                workload_id: "short-chat-v1".into(),
                engine_type: engine.into(),
                engine_version: Some("0.1".into()),
                platform: Some("nvidia".into()),
                gpu_name: None,
                param_fingerprint: None,
            },
            run_ids: vec![format!("r-{engine}")],
            best_ttft_p95_ms: Some(80.0),
            best_throughput_tps: Some(120.0),
            median_error_rate: Some(0.0),
            sample_count: 3,
            updated_at_ms: 1,
        }
    }

    fn sample_run(engine: &str) -> BenchmarkRun {
        use crate::benchmark::{BenchmarkRunStatus, BenchmarkWorkload, WorkloadClass};
        BenchmarkRun {
            run_id: format!("r-{engine}"),
            profile_key: ProfileKey {
                model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                workload_id: "short-chat-v1".into(),
                engine_type: engine.into(),
                engine_version: Some("0.1".into()),
                platform: Some("nvidia".into()),
                gpu_name: None,
                param_fingerprint: None,
            },
            workload: BenchmarkWorkload {
                id: "short-chat-v1".into(),
                class: WorkloadClass::ShortChat,
                prompt: "hi".into(),
                max_tokens: 16,
                concurrency: 1,
                request_count: 1,
                notes: None,
            },
            status: BenchmarkRunStatus::Succeeded,
            base_url: None,
            image_id: Some(format!("{engine}-img")),
            software_version: None,
            ttft_p50_ms: Some(40.0),
            ttft_p95_ms: Some(80.0),
            tpot_p95_ms: None,
            throughput_tps: Some(120.0),
            error_rate: Some(0.0),
            peak_vram_mb: None,
            cost_per_1k_tokens: None,
            started_at_ms: 1,
            finished_at_ms: 2,
            error_message: None,
            evidence_notes: None,
        }
    }

    #[test]
    fn insufficient_without_profiles() {
        let req = SelectionRequest {
            model: ModelProfile {
                profile_id: "p1".into(),
                model_uid: Some("qwen".into()),
                model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                architecture: ModelArchitecture::Dense,
                parameter_billions: Some(0.5),
                quantization: None,
                context_length: Some(4096),
                updated_at_ms: 1,
            },
            workload: WorkloadHint {
                workload_id: Some("short-chat-v1".into()),
                concurrency: None,
                target_qps: None,
            },
            constraints: SelectionConstraints::default(),
            current: CurrentBackend::default(),
        };
        let resp = select_backends(&req, &[], &[]);
        assert_eq!(resp.status, RecommendConfidence::InsufficientData);
        assert!(resp.candidates.is_empty());
    }

    #[test]
    fn ranks_and_applies_switching_cost() {
        let profiles = vec![sample_profile("vllm"), sample_profile("sglang")];
        let runs = vec![sample_run("vllm"), sample_run("sglang")];
        let req = SelectionRequest {
            model: ModelProfile {
                profile_id: "p1".into(),
                model_uid: Some("qwen".into()),
                model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                architecture: ModelArchitecture::Dense,
                parameter_billions: Some(0.5),
                quantization: None,
                context_length: Some(4096),
                updated_at_ms: 1,
            },
            workload: WorkloadHint {
                workload_id: Some("short-chat-v1".into()),
                concurrency: None,
                target_qps: None,
            },
            constraints: SelectionConstraints {
                preference: SelectionPreference::Latency,
                max_candidates: Some(5),
                ..Default::default()
            },
            current: CurrentBackend {
                engine_type: Some("vllm".into()),
                image_id: Some("vllm-img".into()),
                platform: Some("nvidia".into()),
            },
        };
        let resp = select_backends(&req, &profiles, &runs);
        assert!(!resp.candidates.is_empty());
        let vllm = resp
            .candidates
            .iter()
            .find(|c| c.engine_type == "vllm")
            .expect("vllm");
        let sglang = resp
            .candidates
            .iter()
            .find(|c| c.engine_type == "sglang")
            .expect("sglang");
        assert!(vllm.switching_cost < sglang.switching_cost);
        assert!(vllm.switching_cost <= 0.2);
        assert!(sglang
            .score_breakdown
            .iter()
            .any(|s| s.starts_with("switch_penalty=")));
        assert!(sglang
            .score_breakdown
            .iter()
            .any(|s| s.contains("switching_cost=")));
        assert!(vllm.reasons.iter().any(|r| r == "preference=latency"));
        assert!(vllm.score >= sglang.score);
    }

    #[test]
    fn draft_requires_model_uid() {
        let candidate = BackendCandidate {
            engine_type: "vllm".into(),
            engine_version: None,
            image_id: Some("img".into()),
            platform: Some("nvidia".into()),
            confidence: RecommendConfidence::High,
            switching_cost: 0.0,
            score: 0.0,
            score_breakdown: vec![],
            evidence_run_ids: vec![],
            ttft_p95_ms: Some(10.0),
            throughput_tps: Some(1.0),
            cost_per_1k_tokens: None,
            reasons: vec!["ok".into()],
        };
        let draft_req = DraftRequest {
            selection: SelectionRequest {
                model: ModelProfile {
                    profile_id: "p".into(),
                    model_uid: None,
                    model_name: "m".into(),
                    architecture: ModelArchitecture::Unknown,
                    parameter_billions: None,
                    quantization: None,
                    context_length: None,
                    updated_at_ms: 0,
                },
                workload: WorkloadHint {
                    workload_id: None,
                    concurrency: None,
                    target_qps: None,
                },
                constraints: SelectionConstraints::default(),
                current: CurrentBackend::default(),
            },
            candidate_index: 0,
            model_uid: None,
            replicas: Some(1),
        };
        assert!(draft_from_candidate(&draft_req, &candidate, 1).is_err());
        let ok = draft_from_candidate(
            &DraftRequest {
                model_uid: Some("qwen".into()),
                ..draft_req
            },
            &candidate,
            1,
        )
        .unwrap();
        assert_eq!(ok.deployment.model_uid, "qwen");
        assert_eq!(ok.engine_type, "vllm");
    }

    #[test]
    fn roofline_prefers_4bit_at_low_concurrency_and_fp8_at_high_concurrency() {
        let mut p_4bit = sample_profile("vllm");
        p_4bit.profile_key.param_fingerprint = Some("awq-4bit".into());
        p_4bit.run_ids = vec!["r-4bit".into()];

        let mut p_fp8 = sample_profile("sglang");
        p_fp8.profile_key.param_fingerprint = Some("fp8".into());
        p_fp8.run_ids = vec!["r-fp8".into()];

        let mut r_4bit = sample_run("vllm");
        r_4bit.run_id = "r-4bit".into();
        r_4bit.profile_key.param_fingerprint = Some("awq-4bit".into());
        r_4bit.image_id = Some("vllm-awq-4bit".into());

        let mut r_fp8 = sample_run("sglang");
        r_fp8.run_id = "r-fp8".into();
        r_fp8.profile_key.param_fingerprint = Some("fp8".into());
        r_fp8.image_id = Some("sglang-fp8".into());

        let profiles = vec![p_4bit, p_fp8];
        let runs = vec![r_4bit, r_fp8];

        let base_model = ModelProfile {
            profile_id: "p1".into(),
            model_uid: Some("qwen3.8-27b".into()),
            model_name: "Qwen/Qwen2.5-0.5B-Instruct".into(),
            architecture: ModelArchitecture::Dense,
            parameter_billions: Some(27.0),
            quantization: None,
            context_length: Some(32768),
            updated_at_ms: 1,
        };

        // Concurrency <= 4 (memory-bound): 4-bit gets +15 bonus and ranks first
        let low_conc = select_backends(
            &SelectionRequest {
                model: base_model.clone(),
                workload: WorkloadHint {
                    workload_id: Some("short-chat-v1".into()),
                    concurrency: Some(1),
                    target_qps: None,
                },
                constraints: SelectionConstraints {
                    preference: SelectionPreference::Throughput,
                    ..Default::default()
                },
                current: CurrentBackend::default(),
            },
            &profiles,
            &runs,
        );
        assert_eq!(
            low_conc.candidates[0].image_id.as_deref(),
            Some("vllm-awq-4bit")
        );
        assert!(low_conc.candidates[0]
            .reasons
            .iter()
            .any(|r| r == ROOFLINE_MEMORY_BOUND_4BIT_REASON));

        // Concurrency >= 16 (compute-bound): FP8 gets +15 bonus, 4-bit gets -5 unpack penalty
        let high_conc = select_backends(
            &SelectionRequest {
                model: base_model,
                workload: WorkloadHint {
                    workload_id: Some("short-chat-v1".into()),
                    concurrency: Some(16),
                    target_qps: None,
                },
                constraints: SelectionConstraints {
                    preference: SelectionPreference::Throughput,
                    ..Default::default()
                },
                current: CurrentBackend::default(),
            },
            &profiles,
            &runs,
        );
        assert_eq!(
            high_conc.candidates[0].image_id.as_deref(),
            Some("sglang-fp8")
        );
        assert!(high_conc.candidates[0]
            .reasons
            .iter()
            .any(|r| r == ROOFLINE_COMPUTE_BOUND_FP8_REASON));
    }
}
