//! Bounded serving-configuration search. No model, sampling, or KV ABI changes.

use serde::{Deserialize, Serialize};

use crate::{
    effective_replica_config, validate_model_config, ModelDeployment, ModelSpec,
    PIN_TIMEOUT_SEC_RANGE,
};

pub const EXPERIMENT_LABEL: &str = "nebula.autotune.experiment";

/// Reserved before spec watches catch up, including placement-first startup.
pub fn is_experiment_uid(uid: &str) -> bool {
    uid.strip_prefix("at-").is_some_and(|suffix| {
        suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TuningParameters {
    pub max_num_seqs: u32,
    pub max_num_batched_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_cpu_size_gb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_timeout_sec: Option<u32>,
}

impl TuningParameters {
    pub fn baseline(spec: &ModelSpec, dep: &ModelDeployment) -> Result<Self, String> {
        if spec.engine_type.as_deref().unwrap_or("vllm") != "vllm" {
            return Err("autotuning currently supports vllm only".into());
        }
        if dep.replica_specs.as_ref().is_some_and(|replicas| {
            replicas
                .iter()
                .any(|r| r.config_overrides.is_some() || r.image_id.is_some())
        }) {
            return Err("autotuning requires uniform replica configs and images".into());
        }
        let config = effective_replica_config(dep, spec.config.as_ref(), 0)
            .ok_or("set explicit max_num_seqs and max_num_batched_tokens before autotuning")?;
        Ok(Self {
            max_num_seqs: config.max_num_seqs.ok_or("max_num_seqs must be explicit")?,
            max_num_batched_tokens: config
                .max_num_batched_tokens
                .ok_or("max_num_batched_tokens must be explicit")?,
            local_cpu_size_gb: dep
                .kv_sharing
                .as_ref()
                .map(|kv| kv.local_cpu_size_gb.unwrap_or(5)),
            pin_timeout_sec: dep
                .kv_sharing
                .as_ref()
                .map(|kv| kv.pin_timeout_sec.unwrap_or(crate::DEFAULT_PIN_TIMEOUT_SEC)),
        })
    }

    pub fn apply(
        &self,
        spec: &ModelSpec,
        dep: &ModelDeployment,
    ) -> Result<ModelDeployment, String> {
        if !(1..=4096).contains(&self.max_num_seqs)
            || !(1..=1_048_576).contains(&self.max_num_batched_tokens)
        {
            return Err("scheduler tuning exceeds bounded search limits".into());
        }
        let mut result = dep.clone();
        let config = result.config_overrides.get_or_insert_with(Default::default);
        config.max_num_seqs = Some(self.max_num_seqs);
        config.max_num_batched_tokens = Some(self.max_num_batched_tokens);
        let effective = effective_replica_config(&result, spec.config.as_ref(), 0)
            .ok_or("effective model config unavailable")?;
        validate_model_config(spec.engine_type.as_deref().unwrap_or("vllm"), &effective)?;
        match result.kv_sharing.as_mut() {
            Some(kv) => {
                let size = self
                    .local_cpu_size_gb
                    .ok_or("L1 size is required for a KV deployment")?;
                let ttl = self
                    .pin_timeout_sec
                    .ok_or("pin TTL is required for a KV deployment")?;
                if !(1..=4096).contains(&size) || !PIN_TIMEOUT_SEC_RANGE.contains(&ttl) {
                    return Err("invalid L1 size or pin TTL".into());
                }
                if kv.pin_budget_gb.is_some_and(|budget| budget >= size) {
                    return Err("pin budget must remain smaller than the L1 tier".into());
                }
                kv.local_cpu_size_gb = Some(size);
                kv.pin_timeout_sec = Some(ttl);
            }
            None if self.local_cpu_size_gb.is_some() || self.pin_timeout_sec.is_some() => {
                return Err("KV tuning requires an existing LMCache deployment".into());
            }
            None => {}
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TuningObjective {
    pub ttft_p95_ms_max: f64,
    pub tpot_p95_ms_max: f64,
    pub e2e_p95_ms_max: f64,
    pub min_improvement: f64,
    pub max_regression: f64,
    pub min_requests: u32,
}

impl TuningObjective {
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.ttft_p95_ms_max,
            self.tpot_p95_ms_max,
            self.e2e_p95_ms_max,
        ]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.0)
            || !self.min_improvement.is_finite()
            || !(0.01..=1.0).contains(&self.min_improvement)
            || !self.max_regression.is_finite()
            || !(0.0..=0.2).contains(&self.max_regression)
            || !(16..=100_000).contains(&self.min_requests)
        {
            return Err("invalid autotuning objective or sample budget".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DesiredState, ModelConfig, ModelSource};

    fn fixture() -> (ModelSpec, ModelDeployment) {
        (
            ModelSpec {
                model_uid: "m".into(),
                model_name: "weights".into(),
                model_source: ModelSource::Local,
                model_path: Some("/models/weights".into()),
                engine_type: Some("vllm".into()),
                docker_image: Some("image@sha256:test".into()),
                config: Some(ModelConfig {
                    max_num_seqs: Some(16),
                    max_num_batched_tokens: Some(2048),
                    enable_chunked_prefill: Some(true),
                    ..Default::default()
                }),
                labels: Default::default(),
                created_at_ms: 1,
                updated_at_ms: 1,
                created_by: None,
            },
            ModelDeployment {
                model_uid: "m".into(),
                desired_state: DesiredState::Running,
                replicas: 1,
                min_replicas: None,
                max_replicas: None,
                node_affinity: None,
                gpu_affinity: None,
                replica_specs: None,
                config_overrides: None,
                image_id: None,
                allowed_pools: None,
                image_override_reason: None,
                compat_rule_ids: vec![],
                topology: None,
                kv_sharing: None,
                version: 1,
                updated_at_ms: 1,
            },
        )
    }

    #[test]
    fn tuning_changes_only_allowlisted_fields() {
        let (spec, dep) = fixture();
        let mut params = TuningParameters::baseline(&spec, &dep).unwrap();
        params.max_num_seqs = 32;
        let result = params.apply(&spec, &dep).unwrap();
        let effective = effective_replica_config(&result, spec.config.as_ref(), 0).unwrap();
        assert_eq!(effective.max_num_seqs, Some(32));
        assert_eq!(effective.enable_chunked_prefill, Some(true));
        assert_eq!(result.version, dep.version);
        assert_eq!(result.model_uid, dep.model_uid);
        assert_eq!(result.image_id, dep.image_id);
        params.local_cpu_size_gb = Some(16);
        assert!(params.apply(&spec, &dep).is_err());
    }

    #[test]
    fn kv_search_preserves_remote_store_and_pin_reserve() {
        let (spec, mut dep) = fixture();
        dep.kv_sharing = Some(
            serde_json::from_value(serde_json::json!({
                "remote_url":"lm://127.0.0.1:6800", "local_cpu_size_gb":5, "pin_budget_gb":4
            }))
            .unwrap(),
        );
        let mut params = TuningParameters::baseline(&spec, &dep).unwrap();
        params.local_cpu_size_gb = Some(4);
        assert!(params.apply(&spec, &dep).is_err());
        params.local_cpu_size_gb = Some(8);
        let tuned = params.apply(&spec, &dep).unwrap().kv_sharing.unwrap();
        assert_eq!(tuned.remote_url, "lm://127.0.0.1:6800");
        assert_eq!(tuned.pin_budget_gb, Some(4));
        assert_eq!(tuned.local_cpu_size_gb, Some(8));
    }

    #[test]
    fn cannot_tune_unknown_baselines_or_unsupported_engines() {
        let (mut spec, dep) = fixture();
        spec.config.as_mut().unwrap().max_num_seqs = None;
        assert!(TuningParameters::baseline(&spec, &dep).is_err());
        spec.engine_type = Some("sglang".into());
        assert!(TuningParameters::baseline(&spec, &dep).is_err());
    }

    #[test]
    fn tuning_rejects_bad_token_budgets_and_unknown_fields() {
        let (spec, dep) = fixture();
        let mut params = TuningParameters::baseline(&spec, &dep).unwrap();
        params.max_num_batched_tokens = 1;
        assert!(params.apply(&spec, &dep).is_err());
        assert!(
            serde_json::from_value::<TuningParameters>(serde_json::json!({
                "max_num_seqs": 16, "max_num_batched_tokens": 2048, "temperature": 0
            }))
            .is_err()
        );
    }
}
