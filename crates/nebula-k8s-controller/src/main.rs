//! Nebula K8s Controller
//!
//! Reconciles etcd `/deployments/{model_uid}` with in-cluster Kubernetes Pods
//! and registers Ready endpoints to `/endpoints/{model_uid}/{replica_id}`.

use clap::Parser;
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use nebula_common::{
    model_deployment::{DesiredState, ModelDeployment},
    model_spec::ModelSpec,
    EndpointInfo, EndpointKind, EndpointStatus,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(long, env = "ETCD_ENDPOINT", default_value = "http://127.0.0.1:2379")]
    pub etcd_endpoint: String,

    #[arg(long, env = "K8S_NAMESPACE", default_value = "powerllm")]
    pub namespace: String,

    #[arg(long, env = "K8S_GPU_NODE", default_value = "xgateway")]
    pub gpu_node: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    tracing::info!(
        etcd=%args.etcd_endpoint,
        namespace=%args.namespace,
        "nebula-k8s-controller starting"
    );

    let store = Arc::new(EtcdMetaStore::connect(std::slice::from_ref(&args.etcd_endpoint)).await?);
    let namespace = args.namespace.clone();
    let gpu_node = args.gpu_node.clone();
    let metrics = Arc::new(ControllerMetrics::default());

    // 1. Reconcile loop
    loop {
        if let Err(e) = reconcile_once(&store, &namespace, &gpu_node, &metrics).await {
            tracing::warn!(
                error = %e,
                model_success = metrics.model_reconcile_success_total.load(std::sync::atomic::Ordering::Relaxed),
                model_errors = metrics.model_reconcile_errors_total.load(std::sync::atomic::Ordering::Relaxed),
                replica_success = metrics.replica_reconcile_success_total.load(std::sync::atomic::Ordering::Relaxed),
                replica_errors = metrics.replica_reconcile_errors_total.load(std::sync::atomic::Ordering::Relaxed),
                "reconcile loop warning"
            );
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Maximum consecutive reconcile ticks a model may renew existing endpoint TTLs
/// while `/models/{model_uid}/spec` fails to load. Protects running Pods across
/// transient etcd read blips while letting permanently broken specs expire.
const MAX_SPEC_ERROR_REFRESH_TICKS: u32 = 3;

#[derive(Debug, Default)]
pub struct ControllerMetrics {
    pub deployment_parse_errors_total: std::sync::atomic::AtomicU64,
    pub spec_load_errors_total: std::sync::atomic::AtomicU64,
    pub model_reconcile_success_total: std::sync::atomic::AtomicU64,
    pub model_reconcile_errors_total: std::sync::atomic::AtomicU64,
    pub replica_reconcile_success_total: std::sync::atomic::AtomicU64,
    pub replica_reconcile_errors_total: std::sync::atomic::AtomicU64,
    pub consecutive_spec_errors: std::sync::Mutex<std::collections::HashMap<String, u32>>,
}

async fn load_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
) -> anyhow::Result<Option<ModelSpec>> {
    let spec_key = format!("/models/{model_uid}/spec");
    match store.get(&spec_key).await? {
        Some((raw, _rev)) => {
            let spec = serde_json::from_slice::<ModelSpec>(&raw)?;
            Ok(Some(spec))
        }
        None => Ok(None),
    }
}

/// Renew existing `Ready` `/endpoints/{model_uid}/{replica_id}` (and `/stats/`)
/// TTLs for replicas on `gpu_node` without reconciling or recreating Pods when
/// `/models/{model_uid}/spec` transiently fails to load, preventing
/// already-healthy Pods from dropping out of Router during brief blips.
async fn refresh_existing_model_endpoints(
    store: &dyn MetaStore,
    gpu_node: &str,
    dep: &ModelDeployment,
) {
    if dep.desired_state != DesiredState::Running {
        return;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let replicas = dep.replicas.max(1);
    for replica_id in 0..replicas {
        if replica_node(dep, replica_id, gpu_node) != gpu_node {
            continue;
        }
        let ep_key = format!("/endpoints/{}/{replica_id}", dep.model_uid);
        if let Ok(Some((raw, _))) = store.get(&ep_key).await {
            if let Ok(mut ep) = serde_json::from_slice::<EndpointInfo>(&raw) {
                if ep.status != EndpointStatus::Ready {
                    continue;
                }
                ep.last_heartbeat_ms = now_ms;
                if let Ok(refreshed) = serde_json::to_vec(&ep) {
                    let _ = store.put(&ep_key, refreshed, Some(15000)).await;
                }
            }
        }
        let stats_key = format!("/stats/{}/{replica_id}", dep.model_uid);
        if let Ok(Some((raw, _))) = store.get(&stats_key).await {
            let _ = store.put(&stats_key, raw, Some(15000)).await;
        }
    }
}

async fn reconcile_deployments_with<F, Fut>(
    store: &dyn MetaStore,
    gpu_node: &str,
    metrics: &ControllerMetrics,
    mut step: F,
) -> anyhow::Result<()>
where
    F: FnMut(ModelDeployment, Option<ModelSpec>) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    use std::sync::atomic::Ordering;
    let deps = store.list_prefix("/deployments/").await?;

    for (_k, v, _rev) in deps {
        let dep = match serde_json::from_slice::<ModelDeployment>(&v) {
            Ok(dep) => dep,
            Err(e) => {
                // Never skip silently: an unparseable deployment means this
                // agent manages nothing for that model (see the pd_split
                // snake_case wire-format incident, 2026-10-12).
                metrics
                    .deployment_parse_errors_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(key = %_k, error = %e, "deployment spec unparseable; skipping");
                continue;
            }
        };

        let model_uid = dep.model_uid.clone();
        let spec = match load_model_spec(store, &model_uid).await {
            Ok(spec) => {
                if let Ok(mut map) = metrics.consecutive_spec_errors.lock() {
                    map.remove(&model_uid);
                }
                spec
            }
            Err(e) => {
                // Never fall back to spec=None on store/parse errors: that would
                // change config_fingerprint and delete healthy running pods.
                // Renew existing Ready endpoint TTLs for up to MAX_SPEC_ERROR_REFRESH_TICKS
                // consecutive ticks so brief blips don't drop healthy pods, while
                // permanently corrupted specs stop renewing and let stale endpoints expire.
                metrics
                    .spec_load_errors_total
                    .fetch_add(1, Ordering::Relaxed);
                let streak = {
                    let mut map = metrics
                        .consecutive_spec_errors
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let entry = map.entry(model_uid.clone()).or_insert(0);
                    *entry = entry.saturating_add(1);
                    *entry
                };
                if streak <= MAX_SPEC_ERROR_REFRESH_TICKS {
                    tracing::warn!(
                        model_uid = %model_uid,
                        streak,
                        max_streak = MAX_SPEC_ERROR_REFRESH_TICKS,
                        error = %e,
                        "failed to load model spec; refreshing existing Ready endpoints within grace window"
                    );
                    refresh_existing_model_endpoints(store, gpu_node, &dep).await;
                } else {
                    tracing::error!(
                        model_uid = %model_uid,
                        streak,
                        max_streak = MAX_SPEC_ERROR_REFRESH_TICKS,
                        error = %e,
                        "model spec load failed beyond grace window; skipping endpoint TTL refresh so stale endpoints expire"
                    );
                }
                continue;
            }
        };

        match step(dep, spec).await {
            Ok(()) => {
                metrics
                    .model_reconcile_success_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                metrics
                    .model_reconcile_errors_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    model_uid = %model_uid,
                    error = %e,
                    "failed to reconcile model deployment; continuing with remaining models"
                );
            }
        }
    }

    Ok(())
}

async fn reconcile_once(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    gpu_node: &str,
    metrics: &Arc<ControllerMetrics>,
) -> anyhow::Result<()> {
    let metrics_for_step = Arc::clone(metrics);
    reconcile_deployments_with(store.as_ref(), gpu_node, metrics.as_ref(), |dep, spec| {
        let store = Arc::clone(store);
        let namespace = namespace.to_string();
        let gpu_node = gpu_node.to_string();
        let metrics = Arc::clone(&metrics_for_step);
        async move {
            match dep.desired_state {
                DesiredState::Running => {
                    ensure_model_running(
                        &store,
                        &namespace,
                        &gpu_node,
                        &dep,
                        spec.as_ref(),
                        &metrics,
                    )
                    .await?;
                }
                DesiredState::Stopped => {
                    ensure_model_stopped(&store, &namespace, &dep).await?;
                }
            }
            Ok(())
        }
    })
    .await
}

async fn ensure_model_running(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    gpu_node: &str,
    dep: &ModelDeployment,
    spec: Option<&ModelSpec>,
    metrics: &ControllerMetrics,
) -> anyhow::Result<()> {
    let replicas = dep.replicas.max(1);
    for replica_id in 0..replicas {
        // Multi-node: this agent only manages replicas assigned to its node.
        if replica_node(dep, replica_id, gpu_node) != gpu_node {
            continue;
        }
        match ensure_replica_running(store, namespace, gpu_node, dep, spec, replica_id).await {
            Ok(()) => {
                metrics
                    .replica_reconcile_success_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                metrics
                    .replica_reconcile_errors_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(
                    model_uid = %dep.model_uid,
                    replica_id,
                    error = %e,
                    "failed to reconcile replica; continuing with remaining replicas"
                );
            }
        }
    }
    if let Err(e) = cleanup_extra_replicas(store, namespace, &dep.model_uid, replicas).await {
        tracing::warn!(
            model_uid = %dep.model_uid,
            error = %e,
            "failed to cleanup extra replicas"
        );
    }
    Ok(())
}

/// Node a replica should run on: `replica_specs[i].node_id` when given, else the
/// agent's own node.
/// Stable fingerprint of the launch-affecting config for one replica.
fn config_fingerprint(
    engine_type: &str,
    role: nebula_common::ReplicaRole,
    kv_sharing: Option<&nebula_common::KvSharingSpec>,
    docker_image: Option<&str>,
    model_path: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(engine_type.as_bytes());
    h.update(format!("{role:?}").as_bytes());
    h.update(docker_image.unwrap_or("").as_bytes());
    h.update(model_path.unwrap_or("").as_bytes());
    if let Some(kv) = kv_sharing {
        h.update(kv.remote_url.as_bytes());
        h.update(kv.serde.as_bytes());
    }
    format!("{:x}", h.finalize())[..16].to_string()
}

fn replica_node(dep: &ModelDeployment, replica_id: u32, default_node: &str) -> String {
    dep.replica_specs
        .as_ref()
        .and_then(|s| s.get(replica_id as usize))
        .and_then(|rs| rs.node_id.clone())
        .unwrap_or_else(|| default_node.to_string())
}

/// Delete pods / endpoints / stats for replicas >= `replicas` (scale-down).
async fn cleanup_extra_replicas(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    model_uid: &str,
    replicas: u32,
) -> anyhow::Result<()> {
    let out = Command::new("kubectl")
        .args([
            "-n",
            namespace,
            "get",
            "pods",
            "-l",
            &format!("nebula.model_uid={model_uid}"),
            "-o",
            "jsonpath={range .items[*]}{.metadata.name}{\"\\n\"}{end}",
        ])
        .output();
    let Ok(out) = out else { return Ok(()) };
    if !out.status.success() {
        return Ok(());
    }

    let base = format!("nebula-{model_uid}");
    let names = String::from_utf8_lossy(&out.stdout).to_string();
    for name in names.lines() {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let replica_id = if name == base {
            0
        } else if let Some(suffix) = name.strip_prefix(&format!("{base}-")) {
            match suffix.parse::<u32>() {
                Ok(n) => n,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        if replica_id >= replicas {
            tracing::info!(pod = %name, replica_id, "deleting extra replica (scale-down)");
            let _ = Command::new("kubectl")
                .args(["-n", namespace, "delete", "pod", name, "--wait=false"])
                .output();
            let _ = store
                .delete(&format!("/endpoints/{model_uid}/{replica_id}"))
                .await;
            let _ = store
                .delete(&format!("/stats/{model_uid}/{replica_id}"))
                .await;
        }
    }
    Ok(())
}

/// Pod name: replica 0 keeps the legacy `nebula-{model}` name; others get a
/// `-{replica_id}` suffix.
fn replica_pod_name(model_uid: &str, replica_id: u32) -> String {
    if replica_id == 0 {
        format!("nebula-{model_uid}")
    } else {
        format!("nebula-{model_uid}-{replica_id}")
    }
}

/// GPU index for a replica: from `replica_specs[i].gpu_indices[0]` when given,
/// else the replica index itself.
fn replica_gpu_index(dep: &ModelDeployment, replica_id: u32) -> u32 {
    dep.replica_specs
        .as_ref()
        .and_then(|s| s.get(replica_id as usize))
        .and_then(|rs| rs.gpu_indices.as_ref())
        .and_then(|g| g.first().copied())
        .unwrap_or(replica_id)
}

/// Resolve the concrete container image for a replica:
/// `replica_specs[i].image_id` -> `dep.image_id` -> `spec.docker_image`,
/// then dereference through `/images/` if it matches a registered `EngineImage.id`.
/// Propagates store errors so a transient etcd failure never falls back to a
/// raw `image_id` (which would change `config_fingerprint` and delete running pods).
async fn resolve_replica_image(
    store: &dyn MetaStore,
    dep: &ModelDeployment,
    spec: Option<&ModelSpec>,
    replica_id: u32,
) -> anyhow::Result<Option<String>> {
    let Some(raw) = nebula_common::effective_replica_image(
        dep,
        spec.and_then(|s| s.docker_image.as_deref()),
        replica_id as usize,
    )
    .map(str::trim)
    .filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let kvs = store.list_prefix("/images/").await?;
    for (_, val, _) in kvs {
        if let Ok(img) = serde_json::from_slice::<nebula_common::EngineImage>(&val) {
            if img.id == raw || img.image == raw {
                return Ok(Some(img.image));
            }
        }
    }
    Ok(Some(raw.to_string()))
}

async fn ensure_replica_running(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    gpu_node: &str,
    dep: &ModelDeployment,
    spec: Option<&ModelSpec>,
    replica_id: u32,
) -> anyhow::Result<()> {
    let model_uid = &dep.model_uid;
    let pod_name = replica_pod_name(model_uid, replica_id);
    let engine_type = spec
        .and_then(|s| s.engine_type.as_deref())
        .unwrap_or("vllm");

    // Check if pod exists
    let status_output = Command::new("kubectl")
        .args([
            "-n",
            namespace,
            "get",
            "pod",
            &pod_name,
            "-o",
            "jsonpath={.status.phase}:{.status.podIP}:{.metadata.annotations.nebula\\.config\\.hash}",
        ])
        .output();

    let mut phase = String::new();
    let mut pod_ip = String::new();
    let mut pod_config_hash = String::new();

    if let Ok(out) = status_output {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let mut parts = s.split(':');
            phase = parts.next().unwrap_or_default().to_string();
            pod_ip = parts.next().unwrap_or_default().to_string();
            pod_config_hash = parts.next().unwrap_or_default().to_string();
        }
    }

    // Config fingerprint: role / kv_sharing / image / engine changes must
    // recreate the pod, otherwise a running engine keeps stale launch config
    // (pool.md §pd_split lifecycle). Rig finding 2026-10-13: switching
    // topology back to replicated left the pd_split env in place.
    let role = dep
        .replica_specs
        .as_ref()
        .and_then(|specs| specs.get(replica_id as usize))
        .and_then(|s| s.role)
        .unwrap_or(nebula_common::ReplicaRole::Mixed);
    let resolved_image = resolve_replica_image(store.as_ref(), dep, spec, replica_id).await?;
    let desired_hash = config_fingerprint(
        engine_type,
        role,
        dep.kv_sharing.as_ref(),
        resolved_image.as_deref(),
        spec.and_then(|s| s.model_path.as_deref()),
    );
    // An unannotated pod predates this mechanism: recreate once so it adopts
    // the fingerprint (otherwise future config changes go undetected for it).
    let config_changed = pod_config_hash != desired_hash;
    if config_changed && !pod_config_hash.is_empty() {
        tracing::info!(pod = %pod_name, "launch config changed; recreating pod");
    }

    let is_running = phase == "Running" && !config_changed;
    // Recreate when the pod is gone, in a terminal phase, or when its launch
    // config changed; keep waiting while Pending (it is still starting).
    let need_create = !matches!(phase.as_str(), "Running" | "Pending") || config_changed;

    // Enforce NEBULA_APPROVED_PD_IMAGES on the execution plane before deleting
    // an existing pod for config adoption or creating a new pod, so an
    // out-of-band image mutation cannot roll out an unapproved pd_split image.
    if (config_changed || need_create)
        && (dep.topology == Some(nebula_common::DeploymentTopology::PdSplit)
            || role != nebula_common::ReplicaRole::Mixed)
    {
        let approved = nebula_common::approved_pd_images_from_env();
        nebula_common::validate_pd_split_image(resolved_image.as_deref(), &approved)
            .map_err(|e| anyhow::anyhow!(e))?;
    }

    // Pod specs are immutable: a config change needs delete-then-create, not
    // apply. Delete now; the next reconcile tick creates the fresh pod.
    if config_changed && !phase.is_empty() {
        tracing::info!(pod = %pod_name, "deleting pod for config adoption; will recreate next tick");
        let _ = Command::new("kubectl")
            .args(["-n", namespace, "delete", "pod", &pod_name, "--wait=false"])
            .output();
        return Ok(());
    }

    if need_create {
        let gpu_index = replica_gpu_index(dep, replica_id);
        tracing::info!(model_uid=%model_uid, pod=%pod_name, replica_id, gpu_index, "creating k8s pod for replica");
        create_engine_pod(EnginePodSpec {
            namespace,
            gpu_node,
            model_uid,
            replica_id,
            gpu_index,
            engine_type,
            spec,
            resolved_image: resolved_image.as_deref(),
            role,
            kv_sharing: dep.kv_sharing.as_ref(),
            config_hash: desired_hash.clone(),
        })?;
    }

    if is_running && !pod_ip.is_empty() {
        let port = 43537;
        let base_url = format!("http://{pod_ip}:{port}");

        // Probe health. Timeout is generous: SGLang's /health can take >800ms.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;
        let health_url = format!("{base_url}/health");
        let is_ready = client.get(&health_url).send().await.is_ok();

        let ep_key = format!("/endpoints/{model_uid}/{replica_id}");
        if is_ready {
            let ep = EndpointInfo {
                model_uid: model_uid.clone(),
                replica_id,
                plan_version: dep.version,
                node_id: gpu_node.to_string(),
                endpoint_kind: EndpointKind::NativeHttp,
                api_flavor: "openai".to_string(),
                status: EndpointStatus::Ready,
                last_heartbeat_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                status_detail: None,
                grpc_target: None,
                base_url: Some(base_url.clone()),
                engine_type: Some(engine_type.to_string()),
                role,
            };
            let val = serde_json::to_vec(&ep)?;
            store.put(&ep_key, val, Some(15000)).await?;
            tracing::debug!(endpoint=%ep_key, "refreshed ready endpoint in etcd");

            // Scrape vLLM metrics -> /stats/ so the router has per-replica
            // signals (pending / kv_cache / prefix cache hit rate).
            let metrics_url = format!("{base_url}/metrics");
            if let Ok(resp) = client.get(&metrics_url).send().await {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        let stats = nebula_common::engine_metrics::parse_engine_metrics(
                            Some(engine_type),
                            &text,
                            model_uid,
                            replica_id,
                            now,
                        );
                        let diag = nebula_common::parse_engine_scrape_diagnostics(
                            Some(engine_type),
                            &text,
                        );
                        if let Some(tau) = diag.spec_decode_mean_acceptance_length {
                            tracing::debug!(
                                model_uid = %model_uid,
                                replica_id,
                                spec_decode_mean_acceptance_length = tau,
                                "scraped speculative decoding acceptance length"
                            );
                        }
                        if let Ok(v) = serde_json::to_vec(&stats) {
                            let stats_key = format!("/stats/{model_uid}/{replica_id}");
                            store.put(&stats_key, v, Some(15000)).await?;
                        }
                    }
                }
            }
        }
    } else {
        // Not running: drop any stale endpoint/stats for this replica.
        let _ = store
            .delete(&format!("/endpoints/{model_uid}/{replica_id}"))
            .await;
        let _ = store
            .delete(&format!("/stats/{model_uid}/{replica_id}"))
            .await;
    }

    Ok(())
}

async fn ensure_model_stopped(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    dep: &ModelDeployment,
) -> anyhow::Result<()> {
    let model_uid = &dep.model_uid;
    let replicas = dep.replicas.max(1);
    for replica_id in 0..replicas {
        let ep_key = format!("/endpoints/{model_uid}/{replica_id}");
        let _ = store.delete(&ep_key).await;
        let _ = store
            .delete(&format!("/stats/{model_uid}/{replica_id}"))
            .await;

        let pod_name = replica_pod_name(model_uid, replica_id);
        let _ = Command::new("kubectl")
            .args(["-n", namespace, "delete", "pod", &pod_name, "--wait=false"])
            .output();
    }
    Ok(())
}

/// Engine-pod creation inputs (grouped — keeps the call site readable and
/// carries the pd_split fields: role + kv_sharing).
struct EnginePodSpec<'a> {
    namespace: &'a str,
    gpu_node: &'a str,
    model_uid: &'a str,
    replica_id: u32,
    gpu_index: u32,
    engine_type: &'a str,
    spec: Option<&'a ModelSpec>,
    resolved_image: Option<&'a str>,
    role: nebula_common::ReplicaRole,
    kv_sharing: Option<&'a nebula_common::KvSharingSpec>,
    /// Launch-config fingerprint stamped as a pod annotation so a config
    /// change forces recreation (pool.md §pd_split lifecycle).
    config_hash: String,
}

fn create_engine_pod(p: EnginePodSpec<'_>) -> anyhow::Result<()> {
    let EnginePodSpec {
        namespace,
        gpu_node,
        model_uid,
        replica_id,
        gpu_index,
        engine_type,
        spec,
        resolved_image,
        role,
        kv_sharing,
        config_hash,
    } = p;
    let pod_name = replica_pod_name(model_uid, replica_id);
    let host_model_path = spec
        .and_then(|s| s.model_path.as_deref())
        .unwrap_or("/opt/powerllm/worker/data/models/Qwen2.5-7B-Instruct");

    let is_sglang = engine_type.eq_ignore_ascii_case("sglang");
    // Honour the resolved deployment/replica/spec pinned image
    // (compat-matrix/baked-digest discipline, review #4).
    let default_image = if is_sglang {
        "lmsysorg/sglang:latest"
    } else {
        "vllm/vllm-openai:v0.23.0"
    };
    let image = resolved_image
        .or_else(|| spec.and_then(|s| s.docker_image.as_deref()))
        .unwrap_or(default_image);
    let mut args = if is_sglang {
        json!([
            "-m",
            "sglang.launch_server",
            "--model-path",
            "/models/weights",
            "--served-model-name",
            model_uid,
            "--host",
            "0.0.0.0",
            "--port",
            "43537",
            "--trust-remote-code",
            "--enable-metrics"
        ])
    } else {
        json!([
            "--model",
            "/models/weights",
            "--served-model-name",
            model_uid,
            "--port",
            "43537",
            "--tensor-parallel-size",
            "1"
        ])
    };
    // P1c: opt-in KV cache events → the router's prefix→replica index (vLLM
    // only; SGLang has no equivalent publisher — capability gap, the
    // prefix_affinity strategy falls back visibly for SGLang models). The
    // hash seed must be pinned (v0.23+ randomizes NONE_HASH per process when
    // PYTHONHASHSEED is unset) and the algo pinned to sha256_cbor (default
    // "sha256" is pickle-based, not reproducible outside Python).
    let mut kv_hash_seed: Option<String> = None;
    if !is_sglang {
        if let Ok(endpoint) = std::env::var("NEBULA_KV_EVENTS_ENDPOINT") {
            let seed = std::env::var("NEBULA_KV_HASH_SEED").unwrap_or_else(|_| "42".to_string());
            if let Some(arr) = args.as_array_mut() {
                arr.push(json!("--kv-events-config"));
                arr.push(json!(format!(
                    "{{\"enable_kv_cache_events\":true,\"publisher\":\"zmq\",\"endpoint\":\"{endpoint}\",\"topic\":\"{model_uid}/{replica_id}\"}}"
                )));
                arr.push(json!("--prefix-caching-hash-algo"));
                arr.push(json!("sha256_cbor"));
            }
            kv_hash_seed = Some(seed);
        }
    }
    // pd_split needs the same pinned seed even without kv-events, otherwise
    // prefill/decode chunk keys diverge and the shared KV never matches
    // (pool.md §pd_split v1.1 seed contract).
    if kv_sharing.is_some() && kv_hash_seed.is_none() {
        kv_hash_seed =
            Some(std::env::var("NEBULA_KV_HASH_SEED").unwrap_or_else(|_| "42".to_string()));
    }

    // pd_split (pool.md §pd_split v1.1): role-based LMCache injection via the
    // shared helpers — identical to the node plane (one source of truth).
    let mut kv_envs: Vec<(&'static str, String)> = Vec::new();
    if let Some(kv) = kv_sharing {
        if let Some(cfg) = nebula_common::kv_transfer_config(role) {
            if let Some(arr) = args.as_array_mut() {
                arr.push(json!("--kv-transfer-config"));
                arr.push(json!(cfg));
                arr.push(json!("--prefix-caching-hash-algo"));
                arr.push(json!("sha256_cbor"));
            }
        }
        kv_envs = nebula_common::kv_sharing_env(role, kv);
        // Controller registration: makes the shared store's contents queryable
        // (POST /lookup) — the pd_split warm signal source.
        if let Ok(ctrl) = std::env::var("NEBULA_LMCACHE_CONTROLLER_URL") {
            let ctrl_reply = std::env::var("NEBULA_LMCACHE_CONTROLLER_REPLY_URL")
                .unwrap_or_else(|_| ctrl.clone());
            let worker_port: u16 = std::env::var("NEBULA_LMCACHE_WORKER_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8001);
            let instance_id = format!("{model_uid}-{replica_id}");
            kv_envs.extend(nebula_common::lmcache_controller_env(
                &ctrl,
                &ctrl_reply,
                worker_port,
                &instance_id,
            ));
        }
    }

    let mut container = json!({
        "name": "engine",
        "image": image,
        "imagePullPolicy": "IfNotPresent",
        "args": args,
        "env": [
            { "name": "PYTHONUNBUFFERED", "value": "1" },
            { "name": "NVIDIA_VISIBLE_DEVICES", "value": gpu_index.to_string() },
            { "name": "NVIDIA_DRIVER_CAPABILITIES", "value": "compute,utility" }
        ],
        "ports": [{ "containerPort": 43537, "protocol": "TCP" }],
        "volumeMounts": [
            { "mountPath": "/models/weights", "name": "weights" },
            { "mountPath": "/dev/shm", "name": "dshm" }
        ]
    });
    if let Some(seed) = kv_hash_seed.as_deref() {
        if let Some(envs) = container["env"].as_array_mut() {
            envs.push(json!({ "name": "PYTHONHASHSEED", "value": seed }));
        }
    }
    if let Some(envs) = container["env"].as_array_mut() {
        for (k, v) in &kv_envs {
            envs.push(json!({ "name": k, "value": v }));
        }
    }

    if is_sglang {
        // SGLang image entrypoint is not the server; invoke the module explicitly.
        container["command"] = json!(["python3"]);
    }

    let manifest = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": pod_name,
            "namespace": namespace,
            "annotations": { "nebula.config.hash": config_hash },
            "labels": {
                "nebula.component": "engine",
                "nebula.model_uid": model_uid
            }
        },
        "spec": {
            "nodeName": gpu_node,
            "restartPolicy": "Always",
            "containers": [container],
            "volumes": [
                {
                    "name": "weights",
                    "hostPath": {
                        "path": host_model_path,
                        "type": "DirectoryOrCreate"
                    }
                },
                {
                    "name": "dshm",
                    "emptyDir": {
                        "medium": "Memory",
                        "sizeLimit": "16Gi"
                    }
                }
            ]
        }
    });

    let manifest_str = serde_json::to_string(&manifest)?;
    let mut child = Command::new("kubectl")
        .args(["apply", "-f", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(manifest_str.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        tracing::error!(
            stderr=%String::from_utf8_lossy(&out.stderr),
            "failed to apply kubectl manifest"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_common::{KvSharingSpec, ReplicaRole};

    /// Any launch-affecting change must move the fingerprint, otherwise a
    /// running pod keeps stale config (rig finding: topology switch left the
    /// pd_split env in place).
    #[test]
    fn config_fingerprint_tracks_launch_config() {
        let kv = KvSharingSpec {
            remote_url: "lm://a:1".into(),
            serde: "naive".into(),
            ..Default::default()
        };
        let base = config_fingerprint(
            "vllm",
            ReplicaRole::Prefill,
            Some(&kv),
            Some("img:1"),
            Some("/m"),
        );
        assert_eq!(
            base,
            config_fingerprint(
                "vllm",
                ReplicaRole::Prefill,
                Some(&kv),
                Some("img:1"),
                Some("/m")
            )
        );
        // role change
        assert_ne!(
            base,
            config_fingerprint(
                "vllm",
                ReplicaRole::Mixed,
                Some(&kv),
                Some("img:1"),
                Some("/m")
            )
        );
        // kv_sharing removed
        assert_ne!(
            base,
            config_fingerprint(
                "vllm",
                ReplicaRole::Prefill,
                None,
                Some("img:1"),
                Some("/m")
            )
        );
        // store address change
        let kv2 = KvSharingSpec {
            remote_url: "lm://b:2".into(),
            serde: "naive".into(),
            ..Default::default()
        };
        assert_ne!(
            base,
            config_fingerprint(
                "vllm",
                ReplicaRole::Prefill,
                Some(&kv2),
                Some("img:1"),
                Some("/m")
            )
        );
        // image change
        assert_ne!(
            base,
            config_fingerprint(
                "vllm",
                ReplicaRole::Prefill,
                Some(&kv),
                Some("img:2"),
                Some("/m")
            )
        );
    }

    #[tokio::test]
    async fn reconcile_deployments_isolates_per_model_errors_and_skips_corrupted_spec() {
        use nebula_common::DeploymentTopology;
        use nebula_meta::MemoryMetaStore;

        let store = MemoryMetaStore::new();
        let dep_a = ModelDeployment {
            model_uid: "a-broken-pd".into(),
            desired_state: DesiredState::Running,
            replicas: 2,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            config_overrides: None,
            version: 1,
            updated_at_ms: 1,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: Vec::new(),
            replica_specs: None,
            allowed_pools: None,
            topology: Some(DeploymentTopology::PdSplit),
            kv_sharing: None,
        };
        let dep_b = ModelDeployment {
            model_uid: "b-corrupt-spec".into(),
            desired_state: DesiredState::Running,
            replicas: 1,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            config_overrides: None,
            version: 1,
            updated_at_ms: 1,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: Vec::new(),
            replica_specs: None,
            allowed_pools: None,
            topology: None,
            kv_sharing: None,
        };
        let dep_c = ModelDeployment {
            model_uid: "c-healthy".into(),
            desired_state: DesiredState::Running,
            replicas: 1,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            config_overrides: None,
            version: 1,
            updated_at_ms: 1,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: Vec::new(),
            replica_specs: None,
            allowed_pools: None,
            topology: None,
            kv_sharing: None,
        };

        store
            .put(
                "/deployments/a-broken-pd",
                serde_json::to_vec(&dep_a).unwrap(),
                None,
            )
            .await
            .unwrap();
        store
            .put(
                "/deployments/b-corrupt-spec",
                serde_json::to_vec(&dep_b).unwrap(),
                None,
            )
            .await
            .unwrap();
        store
            .put(
                "/deployments/c-healthy",
                serde_json::to_vec(&dep_c).unwrap(),
                None,
            )
            .await
            .unwrap();

        // Corrupt /models/b-corrupt-spec/spec: must NOT fall back to spec=None,
        // and must renew existing /endpoints/b-corrupt-spec/0 heartbeat.
        store
            .put(
                "/models/b-corrupt-spec/spec",
                b"{not-valid-json".to_vec(),
                None,
            )
            .await
            .unwrap();
        let existing_ep = EndpointInfo {
            model_uid: "b-corrupt-spec".into(),
            replica_id: 0,
            plan_version: 1,
            node_id: "node-1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status: EndpointStatus::Ready,
            last_heartbeat_ms: 42,
            status_detail: None,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:43537".into()),
            engine_type: Some("vllm".into()),
            role: nebula_common::ReplicaRole::Mixed,
        };
        store
            .put(
                "/endpoints/b-corrupt-spec/0",
                serde_json::to_vec(&existing_ep).unwrap(),
                Some(15000),
            )
            .await
            .unwrap();

        let metrics = ControllerMetrics::default();
        let visited = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let visited_clone = std::sync::Arc::clone(&visited);
        reconcile_deployments_with(&store, "node-1", &metrics, move |dep, _spec| {
            let visited = std::sync::Arc::clone(&visited_clone);
            async move {
                visited.lock().unwrap().push(dep.model_uid.clone());
                if dep.model_uid == "a-broken-pd" {
                    anyhow::bail!("simulated NEBULA_APPROVED_PD_IMAGES rejection");
                }
                Ok(())
            }
        })
        .await
        .unwrap();

        let visited = visited.lock().unwrap().clone();
        // Model A failed with an error, Model B had a corrupted spec and was skipped
        // before running `step` (preventing a bogus spec=None fingerprint) while its
        // existing endpoint was refreshed, and Model C still ran to completion.
        assert_eq!(
            visited,
            vec!["a-broken-pd".to_string(), "c-healthy".to_string()]
        );
        assert_eq!(
            metrics
                .spec_load_errors_total
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .model_reconcile_errors_total
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .model_reconcile_success_total
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let (ep_raw, _) = store
            .get("/endpoints/b-corrupt-spec/0")
            .await
            .unwrap()
            .expect("existing endpoint kept alive");
        let refreshed_ep: EndpointInfo = serde_json::from_slice(&ep_raw).unwrap();
        assert!(
            refreshed_ep.last_heartbeat_ms > 42,
            "existing endpoint heartbeat must be refreshed when spec load fails within grace window"
        );

        // After MAX_SPEC_ERROR_REFRESH_TICKS (3) total consecutive failures, controller
        // must stop renewing the endpoint so permanently broken specs cannot keep dead
        // Pods in routing forever.
        for _ in 2..=MAX_SPEC_ERROR_REFRESH_TICKS {
            reconcile_deployments_with(
                &store,
                "node-1",
                &metrics,
                |_dep, _spec| async move { Ok(()) },
            )
            .await
            .unwrap();
        }
        let reset_marker = 123_456u64;
        let mut stale_ep = refreshed_ep.clone();
        stale_ep.last_heartbeat_ms = reset_marker;
        store
            .put(
                "/endpoints/b-corrupt-spec/0",
                serde_json::to_vec(&stale_ep).unwrap(),
                Some(15000),
            )
            .await
            .unwrap();

        // Tick 4 (> MAX_SPEC_ERROR_REFRESH_TICKS): must NOT refresh last_heartbeat_ms.
        reconcile_deployments_with(
            &store,
            "node-1",
            &metrics,
            |_dep, _spec| async move { Ok(()) },
        )
        .await
        .unwrap();
        let (ep_raw_after_cap, _) = store
            .get("/endpoints/b-corrupt-spec/0")
            .await
            .unwrap()
            .unwrap();
        let ep_after_cap: EndpointInfo = serde_json::from_slice(&ep_raw_after_cap).unwrap();
        assert_eq!(
            ep_after_cap.last_heartbeat_ms, reset_marker,
            "endpoint heartbeat must stop being renewed once MAX_SPEC_ERROR_REFRESH_TICKS is exceeded"
        );
    }
}
