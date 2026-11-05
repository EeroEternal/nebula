//! Durable, opt-in isolated experiments. PostgreSQL serializes controllers; etcd
//! revisions fence configuration writes. Durable intents make crash recovery idempotent.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use nebula_common::autotune::{TuningObjective, TuningParameters, EXPERIMENT_LABEL};
use nebula_common::{
    BenchmarkRun, BenchmarkRunStatus, DesiredState, EndpointInfo, EndpointStatus, ModelDeployment,
    ModelSpec, NodeStatus, PlacementPlan, ReplicaPlacementSpec,
};
use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::{require_role, AuthContext, Role};
use crate::service::{now_ms, ServiceError};
use crate::state::AppState;

const LEASE_MS: u64 = 120_000;
fn default_experiment_seconds() -> u64 {
    7200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperiment {
    pub owner: Uuid,
    pub candidates: Vec<TuningParameters>,
    pub objective: TuningObjective,
    pub replica_specs: Vec<ReplicaPlacementSpec>,
    pub isolated_remote_url: Option<String>,
    pub engine_version: String,
    pub observation_seconds: u64,
    #[serde(default = "default_experiment_seconds")]
    pub max_experiment_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Provisioning,
    Ready,
    Configuring,
    Promoting,
    Observing,
    Restoring,
    Cleaning,
    Completed,
    RolledBack,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Intent {
    key: String,
    expected_revision: u64,
    value: Vec<u8>,
    guards: Vec<(String, u64)>,
    #[serde(default)]
    authorization: Option<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Measurement {
    candidate: Option<usize>,
    production: bool,
    run: BenchmarkRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Experiment {
    id: Uuid,
    owner: Uuid,
    epoch: u64,
    model_uid: String,
    experiment_uid: String,
    platform: String,
    gpu_name: String,
    phase: Phase,
    lease_until_ms: u64,
    created_at_ms: u64,
    spec: ModelSpec,
    spec_revision: u64,
    baseline: ModelDeployment,
    stable_revision: u64,
    trial: ModelDeployment,
    trial_revision: u64,
    #[serde(default)]
    trial_spec_revision: u64,
    parameters: TuningParameters,
    candidate: Option<usize>,
    request: CreateExperiment,
    measurements: Vec<Measurement>,
    pending: Vec<Intent>,
    stable_changed: bool,
    promoted_candidate: Option<usize>,
    promoted_at_ms: Option<u64>,
    reason: Option<String>,
    active: bool,
    #[serde(default)]
    trial_conflicted: bool,
    #[serde(default)]
    last_reconciled_at_ms: u64,
    #[serde(default)]
    trial_may_have_started: bool,
    #[serde(default)]
    stopped_at_ms: Option<u64>,
    #[serde(default)]
    cleanup_wait_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Heartbeat,
    Configure { candidate: Option<usize> },
    Record { run_id: String },
    Promote { candidate: usize },
    Finish,
    Rollback,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    owner: Uuid,
    epoch: u64,
    #[serde(rename = "operation")]
    action: Action,
}

fn bad(message: impl Into<String>) -> ServiceError {
    ServiceError::BadRequest(message.into())
}

fn conflict(message: impl Into<String>) -> ServiceError {
    ServiceError::Conflict(message.into())
}

fn db_error(error: sqlx::Error) -> ServiceError {
    tracing::error!(error=%error, "autotune database operation failed");
    ServiceError::Internal("autotune state unavailable".into())
}

async fn read<T: serde::de::DeserializeOwned>(
    store: &dyn MetaStore,
    key: &str,
) -> Result<(T, u64), ServiceError> {
    let (data, revision) = store
        .get(key)
        .await?
        .ok_or_else(|| ServiceError::NotFound(key.into()))?;
    Ok((serde_json::from_slice(&data)?, revision))
}

pub async fn initialize(db: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bff_autotune_experiments (
            id UUID PRIMARY KEY, model_uid TEXT NOT NULL, active BOOLEAN NOT NULL,
            state_json JSONB NOT NULL, updated_at_ms BIGINT NOT NULL
        )",
    )
    .execute(db)
    .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS bff_autotune_one_active_model
         ON bff_autotune_experiments(model_uid) WHERE active",
    )
    .execute(db)
    .await?;
    Ok(())
}

async fn save(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &Experiment,
) -> Result<(), ServiceError> {
    sqlx::query(
        "UPDATE bff_autotune_experiments SET active=$2, state_json=$3, updated_at_ms=$4 WHERE id=$1",
    )
    .bind(job.id)
    .bind(job.active)
    .bind(serde_json::to_value(job)?)
    .bind(now_ms() as i64)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

fn next_version(dep: &mut ModelDeployment) -> Result<(), ServiceError> {
    dep.version = dep
        .version
        .checked_add(1)
        .ok_or_else(|| bad("deployment version exhausted"))?;
    dep.updated_at_ms = now_ms();
    Ok(())
}

fn intent(key: String, revision: u64, dep: &impl Serialize) -> Result<Intent, ServiceError> {
    Ok(Intent {
        key,
        expected_revision: revision,
        value: serde_json::to_vec(dep)?,
        guards: vec![],
        authorization: None,
    })
}

async fn authorize_intent(
    store: &dyn MetaStore,
    job: &Experiment,
    mut write: Intent,
) -> Result<Intent, ServiceError> {
    let deadline = job
        .lease_until_ms
        .min(job.created_at_ms + job.request.max_experiment_seconds * 1000);
    let remaining = deadline.saturating_sub(now_ms());
    if remaining <= 20_000 {
        return Err(conflict(
            "insufficient lease budget to authorize configuration",
        ));
    }
    // put() grants a server-side lease before attaching the key. The margin
    // exceeds the entire RPC timeout, so authorization expires before the PG
    // deadline even when grant/put consume their full budget.
    let ttl = (remaining - 15_000) / 1000 * 1000;
    let key = format!("/autotune/permits/{}", Uuid::new_v4());
    let revision = tokio::time::timeout(
        Duration::from_secs(10),
        store.put(
            &key,
            format!("{}:{}:{}", job.id, job.owner, job.epoch).into_bytes(),
            Some(ttl),
        ),
    )
    .await
    .map_err(|_| ServiceError::Internal("intent authorization timed out".into()))??;
    if now_ms() >= deadline.saturating_sub(5_000) {
        return Err(conflict(
            "configuration authorization exhausted its deadline",
        ));
    }
    write.authorization = Some((key, revision));
    Ok(write)
}

fn require_fixed_placement(dep: &ModelDeployment) -> Result<(), ServiceError> {
    let replicas = dep
        .replica_specs
        .as_ref()
        .ok_or_else(|| bad("autotuning requires explicitly pinned production nodes and GPUs"))?;
    nebula_common::validate_replica_specs(dep.replicas, replicas).map_err(bad)?;
    if replicas.iter().any(|replica| {
        replica.node_id.as_deref().is_none_or(str::is_empty)
            || replica.gpu_indices.as_ref().is_none_or(Vec::is_empty)
    }) {
        return Err(bad(
            "all production replicas must pin their nodes and GPU indices",
        ));
    }
    Ok(())
}

fn production_intent(job: &Experiment, dep: &ModelDeployment) -> Result<Intent, ServiceError> {
    let mut write = intent(
        format!("/deployments/{}", job.model_uid),
        job.stable_revision,
        dep,
    )?;
    write
        .guards
        .push((format!("/models/{}/spec", job.model_uid), job.spec_revision));
    Ok(write)
}

fn trial_intent(job: &Experiment, dep: &ModelDeployment) -> Result<Intent, ServiceError> {
    let mut write = intent(
        format!("/deployments/{}", job.experiment_uid),
        job.trial_revision,
        dep,
    )?;
    write.guards.push((
        format!("/models/{}/spec", job.experiment_uid),
        job.trial_spec_revision,
    ));
    Ok(write)
}

fn note(job: &Experiment, production: bool) -> Result<String, ServiceError> {
    let version = if production {
        job.baseline.version + 1
    } else {
        job.trial.version
    };
    let parameters = serde_json::to_value(&job.parameters)?;
    Ok(format!("autotune:{}:{version}:{parameters}", job.id))
}

async fn unchanged(store: &dyn MetaStore, job: &Experiment) -> Result<(), ServiceError> {
    for (key, expected) in [
        (format!("/models/{}/spec", job.model_uid), job.spec_revision),
        (
            format!("/deployments/{}", job.model_uid),
            job.stable_revision,
        ),
    ] {
        if store
            .get(&key)
            .await?
            .is_none_or(|(_, revision)| revision != expected)
        {
            return Err(conflict(
                "production configuration changed; refusing automatic mutation",
            ));
        }
    }
    Ok(())
}

async fn trial_owned(store: &dyn MetaStore, job: &Experiment) -> Result<(), ServiceError> {
    for (key, expected) in [
        (
            format!("/models/{}/spec", job.experiment_uid),
            job.trial_spec_revision,
        ),
        (
            format!("/deployments/{}", job.experiment_uid),
            job.trial_revision,
        ),
    ] {
        if store
            .get(&key)
            .await?
            .map_or(expected != 0, |(_, revision)| revision != expected)
        {
            return Err(conflict(
                "trial configuration changed; refusing automatic mutation",
            ));
        }
    }
    Ok(())
}

async fn ready(
    store: &dyn MetaStore,
    spec: &ModelSpec,
    dep: &ModelDeployment,
) -> Result<(), ServiceError> {
    let uid = &dep.model_uid;
    let (current, _) = read::<ModelDeployment>(store, &format!("/deployments/{uid}")).await?;
    let mut actual = serde_json::to_value(&current)?;
    let mut expected = serde_json::to_value(dep)?;
    actual.as_object_mut().unwrap().remove("updated_at_ms");
    expected.as_object_mut().unwrap().remove("updated_at_ms");
    if actual != expected {
        return Err(conflict(
            "deployment no longer matches the measured configuration",
        ));
    }
    let (plan, _) = read::<PlacementPlan>(store, &format!("/placements/{uid}")).await?;
    if plan.model_uid != *uid || plan.assignments.len() != dep.replicas as usize {
        return Err(conflict("waiting for complete placement"));
    }
    let endpoints = store.list_prefix(&format!("/endpoints/{uid}/")).await?;
    for assignment in &plan.assignments {
        let config = nebula_common::effective_replica_config(
            dep,
            spec.config.as_ref(),
            assignment.replica_id as usize,
        );
        let expected_args = nebula_common::build_engine_extra_args(
            spec.engine_type.as_deref(),
            &config.unwrap_or_default(),
        )
        .map_err(bad)?;
        let pinned = dep
            .replica_specs
            .as_ref()
            .and_then(|replicas| replicas.get(assignment.replica_id as usize));
        if assignment.extra_args != expected_args
            || assignment.kv_sharing != dep.kv_sharing
            || assignment.docker_image.as_deref()
                != dep.image_id.as_deref().or(spec.docker_image.as_deref())
            || pinned.is_some_and(|replica| {
                replica.node_id.as_deref() != Some(&assignment.node_id)
                    || replica.gpu_indices != assignment.effective_gpu_indices()
                    || replica.role.unwrap_or_default() != assignment.role
            })
        {
            return Err(conflict(
                "placement has not applied the requested configuration",
            ));
        }
        let found = endpoints.iter().any(|(_, bytes, _)| {
            serde_json::from_slice::<EndpointInfo>(bytes).is_ok_and(|ep| {
                ep.replica_id == assignment.replica_id
                    && ep.model_uid == *uid
                    && ep.plan_version == plan.version
                    && ep.node_id == assignment.node_id
                    && ep.role == assignment.role
                    && ep.status == EndpointStatus::Ready
                    && now_ms().saturating_sub(ep.last_heartbeat_ms) < 30_000
            })
        });
        if !found {
            return Err(conflict("waiting for fresh ready endpoints"));
        }
    }
    Ok(())
}

async fn validate_resources(
    store: &dyn MetaStore,
    stable: &PlacementPlan,
    request: &CreateExperiment,
) -> Result<(String, String), ServiceError> {
    if request.replica_specs.len() != stable.assignments.len() {
        return Err(bad(
            "trial replica count must match the production placement",
        ));
    }
    let mut occupied = HashSet::new();
    for (_, bytes, _) in store.list_prefix("/placements/").await? {
        let plan: PlacementPlan = serde_json::from_slice(&bytes)?;
        for assignment in plan.assignments {
            for gpu in assignment.effective_gpu_indices().unwrap_or_default() {
                occupied.insert((assignment.node_id.clone(), gpu));
            }
        }
    }
    let mut reserved = HashSet::new();
    let mut hardware = None;
    for (replica, source) in request.replica_specs.iter().zip(&stable.assignments) {
        let node_id = replica
            .node_id
            .as_deref()
            .ok_or_else(|| bad("explicit trial node required"))?;
        let gpus = replica
            .gpu_indices
            .as_ref()
            .filter(|x| !x.is_empty())
            .ok_or_else(|| bad("explicit spare trial GPUs required"))?;
        if replica.config_overrides.is_some() || replica.image_id.is_some() {
            return Err(bad(
                "trial resources cannot override images or model configuration",
            ));
        }
        if gpus.len() != source.effective_gpu_indices().unwrap_or_default().len() {
            return Err(bad("trial GPU count must match each production replica"));
        }
        let (node, _) = read::<NodeStatus>(store, &format!("/nodes/{node_id}/status")).await?;
        let (source_node, _) =
            read::<NodeStatus>(store, &format!("/nodes/{}/status", source.node_id)).await?;
        if now_ms().saturating_sub(node.last_heartbeat_ms) > 30_000
            || now_ms().saturating_sub(source_node.last_heartbeat_ms) > 30_000
            || node.platform != source_node.platform
        {
            return Err(bad(
                "trial node must have fresh inventory and the same platform",
            ));
        }
        for (index, source_index) in gpus
            .iter()
            .zip(source.effective_gpu_indices().unwrap_or_default())
        {
            if occupied.contains(&(node_id.to_string(), *index))
                || !reserved.insert((node_id.to_string(), *index))
            {
                return Err(conflict("trial GPU is already assigned"));
            }
            let gpu = node
                .gpus
                .iter()
                .find(|g| g.index == *index)
                .ok_or_else(|| bad("trial GPU absent"))?;
            if gpu.memory_used_mb != 0 {
                return Err(conflict(
                    "trial GPU memory must be zero; use an idle dedicated GPU",
                ));
            }
            let source_gpu = source_node
                .gpus
                .iter()
                .find(|g| g.index == source_index)
                .ok_or_else(|| bad("production GPU inventory unavailable"))?;
            if gpu.name.is_none()
                || gpu.name != source_gpu.name
                || gpu.memory_total_mb != source_gpu.memory_total_mb
            {
                return Err(bad("trial requires the same known GPU model and capacity"));
            }
            let current = (
                node.platform
                    .clone()
                    .ok_or_else(|| bad("explicit platform required"))?,
                gpu.name.clone().unwrap(),
            );
            if hardware.as_ref().is_some_and(|known| known != &current) {
                return Err(bad(
                    "autotuning currently requires homogeneous GPU hardware",
                ));
            }
            hardware = Some(current);
        }
    }
    hardware.ok_or_else(|| bad("no trial hardware"))
}

async fn validate_memory(
    store: &dyn MetaStore,
    dep: &ModelDeployment,
    replicas: &[ReplicaPlacementSpec],
    candidates: &[TuningParameters],
    experiment: bool,
) -> Result<(), ServiceError> {
    let Some(kv) = dep.kv_sharing.as_ref() else {
        return Ok(());
    };
    let baseline = kv.local_cpu_size_gb.unwrap_or(5);
    let maximum = candidates
        .iter()
        .filter_map(|p| p.local_cpu_size_gb)
        .max()
        .unwrap_or(baseline)
        .max(baseline);
    let per_replica = if experiment {
        maximum
    } else {
        maximum.saturating_sub(baseline)
    };
    let mut reservations = std::collections::BTreeMap::<String, u64>::new();
    for replica in replicas {
        let node = replica
            .node_id
            .clone()
            .ok_or_else(|| bad("explicit node required"))?;
        *reservations.entry(node).or_default() += per_replica as u64 * 1024;
    }
    for (node, required_mb) in reservations {
        if required_mb == 0 {
            continue;
        }
        let (status, _) = read::<NodeStatus>(store, &format!("/nodes/{node}/status")).await?;
        if now_ms().saturating_sub(status.last_heartbeat_ms) > 30_000 {
            return Err(bad("fresh host DRAM inventory required"));
        }
        let available = status
            .host_memory_available_mb
            .ok_or_else(|| bad("host DRAM inventory required"))?;
        if required_mb > available.saturating_mul(7) / 10 {
            return Err(bad(
                "candidate L1 memory exceeds 70% of available host DRAM",
            ));
        }
    }
    Ok(())
}

pub async fn create(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(uid): Path<String>,
    Json(request): Json<CreateExperiment>,
) -> Result<Response, ServiceError> {
    if let Some(response) = require_role(&ctx, Role::Operator) {
        return Ok(response);
    }
    request.objective.validate().map_err(bad)?;
    if request.candidates.is_empty()
        || request.candidates.len() > 16
        || request.engine_version.trim().is_empty()
        || request.engine_version.len() > 64
        || !(30..=3600).contains(&request.observation_seconds)
        || !(300..=86_400).contains(&request.max_experiment_seconds)
    {
        return Err(bad(
            "provide 1..16 candidates and an explicit engine version",
        ));
    }
    let mut tx = st.db.begin().await.map_err(db_error)?;
    // Serialize resource reservations before the Scheduler has materialized placements.
    sqlx::query("SELECT pg_advisory_xact_lock(726562854231)")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let (spec, spec_revision) =
        read::<ModelSpec>(&*st.store, &format!("/models/{uid}/spec")).await?;
    let (baseline, stable_revision) =
        read::<ModelDeployment>(&*st.store, &format!("/deployments/{uid}")).await?;
    if spec.labels.contains_key(EXPERIMENT_LABEL) {
        return Err(bad("cannot recursively tune an experimental deployment"));
    }
    if baseline.desired_state != DesiredState::Running
        || baseline
            .min_replicas
            .is_some_and(|n| n != baseline.replicas)
        || baseline
            .max_replicas
            .is_some_and(|n| n != baseline.replicas)
    {
        return Err(bad("autotuning requires a running, fixed-size deployment"));
    }
    require_fixed_placement(&baseline)?;
    let parameters = TuningParameters::baseline(&spec, &baseline).map_err(bad)?;
    for candidate in &request.candidates {
        candidate.apply(&spec, &baseline).map_err(bad)?;
        if candidate.pin_timeout_sec != parameters.pin_timeout_sec {
            return Err(bad(
                "pin TTL must remain unchanged until a tool-pause benchmark is available",
            ));
        }
    }
    ready(&*st.store, &spec, &baseline).await?;
    let (plan, _) = read::<PlacementPlan>(&*st.store, &format!("/placements/{uid}")).await?;
    let (platform, gpu_name) = validate_resources(&*st.store, &plan, &request).await?;
    if baseline.kv_sharing.is_some()
        && request.replica_specs.iter().any(|replica| {
            plan.assignments
                .iter()
                .any(|assignment| replica.node_id.as_deref() == Some(&assignment.node_id))
        })
    {
        return Err(bad(
            "KV trials require nodes separate from production for host-memory isolation",
        ));
    }
    validate_memory(
        &*st.store,
        &baseline,
        &request.replica_specs,
        &request.candidates,
        true,
    )
    .await?;
    let production_resources: Vec<_> = plan
        .assignments
        .iter()
        .map(|a| ReplicaPlacementSpec {
            node_id: Some(a.node_id.clone()),
            gpu_indices: a.effective_gpu_indices(),
            ..Default::default()
        })
        .collect();
    validate_memory(
        &*st.store,
        &baseline,
        &production_resources,
        &request.candidates,
        false,
    )
    .await?;
    let active = sqlx::query("SELECT state_json FROM bff_autotune_experiments WHERE active")
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
    if active.len() >= 16 {
        return Err(conflict("global active experiment budget exhausted"));
    }
    for row in active {
        let other: Experiment = serde_json::from_value(row.get("state_json"))?;
        for replica in other.trial.replica_specs.as_ref().into_iter().flatten() {
            if (baseline.kv_sharing.is_some() || other.baseline.kv_sharing.is_some())
                && request
                    .replica_specs
                    .iter()
                    .any(|requested| requested.node_id == replica.node_id)
            {
                return Err(conflict(
                    "KV experiments require exclusive experimental host-memory reservations",
                ));
            }
            if request.replica_specs.iter().any(|requested| {
                requested.node_id == replica.node_id
                    && requested.gpu_indices.as_ref().is_some_and(|gpus| {
                        replica
                            .gpu_indices
                            .as_ref()
                            .is_some_and(|used| gpus.iter().any(|g| used.contains(g)))
                    })
            }) {
                return Err(conflict(
                    "trial GPUs are reserved by another active experiment",
                ));
            }
        }
    }
    let image = nebula_control::compat::resolve_concrete_image_ref(
        &*st.store,
        baseline
            .image_id
            .as_deref()
            .or(spec.docker_image.as_deref()),
    )
    .await?
    .ok_or_else(|| bad("an immutable image digest is required"))?;
    if !baseline
        .image_id
        .as_deref()
        .or(spec.docker_image.as_deref())
        .is_some_and(|value| value == image && value.contains("@sha256:"))
    {
        return Err(bad(
            "autotuning requires an immutable @sha256 image reference",
        ));
    }
    let id = Uuid::new_v4();
    let experiment_uid = format!("at-{}", id.simple());
    let mut trial_spec = spec.clone();
    trial_spec.model_uid = experiment_uid.clone();
    trial_spec.docker_image = Some(image.clone());
    trial_spec
        .labels
        .insert(EXPERIMENT_LABEL.into(), id.to_string());
    trial_spec.created_by = Some("autotune".into());
    let mut trial = baseline.clone();
    trial.model_uid = experiment_uid.clone();
    trial.node_affinity = None;
    trial.gpu_affinity = None;
    trial.image_id = Some(image);
    trial.replica_specs = Some(request.replica_specs.clone());
    if let Some(kv) = trial.kv_sharing.as_mut() {
        let remote = request
            .isolated_remote_url
            .as_deref()
            .ok_or_else(|| bad("LMCache trials require a separate remote store"))?;
        let target = reqwest::Url::parse(remote).map_err(|_| bad("invalid trial store URL"))?;
        let original =
            reqwest::Url::parse(&kv.remote_url).map_err(|_| bad("invalid production store URL"))?;
        if target.scheme() != "lm"
            || target.host_str().is_none()
            || target.port().is_none()
            || !target.username().is_empty()
            || target.password().is_some()
            || !matches!(target.path(), "" | "/")
            || target.query().is_some()
            || target.fragment().is_some()
            || (target.host_str(), target.port()) == (original.host_str(), original.port())
            || kv.local_disk_path.is_some()
        {
            return Err(bad("trial store must use a separate lm:// host:port; disk-tier trials are not supported"));
        }
        kv.remote_url = remote.to_string();
    }
    for (replica, assignment) in trial
        .replica_specs
        .as_mut()
        .unwrap()
        .iter_mut()
        .zip(&plan.assignments)
    {
        replica.role =
            (assignment.role != nebula_common::ReplicaRole::Mixed).then_some(assignment.role);
    }
    nebula_common::validate_replica_specs(trial.replicas, trial.replica_specs.as_ref().unwrap())
        .map_err(bad)?;
    nebula_common::validate_pd_split_with_context(
        &trial,
        spec.engine_type.as_deref(),
        spec.config.as_ref(),
        &[],
    )
    .map_err(bad)?;
    for replica in trial.replica_specs.as_ref().unwrap() {
        nebula_control::compat::validate_deploy_compat(
            &*st.store,
            "vllm",
            Some(&request.engine_version),
            trial.image_id.as_deref(),
            trial_spec.docker_image.as_deref(),
            replica.node_id.as_deref(),
            None,
        )
        .await?;
    }
    if trial.kv_sharing.is_some() {
        nebula_common::validate_pd_split_image(
            trial.image_id.as_deref(),
            &nebula_common::approved_pd_images_from_env(),
        )
        .map_err(bad)?;
    }
    trial.version = 1;
    trial.updated_at_ms = now_ms();
    let pending = vec![
        intent(format!("/models/{experiment_uid}/spec"), 0, &trial_spec)?,
        intent(format!("/deployments/{experiment_uid}"), 0, &trial)?,
    ];
    let mut job = Experiment {
        id,
        owner: request.owner,
        epoch: 1,
        model_uid: uid,
        experiment_uid,
        platform,
        gpu_name,
        phase: Phase::Provisioning,
        lease_until_ms: now_ms() + LEASE_MS,
        created_at_ms: now_ms(),
        spec,
        spec_revision,
        baseline,
        stable_revision,
        trial,
        trial_revision: 0,
        trial_spec_revision: 0,
        parameters,
        candidate: None,
        request,
        measurements: vec![],
        pending,
        stable_changed: false,
        promoted_candidate: None,
        promoted_at_ms: None,
        reason: None,
        active: true,
        trial_conflicted: false,
        last_reconciled_at_ms: 0,
        trial_may_have_started: false,
        stopped_at_ms: None,
        cleanup_wait_reason: None,
    };
    let pending = std::mem::take(&mut job.pending);
    for write in pending {
        let write = if write.key.starts_with("/deployments/") {
            authorize_intent(&*st.store, &job, write).await?
        } else {
            write
        };
        job.pending.push(write);
    }
    let inserted = sqlx::query(
        "INSERT INTO bff_autotune_experiments(id,model_uid,active,state_json,updated_at_ms)
         VALUES($1,$2,true,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(&job.model_uid)
    .bind(serde_json::to_value(&job)?)
    .bind(now_ms() as i64)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    if inserted.rows_affected() != 1 {
        return Err(conflict("an active optimization already owns this model"));
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Json(job).into_response())
}

pub async fn get(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<Response, ServiceError> {
    if let Some(response) = require_role(&ctx, Role::Viewer) {
        return Ok(response);
    }
    let row = sqlx::query("SELECT state_json FROM bff_autotune_experiments WHERE id=$1")
        .bind(id)
        .fetch_optional(&st.db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ServiceError::NotFound("optimization not found".into()))?;
    Ok(Json(row.get::<serde_json::Value, _>("state_json")).into_response())
}

fn score(run: &BenchmarkRun, objective: &TuningObjective) -> Option<f64> {
    let evidence = run.evidence.as_ref()?;
    let goodput = evidence.measurements.get("goodput_rps")?.as_f64()?;
    let e2e = evidence.measurements.get("e2e_p95_ms")?.as_f64()?;
    let ttft = run.ttft_p95_ms?;
    let tpot = run.tpot_p95_ms?;
    (run.status == BenchmarkRunStatus::Succeeded
        && !evidence.synthetic
        && evidence.schema_version == 2
        && evidence.request_count >= objective.min_requests
        && evidence.wall_time_s.is_finite()
        && evidence.wall_time_s > 0.0
        && goodput <= evidence.request_count as f64 / evidence.wall_time_s * (1.0 + 1e-9)
        && evidence.successful_requests == evidence.request_count
        && run.error_rate == Some(0.0)
        && [goodput, ttft, tpot, e2e]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
        && ttft <= objective.ttft_p95_ms_max
        && tpot <= objective.tpot_p95_ms_max
        && e2e <= objective.e2e_p95_ms_max)
        .then_some(goodput)
}

#[cfg(test)]
#[path = "autotune_tests.rs"]
mod control_tests;

fn wins(job: &Experiment, candidate: usize) -> bool {
    let in_round = |m: &&Measurement| {
        m.run
            .evidence
            .as_ref()
            .and_then(|e| e.measurements.get("round"))
            .and_then(|v| v.as_u64())
            == Some(candidate as u64 + 1)
    };
    let baseline: Vec<_> = job
        .measurements
        .iter()
        .filter(|m| !m.production && m.candidate.is_none())
        .filter(in_round)
        .collect();
    let trials: Vec<_> = job
        .measurements
        .iter()
        .filter(|m| !m.production && m.candidate == Some(candidate))
        .filter(in_round)
        .collect();
    let positions = |runs: &[&Measurement]| {
        let mut positions: Vec<_> = runs
            .iter()
            .filter_map(|m| {
                m.run
                    .evidence
                    .as_ref()?
                    .measurements
                    .get("abba_position")?
                    .as_u64()
            })
            .collect();
        positions.sort_unstable();
        positions
    };
    if baseline.len() != 2
        || trials.len() != 2
        || positions(&baseline) != [0, 3]
        || positions(&trials) != [1, 2]
    {
        return false;
    }
    let base: Option<Vec<_>> = baseline
        .iter()
        .map(|m| score(&m.run, &job.request.objective))
        .collect();
    let trial: Option<Vec<_>> = trials
        .iter()
        .map(|m| score(&m.run, &job.request.objective))
        .collect();
    match (base, trial) {
        (Some(a), Some(b)) => {
            b.into_iter().fold(f64::INFINITY, f64::min)
                > a.into_iter().fold(0.0, f64::max) * (1.0 + job.request.objective.min_improvement)
        }
        _ => false,
    }
}

fn validate_run(job: &Experiment, run: &BenchmarkRun) -> Result<(), ServiceError> {
    let production = job.phase == Phase::Observing;
    let expected_uid = if production {
        &job.model_uid
    } else {
        &job.experiment_uid
    };
    let evidence = run
        .evidence
        .as_ref()
        .ok_or_else(|| bad("versioned benchmark evidence required"))?;
    if evidence.synthetic
        || evidence.schema_version != 2
        || run.profile_key.model_name != *expected_uid
        || run.profile_key.engine_type != "vllm"
        || run.profile_key.engine_version.as_deref() != Some(&job.request.engine_version)
        || run.profile_key.platform.as_deref() != Some(&job.platform)
        || run.profile_key.gpu_name.as_deref() != Some(&job.gpu_name)
        || run.evidence_notes.as_deref() != Some(note(job, production)?.as_str())
        || run.image_id.as_deref() != job.trial.image_id.as_deref()
        || evidence
            .measurements
            .get("comparison_group")
            .and_then(|x| x.as_str())
            != Some(job.id.to_string().as_str())
        || evidence
            .measurements
            .get("cache_state")
            .and_then(|x| x.as_str())
            != Some("warm")
        || evidence
            .measurements
            .get("cache_state_source")
            .and_then(|x| x.as_str())
            != Some("full_plan_warmup")
        || run.started_at_ms
            < if production {
                job.promoted_at_ms.unwrap_or(u64::MAX)
            } else {
                job.trial.updated_at_ms.max(job.created_at_ms)
            }
        || run.finished_at_ms < run.started_at_ms
        || run.finished_at_ms > now_ms() + 5_000
        || run
            .software_version
            .as_deref()
            .is_none_or(|value| value.is_empty())
        || ["trace_sha256", "samples_sha256"].iter().any(|key| {
            evidence
                .measurements
                .get(*key)
                .and_then(|value| value.as_str())
                .is_none_or(|value| value.is_empty())
        })
        || evidence.gpu_count
            != Some(
                job.trial
                    .replica_specs
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|r| r.gpu_indices.as_ref().map_or(0, |v| v.len() as u32))
                    .sum(),
            )
    {
        return Err(bad(
            "benchmark evidence is synthetic, stale, or outside this configuration scope",
        ));
    }
    let targets = serde_json::json!({
        "ttft_ms": job.request.objective.ttft_p95_ms_max,
        "tpot_ms": job.request.objective.tpot_p95_ms_max,
        "e2e_ms": job.request.objective.e2e_p95_ms_max,
    });
    if evidence.measurements.get("slo") != Some(&targets) {
        return Err(bad("benchmark SLO differs from the authorized objective"));
    }
    if !production {
        let round = evidence
            .measurements
            .get("round")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let position = evidence
            .measurements
            .get("abba_position")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);
        let previous: Vec<_> = job
            .measurements
            .iter()
            .filter(|m| {
                !m.production
                    && m.run
                        .evidence
                        .as_ref()
                        .and_then(|e| e.measurements.get("round"))
                        .and_then(|v| v.as_u64())
                        == Some(round)
            })
            .collect();
        let expected_candidate =
            matches!(position, 1 | 2).then_some(round.saturating_sub(1) as usize);
        if round == 0
            || round > job.request.candidates.len() as u64
            || position > 3
            || position != previous.len() as u64
            || job.candidate != expected_candidate
            || previous
                .last()
                .is_some_and(|m| m.run.finished_at_ms > run.started_at_ms)
        {
            return Err(bad(
                "measurements must follow a complete temporal ABBA sequence per candidate",
            ));
        }
    }
    if let Some(previous) = job.measurements.first() {
        if evidence.arrival != previous.run.evidence.as_ref().unwrap().arrival
            || evidence.request_count != previous.run.evidence.as_ref().unwrap().request_count
        {
            return Err(bad("benchmark arrival or request count changed"));
        }
        for key in ["trace_sha256", "seed", "slo"] {
            if key == "seed" {
                if evidence.seed != previous.run.evidence.as_ref().unwrap().seed {
                    return Err(bad("benchmark seed changed"));
                }
            } else if evidence.measurements.get(key)
                != previous
                    .run
                    .evidence
                    .as_ref()
                    .unwrap()
                    .measurements
                    .get(key)
            {
                return Err(bad("benchmark workload or SLO changed"));
            }
        }
        if run.workload != previous.run.workload
            || run.profile_key.platform != previous.run.profile_key.platform
            || run.profile_key.gpu_name != previous.run.profile_key.gpu_name
            || run.software_version != previous.run.software_version
        {
            return Err(bad("benchmark configuration is not comparable"));
        }
    }
    Ok(())
}

fn queue_cleanup(job: &mut Experiment) -> Result<(), ServiceError> {
    if job.trial_conflicted {
        job.phase = Phase::Conflict;
        job.active = false;
        return Ok(());
    }
    job.trial.desired_state = DesiredState::Stopped;
    next_version(&mut job.trial)?;
    job.pending.push(trial_intent(job, &job.trial)?);
    job.phase = Phase::Cleaning;
    Ok(())
}

fn queue_rollback(job: &mut Experiment) -> Result<(), ServiceError> {
    if job.stable_changed {
        let mut restored = job.baseline.clone();
        restored.version = job
            .baseline
            .version
            .checked_add(2)
            .ok_or_else(|| bad("version exhausted"))?;
        restored.updated_at_ms = now_ms();
        job.pending.push(production_intent(job, &restored)?);
        job.phase = Phase::Restoring;
    } else if job.phase != Phase::Restoring {
        queue_cleanup(job)?;
    }
    Ok(())
}

pub async fn command(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(command): Json<Command>,
) -> Result<Response, ServiceError> {
    if let Some(response) = require_role(&ctx, Role::Operator) {
        return Ok(response);
    }
    Ok(Json(execute_command(&*st.store, &st.db, id, command).await?).into_response())
}

async fn execute_command(
    store: &dyn MetaStore,
    db: &sqlx::PgPool,
    id: Uuid,
    command: Command,
) -> Result<Experiment, ServiceError> {
    let mut tx = db.begin().await.map_err(db_error)?;
    let row = sqlx::query("SELECT state_json FROM bff_autotune_experiments WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ServiceError::NotFound("optimization not found".into()))?;
    let mut job: Experiment = serde_json::from_value(row.get("state_json"))?;
    if !job.active
        || job.owner != command.owner
        || job.epoch != command.epoch
        || job.lease_until_ms <= now_ms()
    {
        return Err(conflict(
            "optimization lease is inactive, expired, or owned by another controller",
        ));
    }
    if !matches!(command.action, Action::Rollback) && budget_expired(&job) {
        return Err(conflict("experiment time budget expired"));
    }
    job.lease_until_ms = now_ms() + LEASE_MS;
    if !matches!(command.action, Action::Heartbeat) && !job.pending.is_empty() {
        return Err(conflict("configuration intent is still being applied"));
    }
    if !matches!(command.action, Action::Heartbeat | Action::Rollback) {
        unchanged(store, &job).await?;
        trial_owned(store, &job).await?;
    }
    match command.action {
        Action::Heartbeat => {}
        Action::Configure { candidate } => {
            if job.phase != Phase::Ready {
                return Err(conflict("experiment is not configurable"));
            }
            unchanged(store, &job).await?;
            let (current_plan, _) =
                read::<PlacementPlan>(store, &format!("/placements/{}", job.model_uid)).await?;
            let resources: Vec<_> = current_plan
                .assignments
                .iter()
                .map(|a| ReplicaPlacementSpec {
                    node_id: Some(a.node_id.clone()),
                    gpu_indices: a.effective_gpu_indices(),
                    ..Default::default()
                })
                .collect();
            validate_memory(
                store,
                &job.baseline,
                &resources,
                &job.request.candidates,
                false,
            )
            .await?;
            let mut authorized = job.request.candidates.clone();
            authorized.push(TuningParameters::baseline(&job.spec, &job.baseline).map_err(bad)?);
            validate_memory(
                store,
                &job.trial,
                job.trial.replica_specs.as_ref().unwrap(),
                &authorized,
                false,
            )
            .await?;
            let parameters = match candidate {
                Some(index) => job
                    .request
                    .candidates
                    .get(index)
                    .cloned()
                    .ok_or_else(|| bad("candidate index out of range"))?,
                None => TuningParameters::baseline(&job.spec, &job.baseline).map_err(bad)?,
            };
            let mut trial = parameters.apply(&job.spec, &job.trial).map_err(bad)?;
            next_version(&mut trial)?;
            let write = authorize_intent(store, &job, trial_intent(&job, &trial)?).await?;
            job.pending.push(write);
            job.trial = trial;
            job.parameters = parameters;
            job.candidate = candidate;
            job.phase = Phase::Configuring;
        }
        Action::Record { run_id } => {
            if !matches!(job.phase, Phase::Ready | Phase::Observing) {
                return Err(conflict("experiment cannot accept measurements"));
            }
            if job.measurements.len() >= 128
                || job.measurements.iter().any(|m| m.run.run_id == run_id)
            {
                return Err(bad(
                    "duplicate run or experiment measurement budget exhausted",
                ));
            }
            let production = job.phase == Phase::Observing;
            let desired = if production {
                let mut desired = job
                    .parameters
                    .apply(&job.spec, &job.baseline)
                    .map_err(bad)?;
                desired.version = job.baseline.version + 1;
                desired
            } else {
                job.trial.clone()
            };
            ready(store, &job.spec, &desired).await?;
            let row = sqlx::query("SELECT run_json FROM bff_benchmark_runs WHERE run_id=$1")
                .bind(&run_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db_error)?
                .ok_or_else(|| bad("benchmark run not found"))?;
            let run: BenchmarkRun = serde_json::from_value(row.get("run_json"))?;
            validate_run(&job, &run)?;
            let baseline_floor = job
                .measurements
                .iter()
                .filter(|m| !m.production && m.candidate.is_none())
                .filter_map(|m| score(&m.run, &job.request.objective))
                .fold(f64::INFINITY, f64::min);
            let healthy = score(&run, &job.request.objective).is_some_and(|value| {
                !production
                    || value >= baseline_floor * (1.0 - job.request.objective.max_regression)
            });
            job.measurements.push(Measurement {
                candidate: job.candidate,
                production,
                run,
            });
            if production && !healthy {
                job.reason =
                    Some("production verification failed its SLO or reliability gate".into());
                queue_rollback(&mut job)?;
            }
        }
        Action::Promote { candidate } => {
            if job.phase != Phase::Ready || !wins(&job, candidate) {
                return Err(conflict(
                    "candidate has not passed repeated real benchmark gates",
                ));
            }
            unchanged(store, &job).await?;
            let (plan, placement_revision) =
                read::<PlacementPlan>(store, &format!("/placements/{}", job.model_uid)).await?;
            let resources: Vec<_> = plan
                .assignments
                .iter()
                .map(|a| ReplicaPlacementSpec {
                    node_id: Some(a.node_id.clone()),
                    gpu_indices: a.effective_gpu_indices(),
                    ..Default::default()
                })
                .collect();
            validate_memory(
                store,
                &job.baseline,
                &resources,
                &job.request.candidates,
                false,
            )
            .await?;
            job.parameters = job
                .request
                .candidates
                .get(candidate)
                .cloned()
                .ok_or_else(|| bad("candidate index out of range"))?;
            let mut desired = job
                .parameters
                .apply(&job.spec, &job.baseline)
                .map_err(bad)?;
            next_version(&mut desired)?;
            let mut write = production_intent(&job, &desired)?;
            write.guards.extend([
                (format!("/placements/{}", job.model_uid), placement_revision),
                (
                    format!("/models/{}/spec", job.experiment_uid),
                    job.trial_spec_revision,
                ),
                (
                    format!("/deployments/{}", job.experiment_uid),
                    job.trial_revision,
                ),
            ]);
            job.pending
                .push(authorize_intent(store, &job, write).await?);
            job.phase = Phase::Promoting;
            job.promoted_candidate = Some(candidate);
            job.candidate = Some(candidate);
        }
        Action::Finish => {
            if job.phase == Phase::Observing {
                let observations: Vec<_> =
                    job.measurements.iter().filter(|m| m.production).collect();
                if observations.len() < 2
                    || observations
                        .iter()
                        .any(|m| score(&m.run, &job.request.objective).is_none())
                    || job.promoted_at_ms.is_none_or(|at| {
                        now_ms().saturating_sub(at) < job.request.observation_seconds * 1000
                    })
                {
                    return Err(conflict(
                        "two successful production verification runs are required",
                    ));
                }
            } else if job.phase != Phase::Ready {
                return Err(conflict("experiment cannot finish in its current phase"));
            }
            queue_cleanup(&mut job)?;
        }
        Action::Rollback => {
            job.reason = Some("controller requested rollback".into());
            queue_rollback(&mut job)?;
        }
    }
    save(&mut tx, &job).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(job)
}

async fn apply_intent(store: &dyn MetaStore, write: &Intent) -> Result<u64, ServiceError> {
    if let Some((current, revision)) = store.get(&write.key).await? {
        if current == write.value {
            return Ok(revision);
        }
    }
    let mut guards = write.guards.clone();
    guards.extend(write.authorization.clone());
    let (ok, revision) = store
        .compare_and_swap_guarded(
            &write.key,
            write.expected_revision,
            write.value.clone(),
            &guards,
        )
        .await?;
    if !ok {
        // An older timed-out attempt may have committed between our GET and
        // retry CAS. A failed compare is not evidence of external ownership.
        if let Some((current, revision)) = store.get(&write.key).await? {
            if current == write.value {
                return Ok(revision);
            }
        }
        return Err(conflict("configuration changed outside this experiment"));
    }
    Ok(revision)
}

fn budget_expired(job: &Experiment) -> bool {
    job.lease_until_ms <= now_ms()
        || now_ms().saturating_sub(job.created_at_ms) >= job.request.max_experiment_seconds * 1000
}

fn adopt_applied(job: &mut Experiment, write: &Intent, revision: u64) {
    if write.key == format!("/models/{}/spec", job.experiment_uid) {
        job.trial_spec_revision = revision;
        for pending in job.pending.iter_mut().skip(1) {
            pending.guards.push((write.key.clone(), revision));
        }
    } else if write.key == format!("/deployments/{}", job.model_uid) {
        job.stable_revision = revision;
        job.stable_changed = job.phase == Phase::Promoting;
        if job.stable_changed {
            job.promoted_at_ms = Some(now_ms());
        }
    } else if write.key == format!("/deployments/{}", job.experiment_uid) {
        job.trial_revision = revision;
        if let Ok(deployment) = serde_json::from_slice::<ModelDeployment>(&write.value) {
            if deployment.desired_state == DesiredState::Running {
                job.trial_may_have_started = true;
            } else {
                job.stopped_at_ms = Some(now_ms());
            }
        }
    }
}

async fn cancel_pending(
    store: &dyn MetaStore,
    job: &mut Experiment,
    write: &Intent,
) -> Result<(), ServiceError> {
    if let Some((key, _)) = &write.authorization {
        // An acknowledged delete fences even an RPC still queued at etcd.
        // If the CAS won first, the subsequent linearizable get recovers it.
        store.delete(key).await?;
        return Ok(());
    }
    // Older durable records may predate permit guards. Bump their target
    // revision atomically before forgetting the intent, never just read it.
    if !write.key.starts_with("/deployments/") {
        return Ok(());
    }
    let current = store.get(&write.key).await?;
    let revision = current.as_ref().map_or(0, |(_, revision)| *revision);
    if revision != write.expected_revision
        || current
            .as_ref()
            .is_some_and(|(value, _)| value == &write.value)
    {
        return Ok(());
    }
    let value = if let Some((value, _)) = current {
        value
    } else {
        let mut stopped: ModelDeployment = serde_json::from_slice(&write.value)?;
        stopped.desired_state = DesiredState::Stopped;
        next_version(&mut stopped)?;
        serde_json::to_vec(&stopped)?
    };
    let (ok, revision) = store.compare_and_swap(&write.key, revision, value).await?;
    if !ok {
        return Err(ServiceError::Internal(
            "intent cancellation raced a write; retry recovery".into(),
        ));
    }
    if write.key == format!("/deployments/{}", job.model_uid) {
        job.stable_revision = revision;
    } else {
        job.trial_revision = revision;
    }
    Ok(())
}

async fn expire(store: &dyn MetaStore, job: &mut Experiment) -> Result<(), ServiceError> {
    abort_pending(
        store,
        job,
        "controller lease or experiment time budget expired",
    )
    .await
}

async fn abort_pending(
    store: &dyn MetaStore,
    job: &mut Experiment,
    reason: &str,
) -> Result<(), ServiceError> {
    // Recover writes that committed before a crash, but never execute an expired
    // running-config intent. Only rollback and stop intents may follow expiry.
    for write in job.pending.clone() {
        cancel_pending(store, job, &write).await?;
        if let Some((current, revision)) = store.get(&write.key).await? {
            if current == write.value {
                adopt_applied(job, &write, revision);
            }
        }
    }
    job.pending.clear();
    job.reason = Some(reason.into());
    let production_owned = match unchanged(store, job).await {
        Ok(()) => true,
        Err(ServiceError::Conflict(_)) => {
            job.stable_changed = false;
            job.reason =
                Some("external production configuration change; automatic writes stopped".into());
            false
        }
        Err(error) => return Err(error),
    };
    match trial_owned(store, job).await {
        Ok(()) => {}
        Err(ServiceError::Conflict(_)) => {
            job.trial_conflicted = true;
            job.reason = Some(
                "external trial configuration change; trial cleanup requires its new owner".into(),
            );
        }
        Err(error) => return Err(error),
    }
    if !production_owned {
        queue_cleanup(job)?;
    } else if job.phase == Phase::Provisioning
        && job.trial_spec_revision == 0
        && job.trial_revision == 0
        && !job.trial_conflicted
    {
        job.trial.desired_state = DesiredState::Stopped;
        job.phase = Phase::Cleaning;
    } else {
        queue_rollback(job)?;
    }
    Ok(())
}

async fn trial_gpus_idle(store: &dyn MetaStore, job: &Experiment) -> Result<bool, ServiceError> {
    if !job.trial_may_have_started {
        return Ok(true);
    }
    let Some(stopped_at) = job.stopped_at_ms else {
        return Ok(false);
    };
    for replica in job.trial.replica_specs.as_ref().into_iter().flatten() {
        let Some(node) = &replica.node_id else {
            return Ok(false);
        };
        let Some((bytes, _)) = store.get(&format!("/nodes/{node}/status")).await? else {
            return Ok(false);
        };
        let status: NodeStatus = serde_json::from_slice(&bytes)?;
        if status.last_heartbeat_ms < stopped_at
            || now_ms().saturating_sub(status.last_heartbeat_ms) >= 30_000
        {
            return Ok(false);
        }
        for index in replica.gpu_indices.as_ref().into_iter().flatten() {
            if !status.gpus.iter().any(|gpu| {
                gpu.index == *index
                    && gpu.name.as_deref() == Some(job.gpu_name.as_str())
                    && gpu.memory_used_mb == 0
            }) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

async fn tick(store: &dyn MetaStore, db: &sqlx::PgPool) -> Result<(), ServiceError> {
    let ids = sqlx::query(
        "SELECT id FROM bff_autotune_experiments WHERE active ORDER BY updated_at_ms LIMIT 64",
    )
    .fetch_all(db)
    .await
    .map_err(db_error)?;
    for row in ids {
        let id: Uuid = row.get("id");
        let mut tx = db.begin().await.map_err(db_error)?;
        let Some(row) = sqlx::query("SELECT state_json FROM bff_autotune_experiments WHERE id=$1 AND active FOR UPDATE SKIP LOCKED")
            .bind(id).fetch_optional(&mut *tx).await.map_err(db_error)? else { continue };
        let mut job: Experiment = serde_json::from_value(row.get("state_json"))?;
        if budget_expired(&job) && !matches!(job.phase, Phase::Restoring | Phase::Cleaning) {
            expire(store, &mut job).await?;
        }
        if let Some(write) = job.pending.first().cloned() {
            match tokio::time::timeout(Duration::from_secs(10), apply_intent(store, &write)).await {
                Ok(Ok(revision)) => {
                    adopt_applied(&mut job, &write, revision);
                    job.pending.remove(0);
                }
                Ok(Err(ServiceError::Conflict(_))) => {
                    abort_pending(
                        store,
                        &mut job,
                        "configuration intent lost authorization or its validated snapshots",
                    )
                    .await?;
                    tracing::warn!(experiment=%id, "autotune fenced; ownership rechecked before rollback and cleanup");
                }
                Ok(Err(error)) => {
                    tracing::error!(experiment=%id, error=%error, "autotune intent failed; will retry");
                }
                Err(_) => {
                    tracing::error!(experiment=%id, "autotune intent timed out; will reconcile durable intent")
                }
            }
        }
        if job.active && job.pending.is_empty() {
            if !matches!(job.phase, Phase::Cleaning) {
                match unchanged(store, &job).await {
                    Ok(()) => {}
                    Err(ServiceError::Conflict(_)) => {
                        job.reason = Some(
                            "external production configuration change; automatic writes stopped"
                                .into(),
                        );
                        job.stable_changed = false;
                        queue_cleanup(&mut job)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            if job.active && !matches!(job.phase, Phase::Cleaning) && !job.trial_conflicted {
                match trial_owned(store, &job).await {
                    Ok(()) => {}
                    Err(ServiceError::Conflict(_)) => {
                        job.trial_conflicted = true;
                        job.reason = Some("external trial configuration change; trial cleanup requires its new owner".into());
                        queue_rollback(&mut job)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            match job.phase {
                Phase::Provisioning | Phase::Configuring => job.phase = Phase::Ready,
                Phase::Promoting => job.phase = Phase::Observing,
                Phase::Restoring => {
                    let mut restored = job.baseline.clone();
                    restored.version += 2;
                    match ready(store, &job.spec, &restored).await {
                        Ok(()) => queue_cleanup(&mut job)?,
                        Err(ServiceError::Conflict(_) | ServiceError::NotFound(_)) => {}
                        Err(error) => {
                            tracing::error!(experiment=%id, error=%error, "autotune restore readiness failed")
                        }
                    }
                }
                Phase::Cleaning => {
                    if store
                        .get(&format!("/placements/{}", job.experiment_uid))
                        .await?
                        .is_none()
                        && store
                            .list_prefix(&format!("/endpoints/{}/", job.experiment_uid))
                            .await?
                            .is_empty()
                        && trial_gpus_idle(store, &job).await?
                    {
                        job.cleanup_wait_reason = None;
                        job.phase = if job
                            .reason
                            .as_deref()
                            .is_some_and(|r| r.starts_with("external"))
                        {
                            Phase::Conflict
                        } else if job.reason.is_some() {
                            Phase::RolledBack
                        } else {
                            Phase::Completed
                        };
                        job.active = false;
                    } else {
                        job.cleanup_wait_reason = Some("waiting for placement/endpoint removal and fresh zero-used GPU memory inventory after stop".into());
                    }
                }
                _ => {}
            }
        }
        job.last_reconciled_at_ms = now_ms();
        save(&mut tx, &job).await?;
        tx.commit().await.map_err(db_error)?;
    }
    Ok(())
}

pub fn start_background_reconciler(st: AppState) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Handle::try_current()?;
    let st = Arc::new(st);
    runtime.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            match tokio::time::timeout(Duration::from_secs(30), tick(&*st.store, &st.db)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::error!(error=%error, "autotune reconciliation failed"),
                Err(_) => tracing::error!("autotune reconciliation exceeded cycle budget"),
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_meta::MemoryMetaStore;

    #[tokio::test]
    async fn durable_intent_replays_and_fences_operator_writes() {
        let store = MemoryMetaStore::new();
        let write = Intent {
            key: "/deployments/m".into(),
            expected_revision: 0,
            value: b"first".to_vec(),
            guards: vec![],
            authorization: None,
        };
        let revision = apply_intent(&store, &write).await.unwrap();
        assert_eq!(apply_intent(&store, &write).await.unwrap(), revision);
        store
            .put(&write.key, b"operator".to_vec(), None)
            .await
            .unwrap();
        assert!(matches!(
            apply_intent(&store, &write).await,
            Err(ServiceError::Conflict(_))
        ));
        assert_eq!(store.get(&write.key).await.unwrap().unwrap().0, b"operator");
    }
}
