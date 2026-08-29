//! Scale-down replica selection helpers (shared by binary reconcile + unit tests).

use nebula_common::{EndpointStats, PlacementAssignment};

/// Choose which healthy assignments to remove when scaling down.
/// Prefer low pending_requests, then lower replica_id for stability.
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

    let mut scored: Vec<(u32, u64)> = assignments
        .iter()
        .map(|a| (a.replica_id, pending_of(a.replica_id)))
        .collect();
    scored.sort_by_key(|(rid, pending)| (*pending, *rid));
    scored
        .into_iter()
        .take(remove_count)
        .map(|(rid, _)| rid)
        .collect()
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
        }
    }

    fn stats(replica_id: u32, pending: u64) -> EndpointStats {
        EndpointStats {
            model_uid: "m1".into(),
            replica_id,
            last_updated_ms: 0,
            pending_requests: pending,
            prefix_cache_hit_rate: None,
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
}
