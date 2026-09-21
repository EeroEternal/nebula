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

    // 1. Reconcile loop
    loop {
        if let Err(e) = reconcile_once(&store, &namespace, &gpu_node).await {
            tracing::warn!(error=%e, "reconcile loop warning");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn reconcile_once(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    gpu_node: &str,
) -> anyhow::Result<()> {
    let deps = store.list_prefix("/deployments/").await?;

    for (_k, v, _rev) in deps {
        let Ok(dep) = serde_json::from_slice::<ModelDeployment>(&v) else {
            continue;
        };

        let model_uid = dep.model_uid.clone();
        let spec_key = format!("/models/{model_uid}/spec");
        let spec: Option<ModelSpec> = match store.get(&spec_key).await {
            Ok(Some((raw, _rev))) => serde_json::from_slice(&raw).ok(),
            _ => None,
        };

        match dep.desired_state {
            DesiredState::Running => {
                ensure_model_running(store, namespace, gpu_node, &dep, spec.as_ref()).await?;
            }
            DesiredState::Stopped => {
                ensure_model_stopped(store, namespace, &dep).await?;
            }
        }
    }

    Ok(())
}

async fn ensure_model_running(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    gpu_node: &str,
    dep: &ModelDeployment,
    spec: Option<&ModelSpec>,
) -> anyhow::Result<()> {
    let replicas = dep.replicas.max(1);
    for replica_id in 0..replicas {
        ensure_replica_running(store, namespace, gpu_node, dep, spec, replica_id).await?;
    }
    cleanup_extra_replicas(store, namespace, &dep.model_uid, replicas).await?;
    Ok(())
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
            "jsonpath={.status.phase}:{.status.podIP}",
        ])
        .output();

    let mut phase = String::new();
    let mut pod_ip = String::new();

    if let Ok(out) = status_output {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let mut parts = s.split(':');
            phase = parts.next().unwrap_or_default().to_string();
            pod_ip = parts.next().unwrap_or_default().to_string();
        }
    }

    let is_running = phase == "Running";
    // Recreate when the pod is gone or in a terminal phase; keep waiting while
    // Pending (it is still starting).
    let need_create = !matches!(phase.as_str(), "Running" | "Pending");

    if need_create {
        let gpu_index = replica_gpu_index(dep, replica_id);
        tracing::info!(model_uid=%model_uid, pod=%pod_name, replica_id, gpu_index, "creating k8s pod for replica");
        create_engine_pod(
            namespace,
            gpu_node,
            model_uid,
            replica_id,
            gpu_index,
            engine_type,
            spec,
        )?;
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

fn create_engine_pod(
    namespace: &str,
    gpu_node: &str,
    model_uid: &str,
    replica_id: u32,
    gpu_index: u32,
    engine_type: &str,
    spec: Option<&ModelSpec>,
) -> anyhow::Result<()> {
    let pod_name = replica_pod_name(model_uid, replica_id);
    let host_model_path = spec
        .and_then(|s| s.model_path.as_deref())
        .unwrap_or("/opt/powerllm/worker/data/models/Qwen2.5-7B-Instruct");

    let is_sglang = engine_type.eq_ignore_ascii_case("sglang");
    let image = if is_sglang {
        "lmsysorg/sglang:latest"
    } else {
        "vllm/vllm-openai:latest"
    };
    let args = if is_sglang {
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
