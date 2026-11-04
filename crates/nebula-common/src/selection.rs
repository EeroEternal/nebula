//! L3 selection layer (Phase 1 skeleton).
//!
//! Builds on P5 `recommend_from_profiles` evidence; never invents a silent default engine.
//! Does not touch Gateway/Router hot path.

use serde::{Deserialize, Serialize};

use crate::benchmark::{
    recommend_from_profiles, BenchmarkRun, PerformanceProfile, RecommendCandidate,
    RecommendConfidence, RecommendRequest, RecommendResponse,
};
use crate::capability::{QuantFormat, SupportLevel};
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    /// Sliding-window attention bound `W` (`kSlidingWindow` in TokenSpeed CapacityModel).
    /// When set, attention KV footprint per request caps at `min(context_len, W)` tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding_window_tokens: Option<u32>,
    /// Fraction `[0.0, 1.0]` of layers using recurrent/linear state (`kState`, e.g.
    /// Mamba2 or Qwen3.5 Gated DeltaNet), which occupy a constant 2-slot state checkpoint
    /// per active request regardless of sequence length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_attention_layer_ratio: Option<f64>,
    /// DeepSeek MLA compressed KV latent rank (e.g. 512), reducing per-token KV footprint
    /// relative to uncompressed GQA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_lora_rank: Option<u32>,
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
    "uncalibrated Roofline heuristic: concurrency >= 16 favors a declared native 8-bit path";
pub const ROOFLINE_MEMORY_BOUND_4BIT_REASON: &str =
    "uncalibrated Roofline heuristic: concurrency <= 4 favors a declared native weight-only 4-bit path";
pub const ROOFLINE_NATIVE_W4A4_REASON: &str =
    "uncalibrated Roofline heuristic: this run declares an active native W4A4 path";
pub const ROOFLINE_UNPACK_REASON: &str =
    "uncalibrated Roofline heuristic: concurrency >= 16, active W4A16 and explicitly unsupported W4A4";

/// Estimate total KV-cache / recurrent-state working-set VRAM (MB) for `concurrency`
/// active sequences at `context_len` tokens, modeling the three attention cache kinds
/// (`kFull`, `kSlidingWindow`, and 2-slot `kState` checkpoint) plus MLA latent rank compression.
pub fn estimate_kv_cache_vram_mb(
    profile: &ModelProfile,
    concurrency: u32,
    context_len: u32,
) -> f64 {
    if concurrency == 0 || context_len == 0 {
        return 0.0;
    }
    let params_b = profile.parameter_billions.unwrap_or(7.0).max(0.5);
    let quant_lower = profile
        .quantization
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let dtype_factor = if quant_lower.contains("fp8") || quant_lower.contains("int8") {
        0.5
    } else {
        1.0
    };
    let mla_factor = profile
        .kv_lora_rank
        .filter(|&r| r > 0)
        .map(|r| (r as f64 / 2048.0).clamp(0.15, 1.0))
        .unwrap_or(1.0);
    // Empirical GQA BF16 baseline: ~0.018 MB per token per 1B parameters.
    let mb_per_token = 0.018 * params_b * dtype_factor * mla_factor;

    let linear_ratio = profile
        .linear_attention_layer_ratio
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let attn_ratio = 1.0 - linear_ratio;

    let effective_attn_tokens = match profile.sliding_window_tokens.filter(|&w| w > 0) {
        Some(w) => context_len.min(w) as f64,
        None => context_len as f64,
    };
    // Recurrent state layers (`kState`) hold a fixed 2-slot state checkpoint
    // (~128 token-equivalents per sequence) independent of sequence length.
    const STATE_CHECKPOINT_TOKEN_EQUIV: f64 = 128.0;

    let per_request_mb = mb_per_token
        * (attn_ratio * effective_attn_tokens + linear_ratio * STATE_CHECKPOINT_TOKEN_EQUIV);
    per_request_mb * (concurrency as f64)
}

fn declared_quantization(
    req: &SelectionRequest,
    candidate: &BackendCandidate,
    runs: &[BenchmarkRun],
) -> Option<(QuantFormat, SupportLevel)> {
    for scope in [
        &candidate.image_id,
        &candidate.platform,
        &candidate.engine_version,
    ] {
        scope.as_deref().filter(|value| !value.trim().is_empty())?;
    }
    let mut declaration = None;
    let mut profile_key = None;
    let by_id: std::collections::HashMap<_, _> =
        runs.iter().map(|run| (run.run_id.as_str(), run)).collect();
    if by_id.len() != runs.len() {
        return None;
    }
    // Every evidence run must agree. Missing/contradictory declarations are unknown;
    // never borrow one platform's declaration or a live replica's unrelated snapshot.
    for id in &candidate.evidence_run_ids {
        let run = by_id.get(id.as_str())?;
        if !run.usable_for_selection()
            || run.profile_key.model_name != req.model.model_name
            || run.profile_key.engine_type != candidate.engine_type
            || run.profile_key.engine_version != candidate.engine_version
            || run.profile_key.platform != candidate.platform
            || run.image_id != candidate.image_id
            || profile_key.is_some_and(|key| key != &run.profile_key)
        {
            return None;
        }
        profile_key = Some(&run.profile_key);
        let capability = run.evidence.as_ref()?.engine_capability.as_ref()?;
        if capability.engine_type != run.profile_key.engine_type
            || capability.engine_version != run.profile_key.engine_version
        {
            return None;
        }
        let format = capability.quantization?;
        if capability.native_low_precision_paths.get(&format) != Some(&SupportLevel::Supported) {
            return None;
        }
        let native_w4a4 = capability
            .native_low_precision_paths
            .get(&QuantFormat::W4A4)
            .copied()
            .unwrap_or(SupportLevel::Unknown);
        let current = (format, native_w4a4);
        if declaration.is_some_and(|previous| previous != current) {
            return None;
        }
        declaration = Some(current);
    }
    declaration
}

/// Uncalibrated ranking policy, gated on run-scoped declarations, never chip names.
/// Unknown format/path is neutral; an absent W4A4 entry cannot prove an unpack penalty.
/// Cost retains explanations but never adds a latency/throughput constant.
fn roofline_quantization_adjustment(
    req: &SelectionRequest,
    candidate: &BackendCandidate,
    runs: &[BenchmarkRun],
) -> (f64, Option<&'static str>) {
    let Some((format, native_w4a4)) = declared_quantization(req, candidate, runs) else {
        return (0.0, None);
    };
    let (raw_bonus, reason) = match (format, req.workload.concurrency) {
        (QuantFormat::W4A4, _) => (15.0, Some(ROOFLINE_NATIVE_W4A4_REASON)),
        (QuantFormat::W4A16, Some(1..=4)) => (15.0, Some(ROOFLINE_MEMORY_BOUND_4BIT_REASON)),
        (QuantFormat::Fp8 | QuantFormat::W8A8 | QuantFormat::Int8, Some(16..)) => {
            (15.0, Some(ROOFLINE_COMPUTE_BOUND_FP8_REASON))
        }
        (QuantFormat::W4A16, Some(16..)) if native_w4a4 == SupportLevel::Unsupported => {
            (-5.0, Some(ROOFLINE_UNPACK_REASON))
        }
        _ => (0.0, None),
    };
    let effective_bonus = match req.constraints.preference {
        SelectionPreference::Cost => 0.0,
        SelectionPreference::Latency | SelectionPreference::Throughput => raw_bonus,
    };
    (effective_bonus, reason)
}

/// Shared by scoring and explanations. Cost uses a relative migration premium
/// (issue #88): the 0..1 switching heuristic adds at most 10% of the candidate's
/// measured cost, not a fixed surcharge in unrelated units. This is a ranking
/// policy, not an estimate of actual migration expense.
fn switch_penalty(pref: SelectionPreference, c: &BackendCandidate) -> f64 {
    match pref {
        SelectionPreference::Cost => c
            .cost_per_1k_tokens
            .map(|cost| cost * c.switching_cost * 0.1)
            .unwrap_or(0.0),
        SelectionPreference::Latency | SelectionPreference::Throughput => c.switching_cost * 10.0,
    }
}

fn score_candidate(pref: SelectionPreference, c: &BackendCandidate, roofline_bonus: f64) -> f64 {
    let switch_penalty = switch_penalty(pref, c);
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
    let switch_penalty = switch_penalty(pref, c);
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
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "missing".into())
            ));
        }
    }
    if roofline_bonus.abs() > f64::EPSILON {
        parts.push(format!("roofline_bonus={roofline_bonus:.1}"));
    }
    parts.push(format!("switch_penalty={switch_penalty}"));
    parts.push(format!("switching_cost={:.2}", c.switching_cost));
    parts.push(format!("score={score}"));
    parts
}

fn annotate_scores(
    req: &SelectionRequest,
    candidates: &mut [BackendCandidate],
    runs: &[BenchmarkRun],
) {
    let pref = req.constraints.preference;
    let has_hybrid_kv_geometry = req.model.sliding_window_tokens.is_some()
        || req.model.linear_attention_layer_ratio.is_some()
        || req.model.kv_lora_rank.is_some();
    let kv_reason = if has_hybrid_kv_geometry {
        let conc = req.workload.concurrency.unwrap_or(1).max(1);
        let ctx = req.model.context_length.unwrap_or(4096).max(1);
        let vram_mb = estimate_kv_cache_vram_mb(&req.model, conc, ctx);
        Some(format!("estimated_kv_vram_mb={vram_mb:.1}"))
    } else {
        None
    };
    for c in candidates.iter_mut() {
        let (roofline_bonus, roofline_reason) = roofline_quantization_adjustment(req, c, runs);
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
        if let Some(ref kv_r) = kv_reason {
            if !c.reasons.iter().any(|r| r == kv_r) {
                c.reasons.push(kv_r.clone());
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

    annotate_scores(req, &mut out, runs);
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let max_k = req.constraints.max_candidates.unwrap_or(5) as usize;
    if out.len() > max_k {
        out.truncate(max_k);
    }

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
            evidence: None,
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
                ..Default::default()
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
                ..Default::default()
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
                    ..Default::default()
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

    fn sample_candidate() -> BackendCandidate {
        BackendCandidate {
            engine_type: "vllm".into(),
            engine_version: None,
            image_id: None,
            platform: None,
            confidence: RecommendConfidence::High,
            switching_cost: 0.25,
            score: 0.0,
            score_breakdown: vec![],
            evidence_run_ids: vec![],
            ttft_p95_ms: Some(800.0),
            throughput_tps: Some(42.0),
            cost_per_1k_tokens: Some(0.004),
            reasons: vec![],
        }
    }

    fn breakdown_value(parts: &[String], key: &str) -> f64 {
        parts
            .iter()
            .find_map(|part| part.strip_prefix(key))
            .expect("component in breakdown")
            .parse()
            .expect("numeric component")
    }

    #[test]
    fn cost_preference_savings_outweigh_image_and_engine_switches() {
        let req = SelectionRequest {
            model: ModelProfile {
                model_name: sample_profile("vllm").profile_key.model_name,
                ..Default::default()
            },
            workload: WorkloadHint::default(),
            constraints: SelectionConstraints {
                preference: SelectionPreference::Cost,
                ..Default::default()
            },
            current: CurrentBackend {
                engine_type: Some("vllm".into()),
                image_id: Some("vllm-img".into()),
                platform: Some("nvidia".into()),
            },
        };
        for engine in ["vllm", "sglang"] {
            let current_profile = sample_profile("vllm");
            let mut other_profile = sample_profile(engine);
            other_profile.profile_key.param_fingerprint = Some("alternative".into());
            other_profile.run_ids = vec!["r-alternative".into()];
            let profiles = [current_profile, other_profile];
            let mut current_run = sample_run("vllm");
            current_run.cost_per_1k_tokens = Some(0.008);
            let mut other_run = sample_run(engine);
            other_run.run_id = "r-alternative".into();
            other_run.profile_key = profiles[1].profile_key.clone();
            // Isolate an image change, then an engine change with the same image.
            other_run.image_id = Some(if engine == "vllm" {
                "alternative-img".into()
            } else {
                "vllm-img".into()
            });

            let mut previous_margin = f64::NEG_INFINITY;
            for cost in [0.008, 0.004, 0.002] {
                other_run.cost_per_1k_tokens = Some(cost);
                let response =
                    select_backends(&req, &profiles, &[current_run.clone(), other_run.clone()]);
                assert_eq!(response.candidates.len(), 2);
                let current = response
                    .candidates
                    .iter()
                    .find(|c| c.evidence_run_ids == ["r-vllm"])
                    .unwrap();
                let other = response
                    .candidates
                    .iter()
                    .find(|c| c.evidence_run_ids == ["r-alternative"])
                    .unwrap();
                let margin = other.score - current.score;
                assert!(margin > previous_margin);
                previous_margin = margin;
                let expected = if cost == 0.008 {
                    // Equal costs still prefer avoiding a migration.
                    "r-vllm"
                } else {
                    "r-alternative"
                };
                assert_eq!(response.candidates[0].evidence_run_ids, [expected]);
            }
        }
    }

    #[test]
    fn cost_switch_penalty_is_relative_and_preserves_breakdown_precision() {
        let mut candidate = sample_candidate();
        for cost in [0.0, 1e-8, 0.004, 1.0, 1000.0] {
            candidate.cost_per_1k_tokens = Some(cost);
            for switching_cost in [0.0, 0.3, 0.5, 0.8, 1.0] {
                candidate.switching_cost = switching_cost;
                let score = score_candidate(SelectionPreference::Cost, &candidate, 0.0);
                let parts =
                    build_score_breakdown(SelectionPreference::Cost, &candidate, 0.0, score);
                let penalty = breakdown_value(&parts, "switch_penalty=");
                let expected = cost * switching_cost * 0.1;
                let tolerance = cost * 1e-12;
                assert!(
                    (penalty - expected).abs() <= tolerance,
                    "cost={cost}, switching_cost={switching_cost}, penalty={penalty}"
                );
                assert!(penalty <= cost * 0.1 + tolerance);
                assert_eq!(breakdown_value(&parts, "cost_per_1k="), cost);
                assert_eq!(breakdown_value(&parts, "score="), score);
                assert_eq!(-(cost + penalty), score);
            }
        }
    }

    #[test]
    fn cost_preference_missing_cost_stays_worse_than_measured_cost() {
        let mut candidate = sample_candidate();
        candidate.cost_per_1k_tokens = None;
        let unknown = score_candidate(SelectionPreference::Cost, &candidate, 0.0);
        assert!(unknown.is_finite());
        candidate.cost_per_1k_tokens = Some(0.004);
        let measured = score_candidate(SelectionPreference::Cost, &candidate, 0.0);
        assert!(measured > unknown);
    }

    #[test]
    fn score_breakdown_terms_reconstruct_score_for_every_preference() {
        let c = sample_candidate();
        for pref in [
            SelectionPreference::Latency,
            SelectionPreference::Throughput,
            SelectionPreference::Cost,
        ] {
            let score = score_candidate(pref, &c, 0.0);
            let parts = build_score_breakdown(pref, &c, 0.0, score);
            let printed_penalty = breakdown_value(&parts, "switch_penalty=");
            match pref {
                SelectionPreference::Latency => assert_eq!(score, -802.5),
                SelectionPreference::Throughput => assert_eq!(score, 39.5),
                SelectionPreference::Cost => {}
            }
            let reconstructed = match pref {
                SelectionPreference::Latency => -(800.0 + printed_penalty),
                SelectionPreference::Throughput => 42.0 - printed_penalty,
                SelectionPreference::Cost => -(0.004 + printed_penalty),
            };
            assert!(
                (score - reconstructed).abs() < 1e-6,
                "{pref:?}: score={score} but breakdown reconstructs {reconstructed}"
            );
        }
    }

    fn roofline_request(concurrency: u32, preference: SelectionPreference) -> SelectionRequest {
        SelectionRequest {
            model: ModelProfile {
                model_name: sample_profile("vllm").profile_key.model_name,
                quantization: Some("awq".into()),
                ..Default::default()
            },
            workload: WorkloadHint {
                concurrency: Some(concurrency),
                ..Default::default()
            },
            constraints: SelectionConstraints {
                preference,
                ..Default::default()
            },
            current: CurrentBackend::default(),
        }
    }

    fn declare_quantization(run: &mut BenchmarkRun, format: QuantFormat) {
        let mut capability =
            crate::capability::static_capability(&run.profile_key.engine_type).unwrap();
        capability.engine_version = run.profile_key.engine_version.clone();
        capability.source = crate::capability::CapabilitySource::ManualOverride;
        capability.quantization = Some(format);
        capability
            .native_low_precision_paths
            .insert(format, SupportLevel::Supported);
        run.evidence = Some(crate::benchmark::BenchmarkEvidence {
            schema_version: 2,
            wall_time_s: 1.0,
            request_count: 1,
            successful_requests: 1,
            engine_capability: Some(capability),
            ..Default::default()
        });
    }

    fn select_roofline_runs(req: &SelectionRequest, runs: &[BenchmarkRun]) -> SelectionResponse {
        let profile = crate::benchmark::build_profile_from_runs(&runs[0].profile_key, runs, 10);
        select_backends(req, &[profile], runs)
    }

    #[test]
    fn roofline_unknown_capabilities_ignore_platform_and_image_names() {
        for platform in ["ascend-cann8", "nvidia-b200", "nvidia-gb300", "amd-rocm"] {
            for concurrency in [1, 16] {
                let mut run = sample_run("vllm");
                run.profile_key.platform = Some(platform.into());
                run.image_id = Some("vllm-nvfp4-awq-4bit".into());
                let profile = crate::benchmark::build_profile_from_runs(
                    &run.profile_key,
                    std::slice::from_ref(&run),
                    10,
                );
                let req = roofline_request(concurrency, SelectionPreference::Throughput);
                let response = select_backends(&req, &[profile], &[run]);
                let candidate = &response.candidates[0];
                assert_eq!(
                    candidate.score,
                    score_candidate(req.constraints.preference, candidate, 0.0)
                );
                assert!(!candidate
                    .score_breakdown
                    .iter()
                    .any(|part| part.starts_with("roofline_bonus=")));
                assert!(!candidate.reasons.iter().any(|reason| [
                    ROOFLINE_MEMORY_BOUND_4BIT_REASON,
                    ROOFLINE_NATIVE_W4A4_REASON,
                    ROOFLINE_COMPUTE_BOUND_FP8_REASON,
                    ROOFLINE_UNPACK_REASON,
                ]
                .contains(&reason.as_str())));
            }
        }
    }

    #[test]
    fn roofline_uses_declared_paths_not_hardware_or_quantization_names() {
        for platform in ["ascend-cann8", "nvidia-b200", "amd-rocm"] {
            let mut run = sample_run("vllm");
            run.profile_key.platform = Some(platform.into());
            // These labels intentionally disagree with the scoped declaration.
            run.image_id = Some("image-with-fp8-awq-in-the-name".into());
            declare_quantization(&mut run, QuantFormat::W4A4);
            let req = roofline_request(16, SelectionPreference::Throughput);
            let response = select_roofline_runs(&req, std::slice::from_ref(&run));
            let candidate = &response.candidates[0];
            assert_eq!(
                candidate.score,
                score_candidate(req.constraints.preference, candidate, 15.0)
            );
            assert!(candidate
                .reasons
                .iter()
                .any(|reason| reason == ROOFLINE_NATIVE_W4A4_REASON));
            let cost = roofline_request(16, SelectionPreference::Cost);
            assert_eq!(
                roofline_quantization_adjustment(&cost, candidate, &[run]),
                (0.0, Some(ROOFLINE_NATIVE_W4A4_REASON))
            );
        }
    }

    #[test]
    fn roofline_unknown_or_unsupported_is_neutral_and_unpack_requires_explicit_deny() {
        let mut run = sample_run("vllm");
        declare_quantization(&mut run, QuantFormat::W4A16);
        let req = roofline_request(16, SelectionPreference::Latency);
        for (state, expected) in [
            (None, 0.0),
            (Some(SupportLevel::Unknown), 0.0),
            (Some(SupportLevel::Unsupported), -5.0),
        ] {
            let paths = &mut run
                .evidence
                .as_mut()
                .unwrap()
                .engine_capability
                .as_mut()
                .unwrap()
                .native_low_precision_paths;
            if let Some(state) = state {
                paths.insert(QuantFormat::W4A4, state);
            }
            let response = select_roofline_runs(&req, std::slice::from_ref(&run));
            assert_eq!(
                roofline_quantization_adjustment(
                    &req,
                    &response.candidates[0],
                    std::slice::from_ref(&run)
                )
                .0,
                expected
            );
        }
        for state in [SupportLevel::Unknown, SupportLevel::Unsupported] {
            run.evidence
                .as_mut()
                .unwrap()
                .engine_capability
                .as_mut()
                .unwrap()
                .native_low_precision_paths
                .insert(QuantFormat::W4A16, state);
            let low = roofline_request(1, SelectionPreference::Throughput);
            let response = select_roofline_runs(&low, std::slice::from_ref(&run));
            assert_eq!(
                roofline_quantization_adjustment(
                    &low,
                    &response.candidates[0],
                    std::slice::from_ref(&run)
                ),
                (0.0, None)
            );
        }
    }

    #[test]
    fn roofline_missing_conflicting_or_mismatched_evidence_is_neutral() {
        let mut run = sample_run("vllm");
        declare_quantization(&mut run, QuantFormat::W4A4);
        let req = roofline_request(1, SelectionPreference::Throughput);
        let candidate = select_roofline_runs(&req, std::slice::from_ref(&run))
            .candidates
            .remove(0);
        for mismatch in [
            "engine",
            "version",
            "platform",
            "image",
            "model",
            "synthetic",
            "missing",
            "format",
            "no_image",
            "no_platform",
        ] {
            let mut invalid = run.clone();
            match mismatch {
                "engine" => {
                    invalid
                        .evidence
                        .as_mut()
                        .unwrap()
                        .engine_capability
                        .as_mut()
                        .unwrap()
                        .engine_type = "sglang".into()
                }
                "version" => {
                    invalid
                        .evidence
                        .as_mut()
                        .unwrap()
                        .engine_capability
                        .as_mut()
                        .unwrap()
                        .engine_version = Some("different".into())
                }
                "platform" => invalid.profile_key.platform = Some("different".into()),
                "image" => invalid.image_id = Some("different".into()),
                "model" => invalid.profile_key.model_name = "different".into(),
                "synthetic" => invalid.evidence.as_mut().unwrap().synthetic = true,
                "missing" => invalid.evidence = None,
                "format" => {
                    invalid
                        .evidence
                        .as_mut()
                        .unwrap()
                        .engine_capability
                        .as_mut()
                        .unwrap()
                        .quantization = None
                }
                "no_image" => invalid.image_id = None,
                "no_platform" => invalid.profile_key.platform = None,
                _ => unreachable!(),
            }
            assert_eq!(
                roofline_quantization_adjustment(&req, &candidate, &[invalid]),
                (0.0, None),
                "{mismatch}"
            );
        }
        for scope in ["image", "platform", "version"] {
            let mut unknown_run = run.clone();
            let mut unknown_candidate = candidate.clone();
            match scope {
                "image" => {
                    unknown_run.image_id = None;
                    unknown_candidate.image_id = None;
                }
                "platform" => {
                    unknown_run.profile_key.platform = None;
                    unknown_candidate.platform = None;
                }
                "version" => {
                    unknown_run.profile_key.engine_version = None;
                    unknown_run
                        .evidence
                        .as_mut()
                        .unwrap()
                        .engine_capability
                        .as_mut()
                        .unwrap()
                        .engine_version = None;
                    unknown_candidate.engine_version = None;
                }
                _ => unreachable!(),
            }
            assert_eq!(
                roofline_quantization_adjustment(&req, &unknown_candidate, &[unknown_run]),
                (0.0, None),
                "{scope}"
            );
        }
        let mut other = run.clone();
        other.run_id = "conflicting".into();
        declare_quantization(&mut other, QuantFormat::Fp8);
        let mut both = candidate.clone();
        both.evidence_run_ids.push(other.run_id.clone());
        assert_eq!(
            roofline_quantization_adjustment(&req, &both, &[run.clone(), other.clone()]),
            (0.0, None)
        );
        other.evidence = None;
        assert_eq!(
            roofline_quantization_adjustment(&req, &both, &[run, other]),
            (0.0, None)
        );
        both.evidence_run_ids.clear();
        assert_eq!(
            roofline_quantization_adjustment(&req, &both, &[]),
            (0.0, None)
        );
    }

    #[test]
    fn roofline_cannot_borrow_another_platforms_declaration() {
        let mut declared = sample_run("vllm");
        declared.profile_key.platform = Some("declared".into());
        declare_quantization(&mut declared, QuantFormat::W4A4);
        let mut unknown = declared.clone();
        unknown.run_id = "unknown".into();
        unknown.profile_key.platform = Some("ascend-cann8".into());
        unknown.evidence = None;
        let runs = [declared, unknown];
        let profiles: Vec<_> = runs
            .iter()
            .map(|run| crate::benchmark::build_profile_from_runs(&run.profile_key, &runs, 10))
            .collect();
        let req = roofline_request(1, SelectionPreference::Throughput);
        let response = select_backends(&req, &profiles, &runs);
        assert_eq!(response.candidates.len(), 2);
        assert_eq!(response.candidates[0].platform.as_deref(), Some("declared"));
        for candidate in response.candidates {
            let expected = if candidate.platform.as_deref() == Some("declared") {
                15.0
            } else {
                0.0
            };
            assert_eq!(
                candidate.score,
                score_candidate(req.constraints.preference, &candidate, expected)
            );
        }
    }

    #[test]
    fn overwritten_run_cannot_lend_a_declaration_to_its_old_profile() {
        for changed in ["workload", "gpu", "fingerprint"] {
            let original = sample_run("vllm");
            let stale_profile = crate::benchmark::build_profile_from_runs(
                &original.profile_key,
                std::slice::from_ref(&original),
                10,
            );
            let mut retained = original.clone();
            retained.run_id = "retained".into();
            retained.throughput_tps = Some(10.0);
            let mixed_profile = crate::benchmark::build_profile_from_runs(
                &original.profile_key,
                &[original.clone(), retained.clone()],
                10,
            );
            let mut overwritten = original;
            match changed {
                "workload" => overwritten.profile_key.workload_id = "different-workload".into(),
                "gpu" => overwritten.profile_key.gpu_name = Some("different-gpu".into()),
                "fingerprint" => {
                    overwritten.profile_key.param_fingerprint = Some("different-parameters".into())
                }
                _ => unreachable!(),
            }
            declare_quantization(&mut overwritten, QuantFormat::W4A4);
            let req = roofline_request(1, SelectionPreference::Throughput);
            let stale_only = select_backends(
                &req,
                std::slice::from_ref(&stale_profile),
                std::slice::from_ref(&overwritten),
            );
            assert!(
                stale_only.candidates.is_empty(),
                "{changed}: old profile must not borrow overwritten evidence"
            );
            let current_profile = crate::benchmark::build_profile_from_runs(
                &overwritten.profile_key,
                std::slice::from_ref(&overwritten),
                20,
            );
            let mixed = select_backends(&req, &[mixed_profile], &[overwritten.clone(), retained]);
            assert_eq!(mixed.candidates.len(), 1);
            assert_eq!(mixed.candidates[0].throughput_tps, Some(10.0));
            assert_eq!(mixed.candidates[0].evidence_run_ids, ["retained"]);
            assert_eq!(
                mixed.candidates[0].score,
                score_candidate(req.constraints.preference, &mixed.candidates[0], 0.0)
            );
            let response = select_backends(&req, &[stale_profile, current_profile], &[overwritten]);
            assert_eq!(response.candidates.len(), 1, "{changed}");
            let candidate = &response.candidates[0];
            assert_eq!(
                candidate.score,
                score_candidate(req.constraints.preference, candidate, 15.0)
            );
        }
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
        declare_quantization(&mut r_4bit, QuantFormat::W4A16);
        r_4bit
            .evidence
            .as_mut()
            .unwrap()
            .engine_capability
            .as_mut()
            .unwrap()
            .native_low_precision_paths
            .insert(QuantFormat::W4A4, SupportLevel::Unsupported);

        let mut r_fp8 = sample_run("sglang");
        r_fp8.run_id = "r-fp8".into();
        r_fp8.profile_key.param_fingerprint = Some("fp8".into());
        r_fp8.image_id = Some("sglang-fp8".into());
        declare_quantization(&mut r_fp8, QuantFormat::Fp8);

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
            ..Default::default()
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

    #[test]
    fn three_kind_attention_kv_capacity_model_scales_full_sliding_state_and_mla() {
        let full_attn = ModelProfile {
            profile_id: "full".into(),
            model_name: "Qwen/Qwen2.5-32B".into(),
            parameter_billions: Some(32.0),
            context_length: Some(32768),
            ..Default::default()
        };
        let full_vram = estimate_kv_cache_vram_mb(&full_attn, 8, 32768);

        // Sliding window (W = 4096): caps attention KV at 4096 tokens instead of 32768 (8x reduction)
        let sliding = ModelProfile {
            sliding_window_tokens: Some(4096),
            ..full_attn.clone()
        };
        let sliding_vram = estimate_kv_cache_vram_mb(&sliding, 8, 32768);
        assert!(
            (sliding_vram * 8.0 - full_vram).abs() < 1e-3,
            "sliding window W=4096 at ctx=32768 should be 1/8 of full attention: {sliding_vram} vs {full_vram}"
        );

        // Hybrid 75% linear/recurrent state layers (Qwen3.5 Gated DeltaNet style: kState 2-slot checkpoint)
        let hybrid_state = ModelProfile {
            linear_attention_layer_ratio: Some(0.75),
            ..full_attn.clone()
        };
        let hybrid_vram = estimate_kv_cache_vram_mb(&hybrid_state, 8, 32768);
        assert!(
            hybrid_vram < full_vram * 0.30,
            "75% kState layers with constant 2-slot state should cut long-context KV VRAM by >70%: {hybrid_vram} vs {full_vram}"
        );

        // MLA compressed latent KV rank (e.g. rank 512)
        let mla = ModelProfile {
            kv_lora_rank: Some(512),
            ..full_attn
        };
        let mla_vram = estimate_kv_cache_vram_mb(&mla, 8, 32768);
        assert!(
            (mla_vram * 4.0 - full_vram).abs() < 1e-3,
            "MLA rank=512 should compress KV VRAM by 4x relative to rank=2048 baseline"
        );
    }
}
