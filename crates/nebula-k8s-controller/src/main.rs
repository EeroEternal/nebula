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
    let model_uid = &dep.model_uid;
    let pod_name = format!("nebula-{model_uid}");

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

    let mut need_create = true;
    let mut pod_ip = String::new();
    let mut is_running = false;

    if let Ok(out) = status_output {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = s.split(':').collect();
            if !parts.is_empty() && parts[0] == "Running" {
                need_create = false;
                is_running = true;
                if parts.len() > 1 {
                    pod_ip = parts[1].to_string();
                }
            } else if !parts.is_empty() && !parts[0].is_empty() {
                need_create = false;
            }
        }
    }

    if need_create {
        tracing::info!(model_uid=%model_uid, pod=%pod_name, "creating k8s pod for model");
        create_vllm_pod(namespace, gpu_node, model_uid, spec)?;
    }

    if is_running && !pod_ip.is_empty() {
        let port = 43537;
        let base_url = format!("http://{pod_ip}:{port}");

        // Probe health
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(800))
            .build()?;
        let health_url = format!("{base_url}/health");
        let is_ready = client.get(&health_url).send().await.is_ok();

        let ep_key = format!("/endpoints/{model_uid}/0");
        if is_ready {
            let ep = EndpointInfo {
                model_uid: model_uid.clone(),
                replica_id: 0,
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
                base_url: Some(base_url),
                engine_type: Some("vllm".to_string()),
            };
            let val = serde_json::to_vec(&ep)?;
            store.put(&ep_key, val, Some(15000)).await?;
            tracing::debug!(endpoint=%ep_key, "refreshed ready endpoint in etcd");
        }
    }

    Ok(())
}

async fn ensure_model_stopped(
    store: &Arc<EtcdMetaStore>,
    namespace: &str,
    dep: &ModelDeployment,
) -> anyhow::Result<()> {
    let model_uid = &dep.model_uid;
    let pod_name = format!("nebula-{model_uid}");

    let ep_key = format!("/endpoints/{model_uid}/0");
    let _ = store.delete(&ep_key).await;

    let _ = Command::new("kubectl")
        .args(["-n", namespace, "delete", "pod", &pod_name, "--wait=false"])
        .output();

    Ok(())
}

fn create_vllm_pod(
    namespace: &str,
    gpu_node: &str,
    model_uid: &str,
    spec: Option<&ModelSpec>,
) -> anyhow::Result<()> {
    let pod_name = format!("nebula-{model_uid}");
    let host_model_path = spec
        .and_then(|s| s.model_path.as_deref())
        .unwrap_or("/opt/powerllm/worker/data/models/Qwen2.5-7B-Instruct");

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
            "containers": [{
                "name": "engine",
                "image": "vllm/vllm-openai:latest",
                "imagePullPolicy": "IfNotPresent",
                "args": [
                    "--model", "/models/weights",
                    "--served-model-name", model_uid,
                    "--port", "43537",
                    "--tensor-parallel-size", "1"
                ],
                "env": [
                    { "name": "PYTHONUNBUFFERED", "value": "1" },
                    { "name": "NVIDIA_VISIBLE_DEVICES", "value": "0" },
                    { "name": "NVIDIA_DRIVER_CAPABILITIES", "value": "compute,utility" }
                ],
                "ports": [{ "containerPort": 43537, "protocol": "TCP" }],
                "volumeMounts": [
                    { "mountPath": "/models/weights", "name": "weights" },
                    { "mountPath": "/dev/shm", "name": "dshm" }
                ]
            }],
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
