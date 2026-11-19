//! Async `kubectl` wrappers for `nebula-k8s-controller`.
//!
//! Uses `tokio::process::Command` so Kubernetes API calls never block Tokio
//! worker threads during reconcile loops.

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// List pod names in `namespace` matching `nebula.model_uid={model_uid}`.
pub async fn list_model_pods(namespace: &str, model_uid: &str) -> Vec<String> {
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
        .output()
        .await;
    let Ok(out) = out else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Fetch `(phase, pod_ip, config_hash)` for `pod_name`, or empty strings if not found.
pub async fn get_pod_status(namespace: &str, pod_name: &str) -> (String, String, String) {
    let out = Command::new("kubectl")
        .args([
            "-n",
            namespace,
            "get",
            "pod",
            pod_name,
            "-o",
            "jsonpath={.status.phase}:{.status.podIP}:{.metadata.annotations.nebula\\.config\\.hash}",
        ])
        .output()
        .await;

    if let Ok(out) = out {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let mut parts = s.split(':');
            let phase = parts.next().unwrap_or_default().to_string();
            let pod_ip = parts.next().unwrap_or_default().to_string();
            let pod_config_hash = parts.next().unwrap_or_default().to_string();
            return (phase, pod_ip, pod_config_hash);
        }
    }
    (String::new(), String::new(), String::new())
}

/// Delete a pod asynchronously without waiting for termination.
pub async fn delete_pod_nowait(namespace: &str, pod_name: &str) {
    if let Err(e) = Command::new("kubectl")
        .args(["-n", namespace, "delete", "pod", pod_name, "--wait=false"])
        .output()
        .await
    {
        tracing::warn!(pod = %pod_name, error = %e, "failed to invoke kubectl delete pod");
    }
}

/// Apply a JSON Pod manifest via `kubectl apply -f -`.
pub async fn apply_manifest(manifest: &serde_json::Value) -> anyhow::Result<()> {
    let manifest_str = serde_json::to_string(manifest)?;
    let mut child = Command::new("kubectl")
        .args(["apply", "-f", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(manifest_str.as_bytes()).await?;
        stdin.shutdown().await?;
    }

    let out = child.wait_with_output().await?;
    if !out.status.success() {
        tracing::error!(
            stderr = %String::from_utf8_lossy(&out.stderr),
            "failed to apply kubectl manifest"
        );
    }

    Ok(())
}
