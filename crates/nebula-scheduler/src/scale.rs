//! Scale-down replica selection helpers (shared by binary reconcile + unit tests).

use nebula_common::{EndpointStats, PlacementAssignment, ReplicaRole};

/// Choose which healthy assignments to remove when scaling down.
/// Prefer low pending_requests, then lower replica_id for stability.
/// When roled (`pd_split`) assignments are present, never remove the last
/// `Prefill` or the last `Decode` replica so role completeness is preserved.
pub fn select_replicas_to_remove(
    assignments: &[PlacementAssignment],
    remove_count: usize,
    stats: Option<&Vec<EndpointStats>>,
) -> Vec<u32> {
    if remove_count == 0 || assignments.is_empty() {
        return Vec::new();
    }
    let remove_count = remove_count.min(assignments.len());

    let pending_of = |replica_id: u32| -> u64 {
        stats
            .and_then(|list| list.iter().find(|s| s.replica_id == replica_id))
            .map(|s| s.pending_requests)
            .unwrap_or(0)
    };

    let mut remaining_prefill = assignments
        .iter()
        .filter(|a| a.role == ReplicaRole::Prefill)
        .count();
    let mut remaining_decode = assignments
        .iter()
        .filter(|a| a.role == ReplicaRole::Decode)
        .count();

    let mut scored: Vec<(u32, ReplicaRole, u64)> = assignments
        .iter()
        .map(|a| (a.replica_id, a.role, pending_of(a.replica_id)))
        .collect();
    scored.sort_by_key(|(rid, _, pending)| (*pending, *rid));

    let mut removed = Vec::new();
    for (rid, role, _) in scored {
        if removed.len() >= remove_count {
            break;
        }
        match role {
            ReplicaRole::Prefill if remaining_prefill <= 1 => continue,
            ReplicaRole::Prefill => remaining_prefill -= 1,
            ReplicaRole::Decode if remaining_decode <= 1 => continue,
            ReplicaRole::Decode => remaining_decode -= 1,
            ReplicaRole::Mixed => {}
        }
        removed.push(rid);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(replica_id: u32) -> PlacementAssignment {
        PlacementAssignment {
            replica_id,
            node_id: "n1".into(),
            engine_config_path: "/tmp/c.yaml".into(),
            port: 8000 + replica_id as u16,
            gpu_index: None,
            gpu_indices: None,
            extra_args: None,
            engine_type: None,
            docker_image: None,
            pool_id: None,
            role: Default::default(),
            kv_sharing: None,
        }
    }

    fn stats(replica_id: u32, pending: u64) -> EndpointStats {
        EndpointStats {
            model_uid: "m1".into(),
            replica_id,
            last_updated_ms: 0,
            pending_requests: pending,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: None,
        }
    }

    #[test]
    fn scale_down_prefers_low_pending() {
        let assignments = vec![assignment(0), assignment(1), assignment(2)];
        let stats_list = vec![stats(0, 10), stats(1, 0), stats(2, 5)];
        let remove = select_replicas_to_remove(&assignments, 2, Some(&stats_list));
        assert_eq!(remove, vec![1, 2]);
    }

    #[test]
    fn scale_down_stable_without_stats() {
        let assignments = vec![assignment(2), assignment(0), assignment(1)];
        let remove = select_replicas_to_remove(&assignments, 1, None);
        assert_eq!(remove, vec![0]);
    }

    #[test]
    fn scale_down_desired_one_from_three() {
        let assignments = vec![assignment(0), assignment(1), assignment(2)];
        let remove = select_replicas_to_remove(&assignments, 2, None);
        assert_eq!(remove, vec![0, 1]);
        let kept: Vec<u32> = assignments
            .iter()
            .map(|a| a.replica_id)
            .filter(|id| !remove.contains(id))
            .collect();
        assert_eq!(kept, vec![2]);
    }

    #[test]
    fn scale_down_never_drops_last_prefill_or_decode_role() {
        let mut p0 = assignment(0);
        p0.role = ReplicaRole::Prefill;
        let mut d1 = assignment(1);
        d1.role = ReplicaRole::Decode;
        let mut d2 = assignment(2);
        d2.role = ReplicaRole::Decode;

        // 1 prefill + 1 decode: cannot remove either without breaking role set.
        assert!(
            select_replicas_to_remove(&[p0.clone(), d1.clone()], 1, None).is_empty(),
            "1P+1D must never lose its sole prefill or decode replica"
        );

        // 1 prefill + 2 decode: asking to remove 2 can only remove 1 decode (rid=1),
        // keeping p0 (last prefill) and d2 (last decode).
        let removed = select_replicas_to_remove(&[p0, d1, d2], 2, None);
        assert_eq!(removed, vec![1]);
    }
}
