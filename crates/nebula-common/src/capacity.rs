//! Read-only capacity planning snapshot (Phase 3 skeleton).
//!
//! Aggregates Deployment desired replicas vs ready endpoints and coarse GPU
//! headroom. Never auto-scales; never invents compliance.

use serde::{Deserialize, Serialize};

use crate::endpoint::{EndpointInfo, EndpointStats, EndpointStatus};
use crate::model_deployment::{DeploymentTopology, DesiredState, ModelDeployment, ReplicaRole};
use crate::node_status::NodeStatus;

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
    nodes: &[NodeStatus],
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
        let preemptions_total: u64 = model_stats
            .iter()
            .filter_map(|s| s.num_preemptions_total)
            .sum();
        if preemptions_total > 0 {
            hints.push(format!(
                "kv_preemptions_detected: preemptions_total={preemptions_total} — Decode KV retraction/preemption active; consider adding replicas or lowering max_num_seqs"
            ));
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
        // KV L1 tier (Batch 6.1): each pd_split replica offloads KV to host DRAM. That
        // reservation used to be an undeclared 5 GB per replica, so N replicas could
        // silently overcommit a node's memory. Now the size is declared and checked
        // against what the node reports.
        // `kv_sharing_env(Mixed)` returns nothing, so a `replicated` deployment never
        // applies its declared tier: counting its DRAM would raise
        // `kv_l1_oversubscribed` for a reservation that does not exist.
        let tier_applies_to_pd_split = matches!(dep.topology, Some(DeploymentTopology::PdSplit))
            || eps.iter().any(|e| e.role != ReplicaRole::Mixed);
        if let Some(kv) = dep.kv_sharing.as_ref().filter(|_| tier_applies_to_pd_split) {
            let per_replica_gb = u64::from(kv.local_cpu_size_gb.unwrap_or(5));
            // Tool-Pause pins are unevictable, so they are subtracted from the tier rather
            // than added to it: the admission question is "is there still a cache left",
            // not "is there more host memory reserved".
            let pin_gb = u64::from(kv.pin_budget_gb.unwrap_or(0));
            let working_set_gb = per_replica_gb.saturating_sub(pin_gb);
            let replicas = eps.len() as u64;
            if replicas > 0 {
                if pin_gb > 0 && (pin_gb * 2 >= per_replica_gb || working_set_gb == 0) {
                    hints.push(format!(
                        "kv_l1_pin_reserve_high: {pin_gb} of the {per_replica_gb} GB L1 per replica is reserved for Tool-Pause pins ({}%), leaving {working_set_gb} GB for the cache working set (pinned KV cannot be evicted); lower kv_sharing.pin_budget_gb or raise kv_sharing.local_cpu_size_gb",
                        pin_gb * 100 / per_replica_gb
                    ));
                }
                let mut declared_per_node: std::collections::HashMap<&str, u64> =
                    std::collections::HashMap::new();
                for ep in &eps {
                    *declared_per_node.entry(ep.node_id.as_str()).or_insert(0) += per_replica_gb;
                }
                for (node_id, declared_gb) in declared_per_node {
                    // Unknown host memory is not an alarm: stay silent rather than
                    // emit noise for nodes that do not report it.
                    if let Some(free_mb) = nodes
                        .iter()
                        .find(|n| n.node_id == node_id)
                        .and_then(|n| n.host_memory_available_mb)
                    {
                        let declared_mb = declared_gb.saturating_mul(1024);
                        if declared_mb > free_mb {
                            hints.push(format!(
                                "kv_l1_oversubscribed: node {node_id} declares {declared_gb} GB of KV L1 host DRAM but reports only {free_mb} MB available (undeclared default is 5 GB/replica; set kv_sharing.local_cpu_size_gb or lower replicas)"
                            ));
                        }
                    }
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
            kv_cache_usage: Some(0.9),
            num_preemptions_total: Some(3),
            ..Default::default()
        };
        let snap = build_capacity_snapshot(
            &[dep("m1", 1)],
            &[ep("m1", 1, EndpointStatus::Ready)],
            &[stats],
            &[],
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
        assert!(snap.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("kv_preemptions_detected: preemptions_total=3")));
    }

    #[test]
    fn replicated_tier_declaration_does_not_raise_dram_alarm() {
        use crate::node_status::NodeStatus;
        // `replicated` keeps all replicas Mixed, and `kv_sharing_env(Mixed)` is empty, so the
        // declared L1 tier is never applied - an alarm here would be for a reservation that
        // does not exist.
        let mut d = dep("m1", 2);
        d.kv_sharing = Some(crate::model_deployment::KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(64),
            ..Default::default()
        });
        let node = NodeStatus {
            node_id: "n1".into(),
            last_heartbeat_ms: 1,
            host_memory_total_mb: Some(65536),
            host_memory_available_mb: Some(1024),
            ..Default::default()
        };
        let eps = vec![
            ep("m1", 0, EndpointStatus::Ready),
            ep("m1", 1, EndpointStatus::Ready),
        ];
        let snap = build_capacity_snapshot(&[d], &eps, &[], &[node], 2, 2, 10);
        assert!(
            !snap.models[0]
                .hints
                .iter()
                .any(|h| h.starts_with("kv_l1_oversubscribed")),
            "hints: {:?}",
            snap.models[0].hints
        );
    }

    #[test]
    fn kv_l1_oversubscription_is_reported_per_node() {
        use crate::node_status::NodeStatus;
        let mut d = dep("m1", 2);
        d.topology = Some(DeploymentTopology::PdSplit);
        d.kv_sharing = Some(crate::model_deployment::KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(8),
            ..Default::default()
        });
        let mut e0 = ep("m1", 0, EndpointStatus::Ready);
        e0.node_id = "n1".into();
        let mut e1 = ep("m1", 1, EndpointStatus::Ready);
        e1.node_id = "n1".into();
        // 2 replicas x 8 GB declared, node has 4 GB available.
        let node = NodeStatus {
            node_id: "n1".into(),
            last_heartbeat_ms: 1,
            host_memory_total_mb: Some(65536),
            host_memory_available_mb: Some(4096),
            ..Default::default()
        };
        let snap = build_capacity_snapshot(
            &[d.clone()],
            &[e0.clone(), e1.clone()],
            &[],
            &[node],
            2,
            2,
            10,
        );
        assert!(
            snap.models[0]
                .hints
                .iter()
                .any(|h| h.starts_with("kv_l1_oversubscribed") && h.contains("16 GB")),
            "hints: {:?}",
            snap.models[0].hints
        );

        // Unknown host memory must not produce an alarm (no noise for non-reporting nodes).
        let snap_unknown = build_capacity_snapshot(&[d], &[e0, e1], &[], &[], 2, 2, 10);
        assert!(!snap_unknown.models[0]
            .hints
            .iter()
            .any(|h| h.starts_with("kv_l1_oversubscribed")));
    }

    #[test]
    fn tool_pause_pin_budget_is_subtracted_from_the_l1_working_set() {
        let mut d = dep("m1", 1);
        d.topology = Some(DeploymentTopology::PdSplit);
        d.kv_sharing = Some(crate::model_deployment::KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(8),
            pin_budget_gb: Some(6),
            ..Default::default()
        });
        let snap = build_capacity_snapshot(
            &[d.clone()],
            &[ep("m1", 0, EndpointStatus::Ready)],
            &[],
            &[],
            1,
            1,
            10,
        );
        let hint = snap.models[0]
            .hints
            .iter()
            .find(|h| h.starts_with("kv_l1_pin_reserve_high"))
            .unwrap_or_else(|| panic!("hints: {:?}", snap.models[0].hints));
        // 6 of 8 GB reserved = 75%, 2 GB left for the working set.
        assert!(hint.contains("6 of the 8 GB"), "{hint}");
        assert!(hint.contains("75%"), "{hint}");
        assert!(hint.contains("leaving 2 GB"), "{hint}");

        // A reserve that leaves most of the tier alone is not an alarm: pins are a normal
        // part of pd_split, only a squeezed working set is worth surfacing.
        d.kv_sharing = Some(crate::model_deployment::KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(8),
            pin_budget_gb: Some(1),
            ..Default::default()
        });
        let snap = build_capacity_snapshot(
            &[d],
            &[ep("m1", 0, EndpointStatus::Ready)],
            &[],
            &[],
            1,
            1,
            10,
        );
        assert!(
            !snap.models[0]
                .hints
                .iter()
                .any(|h| h.starts_with("kv_l1_pin_reserve_high")),
            "hints: {:?}",
            snap.models[0].hints
        );
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
            kv_cache_usage: Some(0.30),
            ..Default::default()
        };
        let s1 = EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: 1,
            pending_requests: 1,
            kv_cache_usage: Some(0.90),
            ..Default::default()
        };
        let snap = build_capacity_snapshot(&[d], &[ep0, ep1], &[s0, s1], &[], 4, 2, 10);
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
