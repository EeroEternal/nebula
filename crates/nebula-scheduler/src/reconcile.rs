use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

use nebula_common::{
    DesiredState, EndpointInfo, EndpointStats, EndpointStatus, ModelDeployment, ModelRequest,
    ModelRequestStatus, PlacementPlan,
};
use nebula_meta::LeaderGate;
use nebula_meta::{EtcdMetaStore, MetaStore};

use nebula_scheduler::metrics::SharedMetrics;
use nebula_scheduler::scale::select_replicas_to_remove;

use crate::planner::{allocate_port, list_used_resources, select_node_and_gpus};
use crate::util::now_ms;

/// Endpoint heartbeat timeout: if last_heartbeat_ms is older than this, consider it dead.
/// Keep this generous to avoid reclaiming assignments during cold starts.
pub(crate) const ENDPOINT_TIMEOUT_MS: u64 = 300_000;

/// Whether a placement assignment should be kept during reconcile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentDisposition {
    Keep,
    RemoveStale,
}

/// Classify an assignment's endpoint for reconcile (unit-tested).
pub(crate) fn assignment_disposition(
    ep: Option<&EndpointInfo>,
    now_ms: u64,
) -> AssignmentDisposition {
    match ep {
        Some(ep) => {
            if ep.status == EndpointStatus::Failed {
                AssignmentDisposition::RemoveStale
            } else if ep.status == EndpointStatus::Unhealthy {
                AssignmentDisposition::Keep
            } else if now_ms.saturating_sub(ep.last_heartbeat_ms) > ENDPOINT_TIMEOUT_MS {
                AssignmentDisposition::RemoveStale
            } else {
                AssignmentDisposition::Keep
            }
        }
        None => AssignmentDisposition::Keep,
    }
}

/// Reconcile interval.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// KV cache usage fraction above which we consider scaling up.
const SCALE_UP_KV_THRESHOLD: f64 = 0.80;

/// Pending requests average above which we consider scaling up.
const SCALE_UP_PENDING_THRESHOLD: f64 = 5.0;

/// If avg pending_requests == 0 for all endpoints, consider scaling down.
/// (We only scale down after a cooldown — tracked via plan version age.)
const SCALE_DOWN_IDLE_THRESHOLD: f64 = 0.0;

/// Minimum time (ms) a plan must be stable before we allow scale-down.
const SCALE_DOWN_COOLDOWN_MS: u64 = 300_000; // 5 minutes

/// Main reconcile loop. Runs periodically alongside the watch-based placement loop.
///
/// For each model with a PlacementPlan:
///   1. Check endpoint health — remove stale assignments whose endpoints timed out.
///   2. Check replica count — if fewer healthy replicas than desired, try to add new ones.
pub async fn reconcile_loop(
    store: EtcdMetaStore,
    default_port: u16,
    metrics: Arc<SharedMetrics>,
    leader: LeaderGate,
) {
    // Wait a bit before first reconcile to let the system stabilize.
    tokio::time::sleep(Duration::from_secs(10)).await;
    info!(
        "reconcile loop started (interval={}s)",
        RECONCILE_INTERVAL.as_secs()
    );

    loop {
        if !leader.is_leader() {
            tokio::time::sleep(RECONCILE_INTERVAL).await;
            continue;
        }
        metrics.reconcile_total.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = reconcile_once(&store, default_port, &metrics, &leader).await {
            metrics.reconcile_errors.fetch_add(1, Ordering::Relaxed);
            warn!(error=%e, "reconcile cycle failed");
        }
        tokio::time::sleep(RECONCILE_INTERVAL).await;
    }
}

async fn reconcile_once(
    store: &EtcdMetaStore,
    default_port: u16,
    metrics: &SharedMetrics,
    leader: &LeaderGate,
) -> anyhow::Result<()> {
    if !leader.is_leader() {
        return Ok(());
    }
    let now = now_ms();

    // 1. Load all placements
    let placement_kvs = store.list_prefix("/placements/").await?;
    let mut plans: Vec<PlacementPlan> = Vec::new();
    for (_, val, _) in &placement_kvs {
        if let Ok(plan) = serde_json::from_slice::<PlacementPlan>(val) {
            plans.push(plan);
        }
    }

    // Update placement gauge
    metrics
        .placements_total
        .store(plans.len() as u64, Ordering::Relaxed);

    if plans.is_empty() {
        return Ok(());
    }

    // 2. Load all endpoints
    let endpoint_kvs = store.list_prefix("/endpoints/").await?;
    let mut endpoints: HashMap<(String, u32), EndpointInfo> = HashMap::new();
    for (_, val, _) in &endpoint_kvs {
        if let Ok(ep) = serde_json::from_slice::<EndpointInfo>(val) {
            endpoints.insert((ep.model_uid.clone(), ep.replica_id), ep);
        }
    }

    // 3. Load deployments (declarative desired state — single path)
    let deployment_kvs = store.list_prefix("/deployments/").await?;
    let mut deployments: HashMap<String, ModelDeployment> = HashMap::new();
    for (_, val, _) in &deployment_kvs {
        if let Ok(dep) = serde_json::from_slice::<ModelDeployment>(val) {
            if dep.desired_state == DesiredState::Running {
                deployments.insert(dep.model_uid.clone(), dep);
            }
        }
    }

    // 4. Load stats from etcd (control-plane hot path for autoscaling)
    let stats_by_model = fetch_stats_from_etcd(store).await?;

    // 5. For each placement, reconcile
    for plan in &plans {
        let has_roled_assignments = plan
            .assignments
            .iter()
            .any(|a| a.role != nebula_common::ReplicaRole::Mixed);
        let (base_replicas, min_replicas, max_replicas) =
            if let Some(dep) = deployments.get(&plan.model_uid) {
                let base = dep.replicas.max(1);
                if dep.topology == Some(nebula_common::DeploymentTopology::PdSplit)
                    || has_roled_assignments
                {
                    // pd_split v1 forbids role-blind autoscaling (pool.md §pd_split v1.1).
                    (base, base, base)
                } else {
                    let min = dep.min_replicas.unwrap_or(1).max(1);
                    // Ensure max >= min so clamp never panics on empty/misconfigured bounds.
                    let max = dep.max_replicas.unwrap_or(base).max(min);
                    (base, min, max)
                }
            } else {
                // Orphan placement (deployment gone / stopped): hold current count, allow 0.
                let n = plan.assignments.len() as u32;
                (n, 0, n)
            };

        // Compute desired replicas: start from base, adjust by load signals
        let current_replicas = plan.assignments.len() as u32;
        let desired_replicas = compute_desired_replicas(
            base_replicas,
            min_replicas,
            max_replicas,
            plan,
            stats_by_model.get(&plan.model_uid),
            now,
        );

        if desired_replicas > current_replicas {
            metrics.scale_up_total.fetch_add(1, Ordering::Relaxed);
        }

        // Identify healthy vs stale assignments
        let mut healthy_assignments = Vec::new();
        let mut stale_replica_ids = Vec::new();
        let mut stale_assignments = Vec::new();

        for assignment in &plan.assignments {
            let key = (plan.model_uid.clone(), assignment.replica_id);
            match assignment_disposition(endpoints.get(&key), now) {
                AssignmentDisposition::Keep => {
                    healthy_assignments.push(assignment.clone());
                }
                AssignmentDisposition::RemoveStale => {
                    let ep = endpoints.get(&key);
                    if ep.is_some_and(|e| e.status == EndpointStatus::Failed) {
                        warn!(
                            model_uid=%plan.model_uid,
                            replica_id=assignment.replica_id,
                            "endpoint failed, removing assignment for replacement"
                        );
                    } else {
                        let age = ep
                            .map(|e| now.saturating_sub(e.last_heartbeat_ms))
                            .unwrap_or(0);
                        warn!(
                            model_uid=%plan.model_uid,
                            replica_id=assignment.replica_id,
                            age_ms=age,
                            status=?ep.map(|e| &e.status),
                            "endpoint heartbeat timed out, removing assignment"
                        );
                    }
                    stale_replica_ids.push(assignment.replica_id);
                    stale_assignments.push(assignment.clone());
                    metrics
                        .unhealthy_endpoints_total
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Clean up stale endpoint / stats keys from etcd
        for replica_id in &stale_replica_ids {
            let ep_key = format!("/endpoints/{}/{}", plan.model_uid, replica_id);
            let _ = store.delete(&ep_key).await;
            let stats_key = format!("/stats/{}/{}", plan.model_uid, replica_id);
            let _ = store.delete(&stats_key).await;
        }

        let need_update =
            !stale_replica_ids.is_empty() || (healthy_assignments.len() as u32) != desired_replicas;

        if !need_update {
            continue;
        }

        let mut new_assignments = healthy_assignments.clone();
        let current_count = new_assignments.len() as u32;

        if current_count < desired_replicas {
            let deficit = desired_replicas - current_count;
            info!(
                model_uid=%plan.model_uid,
                current=current_count,
                desired=desired_replicas,
                deficit=deficit,
                "attempting to add replacement replicas"
            );

            if let Some(dep) = deployments.get(&plan.model_uid) {
                add_replacement_replicas_from_plan(
                    store,
                    plan,
                    Some(dep),
                    deficit,
                    default_port,
                    &stale_assignments,
                    &mut new_assignments,
                )
                .await;
            }
        } else if current_count > desired_replicas {
            let remove_count = (current_count - desired_replicas) as usize;
            let remove_ids = select_replicas_to_remove(
                &new_assignments,
                remove_count,
                stats_by_model.get(&plan.model_uid),
            );
            info!(
                model_uid=%plan.model_uid,
                current=current_count,
                desired=desired_replicas,
                remove_ids=?remove_ids,
                "scaling down: removing assignments"
            );
            new_assignments.retain(|a| !remove_ids.contains(&a.replica_id));
            if new_assignments.len() < plan.assignments.len() {
                metrics.scale_down_total.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Skip CAS if assignment set is unchanged (e.g. scale-up could not place).
        let same_as_plan = new_assignments.len() == plan.assignments.len()
            && new_assignments.iter().all(|a| {
                plan.assignments
                    .iter()
                    .any(|b| b.replica_id == a.replica_id && b.node_id == a.node_id)
            });
        if same_as_plan {
            continue;
        }

        // Write updated plan (bump logical version; stamp wall time separately)
        let updated_plan = PlacementPlan {
            request_id: plan.request_id.clone(),
            model_uid: plan.model_uid.clone(),
            model_name: plan.model_name.clone(),
            version: nebula_common::next_placement_version(plan.version),
            updated_at_ms: now_ms(),
            leader_epoch: leader.epoch(),
            assignments: new_assignments,
        };

        let placement_key = format!("/placements/{}", plan.model_uid);

        // Find the revision of the plan we originally read
        let expected_revision = placement_kvs
            .iter()
            .find(|(key, _, _)| key == &placement_key)
            .map(|(_, _, revision)| *revision)
            .unwrap_or(0);

        match serde_json::to_vec(&updated_plan) {
            Ok(val) => {
                if !leader.is_leader() {
                    warn!(
                        model_uid=%plan.model_uid,
                        "lost leadership before placement CAS; skipping"
                    );
                    continue;
                }
                // Use CAS: ensure the key has not been modified since we read it
                match store
                    .compare_and_swap(&placement_key, expected_revision, val)
                    .await
                {
                    Ok((true, _revision)) => {
                        info!(
                            model_uid=%plan.model_uid,
                            old_assignments=plan.assignments.len(),
                            new_assignments=updated_plan.assignments.len(),
                            "reconcile: updated placement (CAS)"
                        );
                    }
                    Ok((false, current_revision)) => {
                        metrics
                            .placement_cas_conflict_total
                            .fetch_add(1, Ordering::Relaxed);
                        warn!(
                            model_uid=%plan.model_uid,
                            expected_revision,
                            current_revision,
                            "reconcile: CAS conflict, skipping cycle"
                        );
                    }
                    Err(e) => {
                        warn!(model_uid=%plan.model_uid, error=%e, "reconcile: CAS update failed, skipping cycle");
                    }
                }
            }
            Err(e) => {
                warn!(model_uid=%plan.model_uid, error=%e, "failed to serialize updated placement");
            }
        }
    }

    Ok(())
}

/// Resolve role and kv_sharing for a replacement replica from the existing plan or deployment.
fn replacement_role_and_kv(
    plan: &PlacementPlan,
    dep: Option<&ModelDeployment>,
    replica_id: u32,
    active_assignments: &[nebula_common::PlacementAssignment],
) -> (
    nebula_common::ReplicaRole,
    Option<nebula_common::KvSharingSpec>,
) {
    if let Some(existing) = plan.assignments.iter().find(|a| a.replica_id == replica_id) {
        if existing.role != nebula_common::ReplicaRole::Mixed || existing.kv_sharing.is_some() {
            return (existing.role, existing.kv_sharing.clone());
        }
    }
    let mut role = dep
        .and_then(|d| d.replica_specs.as_ref())
        .and_then(|specs| specs.get(replica_id as usize))
        .and_then(|s| s.role)
        .unwrap_or(nebula_common::ReplicaRole::Mixed);
    let is_pd_split = dep
        .map(|d| d.topology == Some(nebula_common::DeploymentTopology::PdSplit))
        .unwrap_or(false)
        || plan
            .assignments
            .iter()
            .any(|a| a.role != nebula_common::ReplicaRole::Mixed);
    if is_pd_split && role == nebula_common::ReplicaRole::Mixed {
        let prefill_count = active_assignments
            .iter()
            .filter(|a| a.role == nebula_common::ReplicaRole::Prefill)
            .count();
        let decode_count = active_assignments
            .iter()
            .filter(|a| a.role == nebula_common::ReplicaRole::Decode)
            .count();
        role = if prefill_count <= decode_count {
            nebula_common::ReplicaRole::Prefill
        } else {
            nebula_common::ReplicaRole::Decode
        };
    }
    let kv_sharing = dep
        .and_then(|d| d.kv_sharing.clone())
        .or_else(|| plan.assignments.first().and_then(|a| a.kv_sharing.clone()));
    (role, kv_sharing)
}

/// Allocate the lowest unused `replica_id` starting from `0` so replacement
/// replicas fill missing `0..replicas` slots and align with `replica_specs[id]`.
fn next_unused_replica_id(
    used_replica_ids: &mut std::collections::HashSet<u32>,
    cursor: &mut u32,
) -> u32 {
    while used_replica_ids.contains(cursor) {
        *cursor = cursor.saturating_add(1);
    }
    let id = *cursor;
    used_replica_ids.insert(id);
    *cursor = cursor.saturating_add(1);
    id
}

/// Add replacement replicas for deployment-managed plans.
/// Re-uses existing plan assignments' extra_args/engine_type/docker_image as template.
async fn add_replacement_replicas_from_plan(
    store: &EtcdMetaStore,
    plan: &PlacementPlan,
    dep: Option<&ModelDeployment>,
    deficit: u32,
    default_port: u16,
    stale_templates: &[nebula_common::PlacementAssignment],
    new_assignments: &mut Vec<nebula_common::PlacementAssignment>,
) {
    let (mut used_ports, mut used_gpus) = list_used_resources(store).await.unwrap_or_default();

    for a in new_assignments.iter() {
        used_ports.insert(a.port);
        if let Some(indices) = a.effective_gpu_indices() {
            let entry = used_gpus.entry(a.node_id.clone()).or_default();
            for idx in indices {
                entry.insert(idx);
            }
        }
    }

    // Use the first existing assignment as a template for extra_args/engine_type/docker_image
    let template = plan.assignments.first();
    let extra_args = template.and_then(|a| a.extra_args.clone());
    let engine_type = template.and_then(|a| a.engine_type.clone());
    let docker_image = template.and_then(|a| a.docker_image.clone());

    // Build a minimal ModelRequest-like struct for node selection.
    // We use the plan's existing config (from extra_args) to infer required resources.
    let dummy_req = ModelRequest {
        id: String::new(),
        request: nebula_common::ModelLoadRequest {
            model_name: plan.model_name.clone(),
            model_uid: plan.model_uid.clone(),
            replicas: 1,
            config: None,
            node_id: None,
            gpu_index: None,
            gpu_indices: None,
            engine_type: engine_type.clone(),
            docker_image: docker_image.clone(),
            min_replicas: None,
            max_replicas: None,
            replica_specs: None,
            callback_url: None,
        },
        status: ModelRequestStatus::Scheduled,
        created_at_ms: 0,
    };

    let mut used_replica_ids: std::collections::HashSet<u32> =
        new_assignments.iter().map(|a| a.replica_id).collect();
    let mut next_new_id = 0_u32;

    for i in 0..deficit {
        if let Some(stale) = stale_templates.get(i as usize) {
            let port = if used_ports.contains(&stale.port) {
                allocate_port(default_port, &used_ports)
            } else {
                stale.port
            };
            used_ports.insert(port);

            if let Some(indices) = stale.effective_gpu_indices() {
                let entry = used_gpus.entry(stale.node_id.clone()).or_default();
                for idx in indices {
                    entry.insert(idx);
                }
            }

            new_assignments.push(nebula_common::PlacementAssignment {
                replica_id: stale.replica_id,
                node_id: stale.node_id.clone(),
                engine_config_path: stale.engine_config_path.clone(),
                port,
                gpu_index: stale.gpu_index,
                gpu_indices: stale.gpu_indices.clone(),
                extra_args: stale.extra_args.clone().or(extra_args.clone()),
                engine_type: stale.engine_type.clone().or(engine_type.clone()),
                docker_image: stale.docker_image.clone().or(docker_image.clone()),
                pool_id: stale.pool_id.clone(),
                role: stale.role,
                kv_sharing: stale.kv_sharing.clone(),
            });
            used_replica_ids.insert(stale.replica_id);

            info!(
                model_uid=%plan.model_uid,
                replica_id=stale.replica_id,
                node_id=%stale.node_id,
                "replaced assignment reusing stale replica placement"
            );
            continue;
        }

        let new_replica_id = next_unused_replica_id(&mut used_replica_ids, &mut next_new_id);

        match select_node_and_gpus(store, &dummy_req, &used_gpus).await {
            Ok((node_id, gpu_indices)) => {
                let port = allocate_port(default_port, &used_ports);
                used_ports.insert(port);

                if !gpu_indices.is_empty() {
                    let entry = used_gpus.entry(node_id.clone()).or_default();
                    for &idx in &gpu_indices {
                        entry.insert(idx);
                    }
                }

                let gpu_index = if gpu_indices.len() == 1 {
                    Some(gpu_indices[0])
                } else {
                    gpu_indices.first().copied()
                };
                let gpu_indices_field = if gpu_indices.is_empty() {
                    None
                } else {
                    Some(gpu_indices)
                };

                let (role, kv_sharing) =
                    replacement_role_and_kv(plan, dep, new_replica_id, new_assignments);
                let rep_image = if let Some(d) = dep {
                    let raw = nebula_common::effective_replica_image(
                        d,
                        docker_image.as_deref(),
                        new_replica_id as usize,
                    );
                    match crate::planner::resolve_concrete_image(store, raw).await {
                        Ok(resolved) => resolved.or_else(|| docker_image.clone()),
                        Err(e) => {
                            warn!(
                                model_uid=%plan.model_uid,
                                replica_id=new_replica_id,
                                error=%e,
                                "failed to resolve replacement replica image from /images/; skipping replacement tick"
                            );
                            break;
                        }
                    }
                } else {
                    docker_image.clone()
                };
                if dep
                    .map(|d| d.topology == Some(nebula_common::DeploymentTopology::PdSplit))
                    .unwrap_or(false)
                    || role != nebula_common::ReplicaRole::Mixed
                {
                    let approved = nebula_common::approved_pd_images_from_env();
                    if let Err(reason) =
                        nebula_common::validate_pd_split_image(rep_image.as_deref(), &approved)
                    {
                        warn!(
                            model_uid=%plan.model_uid,
                            replica_id=new_replica_id,
                            reason=%reason,
                            "replacement replica image rejected by pd_split allowlist; skipping"
                        );
                        break;
                    }
                }

                new_assignments.push(nebula_common::PlacementAssignment {
                    replica_id: new_replica_id,
                    node_id,
                    engine_config_path: format!("/tmp/nebula/{}.yaml", plan.model_uid),
                    port,
                    gpu_index,
                    gpu_indices: gpu_indices_field,
                    extra_args: extra_args.clone(),
                    engine_type: engine_type.clone(),
                    docker_image: rep_image,
                    pool_id: None,
                    role,
                    kv_sharing,
                });

                info!(
                    model_uid=%plan.model_uid,
                    replica_id=new_replica_id,
                    "added replacement assignment (from deployment)"
                );
            }
            Err(e) => {
                warn!(
                    model_uid=%plan.model_uid,
                    error=%e,
                    "failed to find node for replacement replica"
                );
                break;
            }
        }
    }
}

/// P0-2: load EndpointStats from etcd `/stats/`, grouped by model_uid.
async fn fetch_stats_from_etcd(
    store: &EtcdMetaStore,
) -> anyhow::Result<HashMap<String, Vec<EndpointStats>>> {
    let mut stats_by_model: HashMap<String, Vec<EndpointStats>> = HashMap::new();
    let kvs = store.list_prefix("/stats/").await?;
    for (_, val, _) in kvs {
        let Ok(stats) = serde_json::from_slice::<EndpointStats>(&val) else {
            continue;
        };
        stats_by_model
            .entry(stats.model_uid.clone())
            .or_default()
            .push(stats);
    }
    Ok(stats_by_model)
}

/// Compute the desired replica count based on load signals and autoscaling bounds.
fn compute_desired_replicas(
    base_replicas: u32,
    min_replicas: u32,
    max_replicas: u32,
    plan: &PlacementPlan,
    stats: Option<&Vec<EndpointStats>>,
    now: u64,
) -> u32 {
    // Role-bearing placements (pd_split) forbid automatic scale-up / scale-down
    // in v1 regardless of bounds or load (pool.md §pd_split v1.1).
    if plan
        .assignments
        .iter()
        .any(|a| a.role != nebula_common::ReplicaRole::Mixed)
    {
        return base_replicas;
    }

    let current = plan.assignments.len() as u32;
    // Normalize bounds: `u32::clamp` panics when min > max.
    let min_replicas = min_replicas.min(max_replicas);
    let max_replicas = max_replicas.max(min_replicas);

    // If no autoscaling range (min == max or no stats), use base
    if min_replicas >= max_replicas {
        return base_replicas.clamp(min_replicas, max_replicas);
    }

    let stats = match stats {
        Some(s) if !s.is_empty() => s,
        _ => return current.clamp(min_replicas, max_replicas),
    };

    // Compute average KV cache usage fraction
    let mut kv_count = 0u32;
    let mut kv_usage_sum = 0.0f64;
    for s in stats.iter() {
        if let Some(usage) = s.kv_cache_usage {
            kv_usage_sum += usage;
            kv_count += 1;
        }
    }
    let avg_kv_usage = if kv_count > 0 {
        kv_usage_sum / kv_count as f64
    } else {
        0.0
    };

    // Compute average pending requests
    let avg_pending: f64 =
        stats.iter().map(|s| s.pending_requests as f64).sum::<f64>() / stats.len() as f64;

    // Scale-up: if avg KV cache usage > threshold OR avg pending > threshold
    if avg_kv_usage > SCALE_UP_KV_THRESHOLD || avg_pending > SCALE_UP_PENDING_THRESHOLD {
        let target = (current + 1).min(max_replicas);
        if target > current {
            info!(
                model_uid=%plan.model_uid,
                avg_kv_usage=format!("{:.2}", avg_kv_usage),
                avg_pending=format!("{:.1}", avg_pending),
                current=current,
                target=target,
                "autoscale: scaling up"
            );
            return target;
        }
    }

    // Scale-down: if avg pending == 0 and KV usage is low, and cooldown has passed
    if avg_pending <= SCALE_DOWN_IDLE_THRESHOLD && avg_kv_usage < SCALE_UP_KV_THRESHOLD * 0.5 {
        let plan_age = now.saturating_sub(plan.effective_updated_at_ms());
        if plan_age > SCALE_DOWN_COOLDOWN_MS && current > min_replicas {
            let target = (current - 1).max(min_replicas);
            info!(
                model_uid=%plan.model_uid,
                avg_kv_usage=format!("{:.2}", avg_kv_usage),
                avg_pending=format!("{:.1}", avg_pending),
                current=current,
                target=target,
                plan_age_ms=plan_age,
                "autoscale: scaling down"
            );
            return target;
        }
    }

    // No change needed
    current.clamp(min_replicas, max_replicas)
}

#[cfg(test)]
mod assignment_tests {
    use super::*;
    use nebula_common::{
        DeploymentTopology, EndpointInfo, EndpointKind, KvSharingSpec, PlacementAssignment,
        ReplicaPlacementSpec, ReplicaRole,
    };

    fn sample_ep(status: EndpointStatus, last_heartbeat_ms: u64) -> EndpointInfo {
        EndpointInfo {
            model_uid: "m".into(),
            replica_id: 0,
            plan_version: 1,
            node_id: "n1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status,
            last_heartbeat_ms,
            status_detail: None,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:10824".into()),
            engine_type: None,
            role: Default::default(),
        }
    }

    fn pd_assignment(replica_id: u32, role: ReplicaRole) -> PlacementAssignment {
        PlacementAssignment {
            replica_id,
            node_id: "n1".into(),
            engine_config_path: "/tmp/m.yaml".into(),
            port: 8000 + replica_id as u16,
            gpu_index: Some(replica_id),
            gpu_indices: Some(vec![replica_id]),
            extra_args: None,
            engine_type: Some("vllm".into()),
            docker_image: Some("nebula/vllm:0.23.0-lmcache0.5.5".into()),
            pool_id: None,
            role,
            kv_sharing: Some(KvSharingSpec {
                remote_url: "lm://store:6800".into(),
                serde: "naive".into(),
            }),
        }
    }

    #[test]
    fn unhealthy_keeps_assignment() {
        let now = 1_000_000;
        let ep = sample_ep(EndpointStatus::Unhealthy, now - 5_000);
        assert_eq!(
            assignment_disposition(Some(&ep), now),
            AssignmentDisposition::Keep
        );
    }

    #[test]
    fn failed_removes_assignment() {
        let now = 1_000_000;
        let ep = sample_ep(EndpointStatus::Failed, now - 5_000);
        assert_eq!(
            assignment_disposition(Some(&ep), now),
            AssignmentDisposition::RemoveStale
        );
    }

    #[test]
    fn missing_endpoint_keeps_assignment() {
        assert_eq!(
            assignment_disposition(None, 1_000_000),
            AssignmentDisposition::Keep
        );
    }

    #[test]
    fn pd_split_never_autoscales_down_or_up() {
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m".into(),
            model_name: "Qwen".into(),
            version: 1,
            updated_at_ms: 1_000,
            leader_epoch: 1,
            assignments: vec![
                pd_assignment(0, ReplicaRole::Prefill),
                pd_assignment(1, ReplicaRole::Decode),
            ],
        };
        let idle_stats = vec![
            EndpointStats {
                model_uid: "m".into(),
                replica_id: 0,
                last_updated_ms: 900_000,
                pending_requests: 0,
                prefix_cache_hit_rate: None,
                prefix_cache_queries_total: None,
                prompt_cache_hit_rate: None,
                kv_cache_usage: Some(0.01),
            },
            EndpointStats {
                model_uid: "m".into(),
                replica_id: 1,
                last_updated_ms: 900_000,
                pending_requests: 0,
                prefix_cache_hit_rate: None,
                prefix_cache_queries_total: None,
                prompt_cache_hit_rate: None,
                kv_cache_usage: Some(0.01),
            },
        ];
        // Even if caller passes loose min=1/max=4 and cooldown has long elapsed,
        // role-bearing placements stay at base_replicas=2.
        assert_eq!(
            compute_desired_replicas(2, 1, 4, &plan, Some(&idle_stats), 1_000_000),
            2
        );

        let overloaded_stats = vec![EndpointStats {
            pending_requests: 100,
            kv_cache_usage: Some(0.99),
            ..idle_stats[0].clone()
        }];
        assert_eq!(
            compute_desired_replicas(2, 1, 4, &plan, Some(&overloaded_stats), 1_000_000),
            2
        );
    }

    #[test]
    fn replacement_role_and_kv_preserves_pd_split_metadata_and_fills_lowest_slot() {
        let kv = KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
        };
        let dep = ModelDeployment {
            model_uid: "m".into(),
            desired_state: DesiredState::Running,
            replicas: 2,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            replica_specs: Some(vec![
                ReplicaPlacementSpec {
                    role: Some(ReplicaRole::Prefill),
                    ..Default::default()
                },
                ReplicaPlacementSpec {
                    role: Some(ReplicaRole::Decode),
                    ..Default::default()
                },
            ]),
            config_overrides: None,
            image_id: None,
            allowed_pools: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
            topology: Some(DeploymentTopology::PdSplit),
            kv_sharing: Some(kv.clone()),
            version: 1,
            updated_at_ms: 0,
        };
        let empty_plan = PlacementPlan {
            request_id: None,
            model_uid: "m".into(),
            model_name: "Qwen".into(),
            version: 1,
            updated_at_ms: 0,
            leader_epoch: 1,
            assignments: vec![pd_assignment(1, ReplicaRole::Decode)],
        };

        // Suppose replica 0 (Prefill) was lost while replica 1 (Decode) survived.
        // Slot allocation must pick 0 first (not max_existing_id + 1 = 2).
        let surviving = vec![pd_assignment(1, ReplicaRole::Decode)];
        let mut used_ids: std::collections::HashSet<u32> =
            surviving.iter().map(|a| a.replica_id).collect();
        let mut cursor = 0_u32;
        let repl_id = next_unused_replica_id(&mut used_ids, &mut cursor);
        assert_eq!(repl_id, 0);

        let (role0, kv0) = replacement_role_and_kv(&empty_plan, Some(&dep), repl_id, &surviving);
        assert_eq!(role0, ReplicaRole::Prefill);
        assert_eq!(kv0, Some(kv.clone()));

        // Even if replica_id exceeds replica_specs.len(), pd_split replacement
        // recovers the missing role instead of falling back to Mixed.
        let (role_oob, kv_oob) = replacement_role_and_kv(&empty_plan, Some(&dep), 5, &surviving);
        assert_eq!(role_oob, ReplicaRole::Prefill);
        assert_eq!(kv_oob, Some(kv));
    }
}
