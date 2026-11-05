use super::*;
use nebula_meta::MemoryMetaStore;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

/// Commit a previously queued identical CAS exactly between retry GET and CAS.
struct CommitDuringRetry {
    inner: MemoryMetaStore,
    inject: AtomicBool,
}

#[async_trait::async_trait]
impl MetaStore for CommitDuringRetry {
    async fn put(&self, key: &str, value: Vec<u8>, ttl: Option<u64>) -> anyhow::Result<u64> {
        self.inner.put(key, value, ttl).await
    }
    async fn get(&self, key: &str) -> anyhow::Result<Option<(Vec<u8>, u64)>> {
        self.inner.get(key).await
    }
    async fn delete(&self, key: &str) -> anyhow::Result<u64> {
        self.inner.delete(key).await
    }
    async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<(String, Vec<u8>, u64)>> {
        self.inner.list_prefix(prefix).await
    }
    async fn compare_and_swap(
        &self,
        key: &str,
        revision: u64,
        value: Vec<u8>,
    ) -> anyhow::Result<(bool, u64)> {
        self.inner.compare_and_swap(key, revision, value).await
    }
    async fn compare_and_swap_guarded(
        &self,
        key: &str,
        revision: u64,
        value: Vec<u8>,
        guards: &[(String, u64)],
    ) -> anyhow::Result<(bool, u64)> {
        if self.inject.swap(false, Ordering::SeqCst) {
            assert!(
                self.inner
                    .compare_and_swap_guarded(key, revision, value.clone(), guards)
                    .await?
                    .0
            );
        }
        self.inner
            .compare_and_swap_guarded(key, revision, value, guards)
            .await
    }
    async fn watch_prefix(
        &self,
        prefix: &str,
        revision: Option<u64>,
    ) -> anyhow::Result<nebula_meta::types::WatchStream> {
        self.inner.watch_prefix(prefix, revision).await
    }
}

#[tokio::test]
async fn retry_recovers_a_cas_that_commits_between_read_and_compare() {
    let store = CommitDuringRetry {
        inner: MemoryMetaStore::new(),
        inject: AtomicBool::new(true),
    };
    let mut job = fixture();
    seed_metadata(&store, &mut job).await;
    job.phase = Phase::Promoting;
    let mut target = job.request.candidates[0]
        .apply(&job.spec, &job.baseline)
        .unwrap();
    next_version(&mut target).unwrap();
    let write = authorize_intent(&store, &job, production_intent(&job, &target).unwrap())
        .await
        .unwrap();
    let revision = apply_intent(&store, &write).await.unwrap();
    adopt_applied(&mut job, &write, revision);
    assert!(job.stable_changed);
    queue_rollback(&mut job).unwrap();
    let restore = &job.pending[0];
    apply_intent(&store, restore).await.unwrap();
    let (restored, _) = read::<ModelDeployment>(&store, &restore.key).await.unwrap();
    assert_eq!(restored.version, job.baseline.version + 2);
    assert!(restored.config_overrides.is_none());
}

fn fixture() -> Experiment {
    let spec: ModelSpec = serde_json::from_value(json!({
        "model_uid": "production", "model_name": "weights", "model_source": "local",
        "engine_type": "vllm", "model_path": "/models/weights",
        "docker_image": "image@sha256:1234",
        "config": {"max_num_seqs": 16, "max_num_batched_tokens": 2048,
                   "enable_chunked_prefill": true}
    }))
    .unwrap();
    let baseline: ModelDeployment = serde_json::from_value(json!({
        "model_uid": "production", "desired_state": "running", "replicas": 1,
        "version": 1, "image_id": "image@sha256:1234",
        "replica_specs":[{"node_id":"node","gpu_indices":[0]}]
    }))
    .unwrap();
    let parameters = TuningParameters::baseline(&spec, &baseline).unwrap();
    let mut candidate = parameters.clone();
    candidate.max_num_seqs = 32;
    let mut trial = baseline.clone();
    trial.model_uid = "experiment".into();
    trial.replica_specs = Some(vec![ReplicaPlacementSpec {
        node_id: Some("trial-node".into()),
        gpu_indices: Some(vec![0]),
        ..Default::default()
    }]);
    let request = CreateExperiment {
        owner: Uuid::new_v4(),
        candidates: vec![candidate],
        objective: TuningObjective {
            ttft_p95_ms_max: 200.0,
            tpot_p95_ms_max: 50.0,
            e2e_p95_ms_max: 1000.0,
            min_improvement: 0.1,
            max_regression: 0.05,
            min_requests: 16,
        },
        replica_specs: trial.replica_specs.clone().unwrap(),
        isolated_remote_url: None,
        engine_version: "0.23.0".into(),
        observation_seconds: 30,
        max_experiment_seconds: 7200,
    };
    Experiment {
        id: Uuid::new_v4(),
        owner: request.owner,
        epoch: 1,
        model_uid: spec.model_uid.clone(),
        experiment_uid: trial.model_uid.clone(),
        platform: "nvidia-cuda".into(),
        gpu_name: "Test GPU".into(),
        phase: Phase::Ready,
        lease_until_ms: now_ms() + LEASE_MS,
        created_at_ms: now_ms() - 1,
        spec,
        spec_revision: 0,
        baseline,
        stable_revision: 0,
        trial,
        trial_revision: 0,
        trial_spec_revision: 0,
        parameters,
        candidate: None,
        request,
        measurements: vec![],
        pending: vec![],
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
    }
}

fn measurement(job: &Experiment, goodput: f64, position: u32) -> BenchmarkRun {
    let production = job.phase == Phase::Observing;
    let objective = &job.request.objective;
    serde_json::from_value(json!({
        "run_id": format!("run-{}", Uuid::new_v4()),
        "profile_key": {
            "model_name": if production { &job.model_uid } else { &job.experiment_uid },
            "engine_type": "vllm", "engine_version": job.request.engine_version,
            "platform": job.platform, "gpu_name": job.gpu_name, "workload_id": "scenario"
        },
        "workload": {"id":"scenario", "class":"custom", "prompt":"fixture",
                     "max_tokens":32, "concurrency":8, "request_count":16},
        "status":"succeeded", "image_id": job.trial.image_id,
        "software_version":"test", "ttft_p95_ms":100.0, "tpot_p95_ms":5.0,
        "error_rate":0.0, "started_at_ms":now_ms(), "finished_at_ms":now_ms(),
        "evidence_notes":note(job, production).unwrap(),
        "evidence": {
            "schema_version":2, "synthetic":false, "seed":42, "arrival":"closed",
            "wall_time_s":16.0 / goodput, "request_count":16, "successful_requests":16,
            "gpu_count":1, "goodput_rps":goodput, "e2e_p95_ms":200.0,
            "comparison_group":job.id.to_string(), "cache_state":"warm",
            "cache_state_source":"full_plan_warmup", "trace_sha256":"trace",
            "samples_sha256":"samples", "round":1, "abba_position":position,
            "slo":{"ttft_ms":objective.ttft_p95_ms_max,
                   "tpot_ms":objective.tpot_p95_ms_max,
                   "e2e_ms":objective.e2e_p95_ms_max}
        }
    }))
    .unwrap()
}

fn add_abba(job: &mut Experiment) {
    for (position, candidate) in [None, Some(0), Some(0), None].into_iter().enumerate() {
        job.candidate = candidate;
        job.parameters = candidate
            .map(|i| job.request.candidates[i].clone())
            .unwrap_or_else(|| TuningParameters::baseline(&job.spec, &job.baseline).unwrap());
        let run = measurement(
            job,
            if candidate.is_some() { 12.0 } else { 10.0 },
            position as u32,
        );
        job.measurements.push(Measurement {
            candidate,
            production: false,
            run,
        });
    }
}

#[test]
fn command_wire_format_is_strict_and_usable() {
    let owner = Uuid::new_v4();
    let command: Command = serde_json::from_value(json!({
        "owner":owner, "epoch":1, "operation":{"action":"configure", "candidate":0}
    }))
    .unwrap();
    assert!(matches!(
        command.action,
        Action::Configure { candidate: Some(0) }
    ));
    assert!(serde_json::from_value::<Command>(json!({
        "owner":owner, "epoch":1, "operation":{"action":"promote", "candidate":0, "temperature":0}
    }))
    .is_err());
}

#[test]
fn repeated_real_evidence_is_required_and_regressions_do_not_win() {
    let mut job = fixture();
    add_abba(&mut job);
    assert!(wins(&job, 0));
    job.measurements[1].run.evidence.as_mut().unwrap().synthetic = true;
    assert!(!wins(&job, 0));
    job.measurements[1].run.evidence.as_mut().unwrap().synthetic = false;
    job.measurements[1].run.status = BenchmarkRunStatus::Failed;
    assert!(!wins(&job, 0));
    job.measurements[1].run.status = BenchmarkRunStatus::Succeeded;
    job.measurements[1]
        .run
        .evidence
        .as_mut()
        .unwrap()
        .measurements
        .insert("goodput_rps".into(), json!(10.5));
    assert!(!wins(&job, 0));
    job.measurements.remove(1);
    assert!(!wins(&job, 0));
}

#[test]
fn wrong_scope_and_declared_only_warmth_are_rejected() {
    let job = fixture();
    let mut run = measurement(&job, 10.0, 0);
    assert!(validate_run(&job, &run).is_ok());
    run.image_id = Some("another-image".into());
    assert!(validate_run(&job, &run).is_err());
    run.image_id = job.trial.image_id.clone();
    run.evidence
        .as_mut()
        .unwrap()
        .measurements
        .insert("cache_state_source".into(), json!("operator_declared"));
    assert!(validate_run(&job, &run).is_err());
}

#[tokio::test]
async fn related_spec_revision_is_an_atomic_write_guard() {
    let store = MemoryMetaStore::new();
    let revision = store
        .put("/models/m/spec", b"old".to_vec(), None)
        .await
        .unwrap();
    let write = Intent {
        key: "/deployments/m".into(),
        expected_revision: 0,
        value: b"candidate".to_vec(),
        guards: vec![("/models/m/spec".into(), revision)],
        authorization: None,
    };
    store
        .put("/models/m/spec", b"operator".to_vec(), None)
        .await
        .unwrap();
    assert!(matches!(
        apply_intent(&store, &write).await,
        Err(ServiceError::Conflict(_))
    ));
    assert!(store.get("/deployments/m").await.unwrap().is_none());
}

async fn load(db: &sqlx::PgPool, id: Uuid) -> Experiment {
    let row = sqlx::query("SELECT state_json FROM bff_autotune_experiments WHERE id=$1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap();
    serde_json::from_value(row.get("state_json")).unwrap()
}

async fn seed(store: &dyn MetaStore, db: &sqlx::PgPool, job: &mut Experiment) {
    seed_metadata(store, job).await;
    sqlx::query("INSERT INTO bff_autotune_experiments VALUES($1,$2,true,$3,$4)")
        .bind(job.id)
        .bind(&job.model_uid)
        .bind(serde_json::to_value(&*job).unwrap())
        .bind(now_ms() as i64)
        .execute(db)
        .await
        .unwrap();
}

async fn seed_metadata(store: &dyn MetaStore, job: &mut Experiment) {
    job.trial_may_have_started = true;
    job.spec_revision = store
        .put(
            &format!("/models/{}/spec", job.model_uid),
            serde_json::to_vec(&job.spec).unwrap(),
            None,
        )
        .await
        .unwrap();
    let mut trial_spec = job.spec.clone();
    trial_spec.model_uid = job.experiment_uid.clone();
    job.trial_spec_revision = store
        .put(
            &format!("/models/{}/spec", job.experiment_uid),
            serde_json::to_vec(&trial_spec).unwrap(),
            None,
        )
        .await
        .unwrap();
    job.stable_revision = store
        .put(
            &format!("/deployments/{}", job.model_uid),
            serde_json::to_vec(&job.baseline).unwrap(),
            None,
        )
        .await
        .unwrap();
    job.trial_revision = store
        .put(
            &format!("/deployments/{}", job.experiment_uid),
            serde_json::to_vec(&job.trial).unwrap(),
            None,
        )
        .await
        .unwrap();
    project(store, &job.spec, &job.baseline).await;
    project(store, &job.spec, &job.trial).await;
}

#[tokio::test]
async fn expired_promotion_is_cancelled_or_recovered_without_a_new_promotion() {
    for already_applied in [false, true] {
        let store = MemoryMetaStore::new();
        let mut job = fixture();
        seed_metadata(&store, &mut job).await;
        job.phase = Phase::Promoting;
        job.parameters = job.request.candidates[0].clone();
        let mut promoted = job.parameters.apply(&job.spec, &job.baseline).unwrap();
        next_version(&mut promoted).unwrap();
        job.pending
            .push(production_intent(&job, &promoted).unwrap());
        let write = job.pending.pop().unwrap();
        job.pending
            .push(authorize_intent(&store, &job, write).await.unwrap());
        if already_applied {
            apply_intent(&store, &job.pending[0]).await.unwrap();
        }
        job.lease_until_ms = 0;
        assert!(budget_expired(&job));
        expire(&store, &mut job).await.unwrap();
        assert_eq!(
            job.phase,
            if already_applied {
                Phase::Restoring
            } else {
                Phase::Cleaning
            }
        );
        let next: ModelDeployment = serde_json::from_slice(&job.pending[0].value).unwrap();
        if already_applied {
            assert_eq!(next.model_uid, job.model_uid);
            assert_eq!(next.version, job.baseline.version + 2);
            assert!(next.config_overrides.is_none());
        } else {
            assert_eq!(next.model_uid, job.experiment_uid);
            assert_eq!(next.desired_state, DesiredState::Stopped);
            let (unchanged, _) =
                read::<ModelDeployment>(&store, &format!("/deployments/{}", job.model_uid))
                    .await
                    .unwrap();
            assert_eq!(unchanged.version, job.baseline.version);
        }
    }
}

#[tokio::test]
async fn expired_unprovisioned_experiment_never_starts_a_trial() {
    let store = MemoryMetaStore::new();
    let mut job = fixture();
    seed_metadata(&store, &mut job).await;
    store
        .delete(&format!("/models/{}/spec", job.experiment_uid))
        .await
        .unwrap();
    store
        .delete(&format!("/deployments/{}", job.experiment_uid))
        .await
        .unwrap();
    store
        .delete(&format!("/placements/{}", job.experiment_uid))
        .await
        .unwrap();
    store
        .delete(&format!("/endpoints/{}/0", job.experiment_uid))
        .await
        .unwrap();
    job.trial_revision = 0;
    job.trial_spec_revision = 0;
    job.trial_may_have_started = false;
    job.phase = Phase::Provisioning;
    let write = intent(
        format!("/deployments/{}", job.experiment_uid),
        0,
        &job.trial,
    )
    .unwrap();
    job.pending
        .push(authorize_intent(&store, &job, write).await.unwrap());
    expire(&store, &mut job).await.unwrap();
    assert_eq!(job.phase, Phase::Cleaning);
    assert!(job.pending.is_empty());
    assert!(store
        .get(&format!("/deployments/{}", job.experiment_uid))
        .await
        .unwrap()
        .is_none());
}

async fn project(store: &dyn MetaStore, spec: &ModelSpec, dep: &ModelDeployment) {
    let node_id = dep
        .replica_specs
        .as_ref()
        .and_then(|r| r[0].node_id.as_deref())
        .unwrap_or("node");
    store.put(&format!("/nodes/{node_id}/status"), serde_json::to_vec(&json!({
        "node_id":node_id,"last_heartbeat_ms":now_ms(),"platform":"nvidia-cuda",
        "gpus":[{"index":0,"name":"Test GPU","memory_total_mb":81920,
                 "memory_used_mb":if dep.desired_state == DesiredState::Stopped {0} else {8192}}]
    })).unwrap(), None).await.unwrap();
    if dep.desired_state == DesiredState::Stopped {
        store
            .delete(&format!("/placements/{}", dep.model_uid))
            .await
            .unwrap();
        store
            .delete(&format!("/endpoints/{}/0", dep.model_uid))
            .await
            .unwrap();
        return;
    }
    let node_id = dep
        .replica_specs
        .as_ref()
        .and_then(|r| r[0].node_id.as_deref())
        .unwrap_or("node");
    let args = nebula_common::build_engine_extra_args(
        spec.engine_type.as_deref(),
        &nebula_common::effective_replica_config(dep, spec.config.as_ref(), 0).unwrap(),
    )
    .unwrap();
    let plan = json!({
        "model_uid":dep.model_uid, "model_name":spec.model_name, "version":dep.version,
        "assignments":[{"replica_id":0, "node_id":node_id, "engine_config_path":"/fixture",
                        "docker_image":dep.image_id,
                        "port":8000,"gpu_indices":[0],"extra_args":args,
                        "kv_sharing":dep.kv_sharing}]
    });
    store
        .put(
            &format!("/placements/{}", dep.model_uid),
            serde_json::to_vec(&plan).unwrap(),
            None,
        )
        .await
        .unwrap();
    store.put(&format!("/endpoints/{}/0", dep.model_uid), serde_json::to_vec(&json!({
        "model_uid":dep.model_uid,"replica_id":0,"plan_version":dep.version,"node_id":node_id,
        "status":"ready","last_heartbeat_ms":now_ms(),"base_url":"http://127.0.0.1:8000"
    })).unwrap(), None).await.unwrap();
}

#[test]
fn automatic_placement_is_not_an_authorized_production_target() {
    let mut dep = fixture().baseline;
    require_fixed_placement(&dep).unwrap();
    dep.replica_specs.as_mut().unwrap()[0].gpu_indices = None;
    assert!(require_fixed_placement(&dep).is_err());
    dep.replica_specs = None;
    assert!(require_fixed_placement(&dep).is_err());
}

#[tokio::test]
async fn a_captured_cas_cannot_commit_after_expiry_cancellation() {
    let store = MemoryMetaStore::new();
    let mut job = fixture();
    seed_metadata(&store, &mut job).await;
    job.phase = Phase::Promoting;
    let mut target = job.request.candidates[0]
        .apply(&job.spec, &job.baseline)
        .unwrap();
    next_version(&mut target).unwrap();
    let queued = authorize_intent(&store, &job, production_intent(&job, &target).unwrap())
        .await
        .unwrap();
    let mut guards = queued.guards.clone();
    guards.extend(queued.authorization.clone());
    job.pending.push(queued.clone());
    job.lease_until_ms = 0;
    expire(&store, &mut job).await.unwrap();
    // Deliver the old, already-captured transaction only AFTER cancellation
    // acknowledged. Its target revision is still unchanged, so only the
    // deleted server-side permit prevents the stale promotion.
    assert_eq!(
        store.get(&queued.key).await.unwrap().unwrap().1,
        queued.expected_revision
    );
    let (applied, _) = store
        .compare_and_swap_guarded(&queued.key, queued.expected_revision, queued.value, &guards)
        .await
        .unwrap();
    assert!(!applied);
    let (current, _) = read::<ModelDeployment>(&store, &queued.key).await.unwrap();
    assert_eq!(current.version, job.baseline.version);
}

#[tokio::test]
async fn endpoint_disappearance_does_not_confirm_a_stopped_gpu() {
    let store = MemoryMetaStore::new();
    let mut job = fixture();
    job.trial_may_have_started = true;
    let stopped_at = now_ms();
    job.stopped_at_ms = Some(stopped_at);
    let key = "/nodes/trial-node/status";
    assert!(
        !trial_gpus_idle(&store, &job).await.unwrap(),
        "lost node cannot acknowledge stopping"
    );
    for (heartbeat, used, expected) in [
        (stopped_at - 1, 0, false),
        (stopped_at, 128, false),
        (stopped_at, 0, true),
    ] {
        store.put(key, serde_json::to_vec(&json!({
            "node_id":"trial-node","last_heartbeat_ms":heartbeat,
            "gpus":[{"index":0,"name":"Test GPU","memory_total_mb":81920,"memory_used_mb":used}]
        })).unwrap(), None).await.unwrap();
        assert_eq!(trial_gpus_idle(&store, &job).await.unwrap(), expected);
    }
}

fn command_for(job: &Experiment, action: Action) -> Command {
    Command {
        owner: job.owner,
        epoch: job.epoch,
        action,
    }
}

#[tokio::test]
#[ignore = "requires NEBULA_AUTOTUNE_TEST_DATABASE_URL pointing to isolated PostgreSQL"]
async fn postgres_recovery_and_multi_controller_drill() {
    let url = std::env::var("NEBULA_AUTOTUNE_TEST_DATABASE_URL").unwrap();
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&url)
        .await
        .unwrap();
    initialize(&db).await.unwrap();
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bff_benchmark_runs
        (run_id TEXT PRIMARY KEY,run_json JSONB NOT NULL,finished_at_ms BIGINT NOT NULL)",
    )
    .execute(&db)
    .await
    .unwrap();
    recovery_drill(MemoryMetaStore::new(), db.clone()).await;
    if let Ok(endpoint) = std::env::var("NEBULA_AUTOTUNE_TEST_ETCD_ENDPOINT") {
        let store = nebula_meta::EtcdMetaStore::connect(&[endpoint])
            .await
            .unwrap();
        let key = format!("/deployments/permit-test-{}", Uuid::new_v4());
        let permit = format!("/autotune/permits/{}", Uuid::new_v4());
        let revision = store.put(&key, b"baseline".to_vec(), None).await.unwrap();
        let authorization = store
            .put(&permit, b"allowed".to_vec(), Some(1000))
            .await
            .unwrap();
        // Wait for the real server-side lease, not a producer-declared clock.
        tokio::time::timeout(Duration::from_secs(6), async {
            while store.get(&permit).await.unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        let late = Intent {
            key: key.clone(),
            expected_revision: revision,
            value: b"candidate".to_vec(),
            guards: vec![],
            authorization: Some((permit, authorization)),
        };
        assert!(matches!(
            apply_intent(&store, &late).await,
            Err(ServiceError::Conflict(_))
        ));
        assert_eq!(store.get(&key).await.unwrap().unwrap().0, b"baseline");
        store.delete(&key).await.unwrap();
        recovery_drill(store, db.clone()).await;
    }
    db.close().await;
}

async fn recovery_drill(store: impl MetaStore, db: sqlx::PgPool) {
    let mut job = fixture();
    // Unique names make this drill safe to repeat in the isolated test database.
    job.model_uid = format!("prod-{}", job.id.simple());
    job.baseline.model_uid = job.model_uid.clone();
    job.spec.model_uid = job.model_uid.clone();
    job.experiment_uid = format!("trial-{}", job.id.simple());
    job.trial.model_uid = job.experiment_uid.clone();
    seed(&store, &db, &mut job).await;

    let (a, b) = tokio::join!(
        execute_command(
            &store,
            &db,
            job.id,
            command_for(&job, Action::Configure { candidate: None })
        ),
        execute_command(
            &store,
            &db,
            job.id,
            command_for(&job, Action::Configure { candidate: Some(0) })
        ),
    );
    assert_ne!(
        a.is_ok(),
        b.is_ok(),
        "only one controller may enqueue an intent"
    );
    let pending = load(&db, job.id).await;
    // The etcd write succeeds, then the BFF dies before saving the new revision.
    let revision = apply_intent(&store, &pending.pending[0]).await.unwrap();
    tick(&store, &db).await.unwrap();
    job = load(&db, job.id).await;
    assert_eq!(job.trial_revision, revision);
    assert_eq!(job.phase, Phase::Ready);

    for (position, candidate) in [None, Some(0), Some(0), None].into_iter().enumerate() {
        job = execute_command(
            &store,
            &db,
            job.id,
            command_for(&job, Action::Configure { candidate }),
        )
        .await
        .unwrap();
        tick(&store, &db).await.unwrap();
        job = load(&db, job.id).await;
        project(&store, &job.spec, &job.trial).await;
        let run = measurement(
            &job,
            if candidate.is_some() { 12.0 } else { 10.0 },
            position as u32,
        );
        sqlx::query("INSERT INTO bff_benchmark_runs VALUES($1,$2,$3)")
            .bind(&run.run_id)
            .bind(serde_json::to_value(&run).unwrap())
            .bind(now_ms() as i64)
            .execute(&db)
            .await
            .unwrap();
        job = execute_command(
            &store,
            &db,
            job.id,
            command_for(&job, Action::Record { run_id: run.run_id }),
        )
        .await
        .unwrap();
    }
    assert!(wins(&job, 0));
    let stale_owner = Command {
        owner: Uuid::new_v4(),
        epoch: 1,
        action: Action::Promote { candidate: 0 },
    };
    assert!(execute_command(&store, &db, job.id, stale_owner)
        .await
        .is_err());
    job = execute_command(
        &store,
        &db,
        job.id,
        command_for(&job, Action::Promote { candidate: 0 }),
    )
    .await
    .unwrap();
    apply_intent(&store, &job.pending[0]).await.unwrap();
    let (a, b) = tokio::join!(tick(&store, &db), tick(&store, &db));
    a.unwrap();
    b.unwrap();
    job = load(&db, job.id).await;
    assert_eq!(job.phase, Phase::Observing);
    let (promoted, _) = read::<ModelDeployment>(&store, &format!("/deployments/{}", job.model_uid))
        .await
        .unwrap();
    assert_eq!(
        promoted.config_overrides.as_ref().unwrap().max_num_seqs,
        Some(32)
    );
    project(&store, &job.spec, &promoted).await;
    let mut failed = measurement(&job, 8.0, 0);
    failed.status = BenchmarkRunStatus::Failed;
    failed.error_rate = Some(0.1);
    sqlx::query("INSERT INTO bff_benchmark_runs VALUES($1,$2,$3)")
        .bind(&failed.run_id)
        .bind(serde_json::to_value(&failed).unwrap())
        .bind(now_ms() as i64)
        .execute(&db)
        .await
        .unwrap();
    job = execute_command(
        &store,
        &db,
        job.id,
        command_for(
            &job,
            Action::Record {
                run_id: failed.run_id,
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(job.phase, Phase::Restoring);
    tick(&store, &db).await.unwrap();
    let (restored, _) = read::<ModelDeployment>(&store, &format!("/deployments/{}", job.model_uid))
        .await
        .unwrap();
    project(&store, &job.spec, &restored).await;
    tick(&store, &db).await.unwrap();
    tick(&store, &db).await.unwrap();
    job = load(&db, job.id).await;
    assert_eq!(job.phase, Phase::Cleaning);
    assert!(
        job.active,
        "reservations remain held until the execution plane stops the trial"
    );
    store
        .delete(&format!("/placements/{}", job.experiment_uid))
        .await
        .unwrap();
    store
        .delete(&format!("/endpoints/{}/0", job.experiment_uid))
        .await
        .unwrap();
    tick(&store, &db).await.unwrap();
    let waiting = load(&db, job.id).await;
    assert!(
        waiting.active && waiting.cleanup_wait_reason.is_some(),
        "disappeared endpoints cannot hide a failed engine stop"
    );
    store.delete("/nodes/trial-node/status").await.unwrap();
    tick(&store, &db).await.unwrap();
    assert!(
        load(&db, job.id).await.active,
        "a lost node must retain its reservation"
    );
    project(&store, &job.spec, &job.trial).await;
    tick(&store, &db).await.unwrap();
    job = load(&db, job.id).await;
    assert_eq!(job.phase, Phase::RolledBack);
    assert!(!job.active);
    let (restored, _) = read::<ModelDeployment>(&store, &format!("/deployments/{}", job.model_uid))
        .await
        .unwrap();
    assert!(restored.config_overrides.is_none());
    assert_eq!(restored.version, job.baseline.version + 2);

    // A different operator's change must survive both promotion recovery and cleanup.
    let mut external = fixture();
    external.model_uid = format!("prod-{}", external.id.simple());
    external.spec.model_uid = external.model_uid.clone();
    external.baseline.model_uid = external.model_uid.clone();
    external.experiment_uid = format!("trial-{}", external.id.simple());
    external.trial.model_uid = external.experiment_uid.clone();
    seed(&store, &db, &mut external).await;
    external.phase = Phase::Promoting;
    let mut desired = external.request.candidates[0]
        .apply(&external.spec, &external.baseline)
        .unwrap();
    next_version(&mut desired).unwrap();
    external
        .pending
        .push(production_intent(&external, &desired).unwrap());
    let mut tx = db.begin().await.unwrap();
    save(&mut tx, &external).await.unwrap();
    tx.commit().await.unwrap();
    let mut operator = external.baseline.clone();
    operator.version = 99;
    store
        .put(
            &format!("/deployments/{}", external.model_uid),
            serde_json::to_vec(&operator).unwrap(),
            None,
        )
        .await
        .unwrap();
    tick(&store, &db).await.unwrap();
    tick(&store, &db).await.unwrap();
    external = load(&db, external.id).await;
    project(&store, &external.spec, &external.trial).await;
    tick(&store, &db).await.unwrap();
    assert_eq!(load(&db, external.id).await.phase, Phase::Conflict);
    let (preserved, _) =
        read::<ModelDeployment>(&store, &format!("/deployments/{}", external.model_uid))
            .await
            .unwrap();
    assert_eq!(preserved.version, 99);

    // A naturally expired configure permit is not an ownership transfer.
    // The old trial still runs and must be stopped with its reservation held.
    let mut expired = fixture();
    expired.model_uid = format!("prod-{}", expired.id.simple());
    expired.spec.model_uid = expired.model_uid.clone();
    expired.baseline.model_uid = expired.model_uid.clone();
    expired.experiment_uid = format!("trial-{}", expired.id.simple());
    expired.trial.model_uid = expired.experiment_uid.clone();
    seed(&store, &db, &mut expired).await;
    expired = execute_command(
        &store,
        &db,
        expired.id,
        command_for(&expired, Action::Configure { candidate: Some(0) }),
    )
    .await
    .unwrap();
    let permit = &expired.pending[0].authorization.as_ref().unwrap().0;
    store.delete(permit).await.unwrap();
    assert!(expired.lease_until_ms > now_ms(), "PG lease remains valid");
    tick(&store, &db).await.unwrap();
    expired = load(&db, expired.id).await;
    assert_eq!(expired.phase, Phase::Cleaning);
    assert!(expired.active && !expired.trial_conflicted);
    tick(&store, &db).await.unwrap();
    expired = load(&db, expired.id).await;
    assert!(
        expired.active,
        "a still-running engine must retain its reservation"
    );
    project(&store, &expired.spec, &expired.trial).await;
    tick(&store, &db).await.unwrap();
    assert_eq!(load(&db, expired.id).await.phase, Phase::RolledBack);
}
