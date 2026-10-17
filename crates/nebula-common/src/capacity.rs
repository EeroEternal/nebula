//! Read-only capacity planning snapshot (Phase 3 skeleton).
//!
//! Aggregates Deployment desired replicas vs ready endpoints and coarse GPU
//! headroom. Never auto-scales; never invents compliance.

use serde::{Deserialize, Serialize};

use crate::endpoint::{EndpointInfo, EndpointStats, EndpointStatus};
use crate::model_deployment::{DeploymentTopology, DesiredState, ModelDeployment, ReplicaRole};

/// One model's desired vs observed serving capacity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCapacityRow {
    pub model_uid: String,
    pub desired_state: String,
    pub desired_replicas: u32,
    pub ready_replicas: u32,
    pub unhealthy_replicas: u32,
    pub pending_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_kv_usage: Option<f64>,
    /// `desired_replicas - ready_replicas` while Running; else 0.
    pub replica_gap: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

/// Cluster-wide capacity view for the console (not a write path).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacitySnapshot {
    pub models: Vec<ModelCapacityRow>,
    pub gpu_total: u32,
    pub gpu_free: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    pub evaluated_at_ms: u64,
}

/// Pure aggregation used by BFF. `gpu_free` / `gpu_total` come from inventory.
pub fn build_capacity_snapshot(
    deployments: &[ModelDeployment],
    endpoints: &[EndpointInfo],
    stats: &[EndpointStats],
    gpu_total: u32,
    gpu_free: u32,
    now_ms: u64,
) -> CapacitySnapshot {
    let mut models = Vec::new();
    let mut cluster_hints = Vec::new();

    for dep in deployments {
        let eps: Vec<&EndpointInfo> = endpoints
            .iter()
            .filter(|e| e.model_uid == dep.model_uid)
            .collect();
        let ready = eps
            .iter()
            .filter(|e| e.status == EndpointStatus::Ready)
            .count() as u32;
        let unhealthy = eps
            .iter()
            .filter(|e| matches!(e.status, EndpointStatus::Unhealthy | EndpointStatus::Failed))
            .count() as u32;

        let model_stats: Vec<&EndpointStats> = stats
            .iter()
            .filter(|s| s.model_uid == dep.model_uid)
            .collect();
        let pending_total: u64 = model_stats.iter().map(|s| s.pending_requests).sum();
        let kv_vals: Vec<f64> = model_stats
            .iter()
            .filter_map(|s| s.kv_cache_usage)
            .collect();
        let avg_kv_usage = if kv_vals.is_empty() {
            None
        } else {
            Some(kv_vals.iter().sum::<f64>() / kv_vals.len() as f64)
        };

        let desired_state = match dep.desired_state {
            DesiredState::Running => "running",
            DesiredState::Stopped => "stopped",
        };
        let replica_gap = if dep.desired_state == DesiredState::Running {
            dep.replicas as i32 - ready as i32
        } else {
            0
        };

        let mut hints = Vec::new();
        if dep.desired_state == DesiredState::Running && replica_gap > 0 {
            hints.push(format!(
                "replica_gap={replica_gap}: desired {} ready {ready}; scheduler may still be placing or nodes lack free GPUs",
                dep.replicas
            ));
        }
        if unhealthy > 0 {
            hints.push(format!(
                "unhealthy_replicas={unhealthy}: inspect /endpoints and node health before adding load"
            ));
        }
        if pending_total >= 8 {
            hints.push(format!(
                "pending_total={pending_total}: queue pressure — consider more replicas only if GPUs are free"
            ));
        }
        if let Some(kv) = avg_kv_usage {
            if kv >= 0.85 {
                hints.push(format!(
                    "avg_kv_usage={kv:.2}: KV near saturation — capacity tight even if replica count matches"
                ));
            }
        }
        let is_pd_split = matches!(dep.topology, Some(DeploymentTopology::PdSplit))
            || eps.iter().any(|e| e.role != ReplicaRole::Mixed);
        if is_pd_split {
            let prefill_pending: u64 = eps
                .iter()
                .filter(|e| e.role == ReplicaRole::Prefill)
                .filter_map(|e| model_stats.iter().find(|s| s.replica_id == e.replica_id))
                .map(|s| s.pending_requests)
                .sum();
            if prefill_pending >= 4 {
                hints.push(format!(
                    "scale_prefill_pool: prefill_pending_total={prefill_pending} (ISL/TTFT compute-bound; consider adding Prefill replicas)"
                ));
            }
            let decode_kvs: Vec<f64> = eps
                .iter()
                .filter(|e| e.role == ReplicaRole::Decode)
                .filter_map(|e| model_stats.iter().find(|s| s.replica_id == e.replica_id))
                .filter_map(|s| s.kv_cache_usage)
                .collect();
            if !decode_kvs.is_empty() {
                let decode_avg_kv = decode_kvs.iter().sum::<f64>() / decode_kvs.len() as f64;
                if decode_avg_kv > 0.85 {
                    hints.push(format!(
                        "scale_decode_pool: decode_avg_kv_usage={decode_avg_kv:.2} > 0.85 (OSL/TPOT memory-bound; consider adding Decode replicas)"
                    ));
                }
            }
        }
        if dep.desired_state == DesiredState::Running && replica_gap > 0 && gpu_free == 0 {
            hints.push(
                "no free GPUs while replica_gap>0: cannot place more replicas until capacity frees"
                    .into(),
            );
        }

        models.push(ModelCapacityRow {
            model_uid: dep.model_uid.clone(),
            desired_state: desired_state.into(),
            desired_replicas: dep.replicas,
            ready_replicas: ready,
            unhealthy_replicas: unhealthy,
            pending_total,
            avg_kv_usage,
            replica_gap,
            hints,
        });
    }

    models.sort_by(|a, b| a.model_uid.cmp(&b.model_uid));

    let any_gap = models.iter().any(|m| m.replica_gap > 0);
    if any_gap && gpu_free == 0 {
        cluster_hints.push(
            "cluster: replica gaps with zero free GPUs — add nodes/GPUs or stop unused models"
                .into(),
        );
    }
    if gpu_total == 0 {
        cluster_hints.push(
            "cluster: no GPU inventory (no node heartbeats) — capacity view is incomplete".into(),
        );
    } else if gpu_free * 10 < gpu_total {
        cluster_hints.push(format!(
            "cluster: gpu_free={gpu_free}/{gpu_total} (<10% headroom) — plan capacity before new deployments"
        ));
    }

    CapacitySnapshot {
        models,
        gpu_total,
        gpu_free,
        hints: cluster_hints,
        evaluated_at_ms: now_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::EndpointKind;

    fn dep(uid: &str, replicas: u32) -> ModelDeployment {
        ModelDeployment {
            model_uid: uid.into(),
            desired_state: DesiredState::Running,
            replicas,
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
        }
    }

    fn ep(uid: &str, rid: u32, status: EndpointStatus) -> EndpointInfo {
        EndpointInfo {
            model_uid: uid.into(),
            replica_id: rid,
            plan_version: 1,
            node_id: "n1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status,
            last_heartbeat_ms: 1,
            status_detail: None,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:1".into()),
            engine_type: None,
            role: Default::default(),
        }
    }

    #[test]
    fn reports_replica_gap_and_no_gpu() {
        let snap = build_capacity_snapshot(
            &[dep("m1", 2)],
            &[ep("m1", 1, EndpointStatus::Ready)],
            &[],
            4,
            0,
            10,
        );
        assert_eq!(snap.models[0].replica_gap, 1);
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("replica_gap=")));
        assert!(snap.hints.iter().any(|h| h.contains("zero free GPUs")));
    }

    #[test]
    fn pending_and_kv_hints() {
        let stats = EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: 1,
            pending_requests: 12,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.9),
        };
        let snap = build_capacity_snapshot(
            &[dep("m1", 1)],
            &[ep("m1", 1, EndpointStatus::Ready)],
            &[stats],
            2,
            1,
            10,
        );
        assert_eq!(snap.models[0].replica_gap, 0);
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("pending_total=")));
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("avg_kv_usage=")));
    }

    #[test]
    fn pd_split_emits_role_pool_scaling_hints() {
        let mut d = dep("m1", 2);
        d.topology = Some(DeploymentTopology::PdSplit);
        let mut ep0 = ep("m1", 0, EndpointStatus::Ready);
        ep0.role = ReplicaRole::Prefill;
        let mut ep1 = ep("m1", 1, EndpointStatus::Ready);
        ep1.role = ReplicaRole::Decode;
        let s0 = EndpointStats {
            model_uid: "m1".into(),
            replica_id: 0,
            last_updated_ms: 1,
            pending_requests: 6,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.30),
        };
        let s1 = EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: 1,
            pending_requests: 1,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.90),
        };
        let snap = build_capacity_snapshot(&[d], &[ep0, ep1], &[s0, s1], 4, 2, 10);
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("scale_prefill_pool:")));
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("scale_decode_pool:")));
    }
}
