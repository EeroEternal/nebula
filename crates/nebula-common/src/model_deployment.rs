use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model_request::ModelConfig;

/// Desired runtime state for a model deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Running,
    Stopped,
}

/// Per-replica role for pd_split topology (pool.md §pd_split v1.1).
/// `mixed` = full-request serving (today's behavior); `prefill`/`decode` =
/// role-biased with shared KV. serde default = mixed for backward compat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplicaRole {
    #[default]
    Mixed,
    Prefill,
    Decode,
}

/// Deployment topology (pool.md §pd_split v1.1). Default = replicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentTopology {
    #[default]
    Replicated,
    PdSplit,
}

/// Shared-KV orchestration params for pd_split (pool.md §pd_split v1.1).
/// All values are the rig-verified defaults from engines.md §7.4.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KvSharingSpec {
    /// lm:// address of the lmcache_server (TCP, hardware-neutral).
    pub remote_url: String,
    /// v1 fixed "naive" (other values rejected until verified).
    #[serde(default = "default_kv_serde")]
    pub serde: String,

    /// **L1 tier**: host DRAM offload size in GB.
    ///
    /// `None` keeps the engine default (LMCache 0.5.5: 5 GB per replica). It used to be
    /// hardcoded at 5 GB for every `pd_split` replica with nothing declaring or reporting
    /// it, so a node running N replicas silently reserved N x 5 GB of host memory
    /// (Batch 6.1 / `docs/dev/kv-governance.md`). Set it to make the reservation
    /// explicit and visible in the capacity snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_cpu_size_gb: Option<u32>,

    /// **L2 tier**: local NVMe path for the disk tier. `None` = L2 disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_disk_path: Option<String>,

    /// L2 tier size in GB (LMCache requires a size alongside the path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_disk_size_gb: Option<u32>,
}

fn default_kv_serde() -> String {
    "naive".to_string()
}

/// Per-replica placement desired state (subset of runtime `PlacementAssignment`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicaPlacementSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_indices: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_overrides: Option<ModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    /// Role for pd_split topology (ignored when replicated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ReplicaRole>,
}

/// Declares how a model should run — similar to a K8s Deployment spec.
///
/// Stored in etcd under `/deployments/{model_uid}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDeployment {
    /// References the ModelSpec's model_uid.
    pub model_uid: String,

    /// User-writable desired state: `running` or `stopped`.
    pub desired_state: DesiredState,

    /// Number of desired replicas.
    #[serde(default = "default_replicas")]
    pub replicas: u32,

    /// Minimum replicas for autoscaling. None means use `replicas` as fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_replicas: Option<u32>,

    /// Maximum replicas for autoscaling. None means use `replicas` as fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_replicas: Option<u32>,

    /// Optional node affinity constraint. None means Scheduler decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_affinity: Option<String>,

    /// Optional GPU affinity constraint. None means Scheduler decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_affinity: Option<Vec<u32>>,

    /// Per-replica placement overrides; length must equal `replicas` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_specs: Option<Vec<ReplicaPlacementSpec>>,

    /// Overrides for ModelSpec.config fields (merge semantics — only specified fields override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_overrides: Option<ModelConfig>,

    /// Pin to a registered `EngineImage.id` (preferred over free-form docker_image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,

    /// Optional pool affinity constraints. If non-empty, Scheduler only assigns to nodes in these pools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_pools: Option<Vec<String>>,

    /// Human reason when forcing an image / platform override past defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_override_reason: Option<String>,

    /// Compatibility rule ids applied when this deployment was last validated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compat_rule_ids: Vec<String>,

    /// Deployment topology. `None` = replicated (today's behavior).
    /// `pd_split` requires kv_sharing + role-complete replica_specs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<DeploymentTopology>,

    /// Shared-KV orchestration for pd_split topology.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_sharing: Option<KvSharingSpec>,

    /// Monotonically increasing version; bumped on every change.
    /// Scheduler uses this to detect whether a re-plan is needed.
    #[serde(default)]
    pub version: u64,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_replicas() -> u32 {
    1
}

/// Validate heterogeneous per-replica placement specs.
pub fn validate_replica_specs(replicas: u32, specs: &[ReplicaPlacementSpec]) -> Result<(), String> {
    if specs.len() != replicas as usize {
        return Err(format!(
            "replica_specs length {} must match replicas {replicas}",
            specs.len()
        ));
    }

    let mut used_gpus: HashMap<String, HashSet<u32>> = HashMap::new();
    for (idx, spec) in specs.iter().enumerate() {
        let Some(node) = spec.node_id.as_deref() else {
            continue;
        };
        let Some(gpus) = spec.gpu_indices.as_ref() else {
            continue;
        };
        let entry = used_gpus.entry(node.to_string()).or_default();
        for gpu in gpus {
            if !entry.insert(*gpu) {
                return Err(format!(
                    "replica_specs[{idx}] duplicates gpu {gpu} on node '{node}'"
                ));
            }
        }
    }
    Ok(())
}

/// LMCache environment variables for a pd_split replica role.
///
/// Single source of truth for BOTH execution planes (k8s-controller pod env
/// and node engine launch env) — engines.md §7.4 rig-verified values.
/// `Mixed` returns empty (replicated deployments unchanged).
pub fn kv_sharing_env(role: ReplicaRole, kv: &KvSharingSpec) -> Vec<(&'static str, String)> {
    // L1 (host DRAM): declared size wins; undeclared keeps the historical 5 GB so
    // existing deployments do not change behaviour when they upgrade.
    let local_cpu_size = kv.local_cpu_size_gb.unwrap_or(5);
    let mut env = vec![
        ("LMCACHE_USE_EXPERIMENTAL", "True".to_string()),
        ("LMCACHE_LOCAL_CPU", "True".to_string()),
        ("LMCACHE_MAX_LOCAL_CPU_SIZE", local_cpu_size.to_string()),
        ("LMCACHE_REMOTE_URL", kv.remote_url.clone()),
        ("LMCACHE_REMOTE_SERDE", kv.serde.clone()),
    ];
    // L2 (local NVMe): off unless a path is declared.
    if let Some(path) = kv
        .local_disk_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        env.push(("LMCACHE_LOCAL_DISK", path.to_string()));
        env.push((
            "LMCACHE_MAX_LOCAL_DISK_SIZE",
            kv.local_disk_size_gb.unwrap_or(0).to_string(),
        ));
    }
    match role {
        ReplicaRole::Prefill => env.push(("LMCACHE_SAVE_UNFULL_CHUNK", "True".to_string())),
        ReplicaRole::Decode => {
            env.push(("LMCACHE_RETRIEVE_LOCATIONS", "RemoteBackend".to_string()))
        }
        ReplicaRole::Mixed => return Vec::new(),
    }
    env
}

/// LMCache controller registration env for a pd_split replica.
///
/// The controller (`lmcache_controller`, ports 9000/9001) answers the
/// authoritative "is this prefix in the shared store" question via
/// `POST /lookup {"tokens": [...]}` — the warm signal for pd_split stage
/// routing (LMCache disables vLLM's kv-events once its connector is active,
/// so the P1c event index cannot serve that role; rig finding 2026-10-12).
///
/// `controller_url` is the monitor endpoint the engine reports to
/// (host:9001); `instance_id` must be unique per replica.
pub fn lmcache_controller_env(
    pull_url: &str,
    reply_url: &str,
    worker_port: u16,
    instance_id: &str,
) -> Vec<(&'static str, String)> {
    // LMCache validates all four when enable_controller=True
    // (config.py::_validate_config); a missing reply URL crash-loops the
    // engine at startup (rig, 2026-10-13).
    vec![
        ("LMCACHE_ENABLE_CONTROLLER", "True".to_string()),
        ("LMCACHE_CONTROLLER_PULL_URL", pull_url.to_string()),
        ("LMCACHE_CONTROLLER_REPLY_URL", reply_url.to_string()),
        ("LMCACHE_LMCACHE_WORKER_PORTS", worker_port.to_string()),
        ("LMCACHE_LMCACHE_INSTANCE_ID", instance_id.to_string()),
    ]
}

/// Extract `host:port` from a controller URL (`http://h:9000`, `h:9001/`, `h:9000`).
///
/// The readiness gate below needs a socket address; the env var is documented as a URL.
pub fn controller_addr(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next()?.trim();
    if host_port.is_empty() {
        return None;
    }
    Some(host_port.to_string())
}

/// Wait until `lmcache_controller` accepts a TCP connection, or `timeout` elapses.
///
/// Bring-up order matters and the failure is **sticky**: an engine that starts before the
/// controller registers but never reports its KV keys, so `/lookup` answers cold for that
/// engine's lifetime (measured, see `docs/dev/benefit-verification.md`
/// §"暖信号启动顺序"). Waiting here turns that silent degradation into a retryable
/// launch failure.
///
/// Returns `Ok(())` immediately when no controller URL is configured.
pub async fn wait_for_lmcache_controller(
    controller_url: Option<&str>,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let Some(url) = controller_url.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok(());
    };
    let Some(addr) = controller_addr(url) else {
        return Err(format!("unparseable NEBULA_LMCACHE_CONTROLLER_URL: {url}"));
    };
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "lmcache_controller not reachable at {addr} after {}s; starting an engine now would register but never report KV keys (pd_split would silently run as single-Prefill)",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// `--kv-transfer-config` JSON for a pd_split replica role.
///
/// prefill → `kv_producer` (prefills and stores to the shared KV);
/// decode → `kv_both` (retrieves, self-prefills on miss — correctness
/// fallback verified on the rig). `Mixed` returns None.
pub fn kv_transfer_config(role: ReplicaRole) -> Option<String> {
    let (kv_role, rpc_port) = match role {
        ReplicaRole::Prefill => ("kv_producer", "producer1"),
        ReplicaRole::Decode => ("kv_both", "consumer1"),
        ReplicaRole::Mixed => return None,
    };
    Some(
        serde_json::json!({
            "kv_connector": "LMCacheConnectorV1",
            "kv_role": kv_role,
            "kv_connector_extra_config": {
                "discard_partial_chunks": false,
                "lmcache_rpc_port": rpc_port,
            }
        })
        .to_string(),
    )
}

/// Approved pd_split engine images (review #4: a deployment must reference an
/// artifact that actually passed the LMCache gate — the gate's pip-install
/// environment is not the shipped image, and `:latest`-style floating tags
/// drift silently).
///
/// `NEBULA_APPROVED_PD_IMAGES` lists the gate-passed refs (comma-separated,
/// e.g. `nebula/vllm:0.23.0-lmcache0.5.5` or a digest-pinned
/// `registry/img@sha256:...`). pd_split is rejected unless the deployment's
/// image is on the list, so unverified combinations stay out.
pub fn validate_pd_split_image(
    docker_image: Option<&str>,
    approved: &[String],
) -> Result<(), String> {
    let Some(image) = docker_image else {
        return Err(
            "pd_split requires an explicit engine image (spec.docker_image) from the \
             gate-passed allowlist"
                .to_string(),
        );
    };
    if approved.is_empty() {
        return Err(
            "pd_split is disabled: NEBULA_APPROVED_PD_IMAGES is empty (run \
             scripts/check_lmcache_compat.sh --full on the artifact, then list it)"
                .to_string(),
        );
    }
    if !approved.iter().any(|a| a == image) {
        return Err(format!(
            "pd_split image '{image}' is not on the approved list (NEBULA_APPROVED_PD_IMAGES: {})",
            approved.join(", ")
        ));
    }
    Ok(())
}

/// Read the comma-separated `NEBULA_APPROVED_PD_IMAGES` allowlist from the process environment.
pub fn approved_pd_images_from_env() -> Vec<String> {
    std::env::var("NEBULA_APPROVED_PD_IMAGES")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Merge two ModelConfig values. `overrides` fields win when present.
pub fn merge_model_config(
    base: Option<&ModelConfig>,
    overrides: Option<&ModelConfig>,
) -> Option<ModelConfig> {
    match (base, overrides) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(o)) => Some(o.clone()),
        (Some(b), Some(o)) => Some(ModelConfig {
            tensor_parallel_size: o.tensor_parallel_size.or(b.tensor_parallel_size),
            gpu_memory_utilization: o.gpu_memory_utilization.or(b.gpu_memory_utilization),
            max_model_len: o.max_model_len.or(b.max_model_len),
            required_vram_mb: o.required_vram_mb.or(b.required_vram_mb),
            lora_modules: o.lora_modules.clone().or_else(|| b.lora_modules.clone()),
            served_model_name: o
                .served_model_name
                .clone()
                .or_else(|| b.served_model_name.clone()),
            kv_cache_dtype: o
                .kv_cache_dtype
                .clone()
                .or_else(|| b.kv_cache_dtype.clone()),
            trust_remote_code: o.trust_remote_code.or(b.trust_remote_code),
            enable_expert_parallel: o.enable_expert_parallel.or(b.enable_expert_parallel),
            block_size: o.block_size.or(b.block_size),
            tokenizer_mode: o
                .tokenizer_mode
                .clone()
                .or_else(|| b.tokenizer_mode.clone()),
            enable_prefix_caching: o.enable_prefix_caching.or(b.enable_prefix_caching),
            speculative_config: o
                .speculative_config
                .clone()
                .or_else(|| b.speculative_config.clone()),
        }),
    }
}

/// Effective merged ModelConfig for a specific replica index:
/// `base_config` -> `deployment.config_overrides` -> `replica_specs[replica_id].config_overrides`.
pub fn effective_replica_config(
    deployment: &ModelDeployment,
    base_config: Option<&ModelConfig>,
    replica_id: usize,
) -> Option<ModelConfig> {
    let base = merge_model_config(base_config, deployment.config_overrides.as_ref());
    let replica_override = deployment
        .replica_specs
        .as_ref()
        .and_then(|specs| specs.get(replica_id))
        .and_then(|s| s.config_overrides.as_ref());
    merge_model_config(base.as_ref(), replica_override)
}

/// Effective image reference (image_id or docker_image) for a specific replica index:
/// `replica_specs[replica_id].image_id` -> `deployment.image_id` -> `spec_docker_image`.
pub fn effective_replica_image<'a>(
    deployment: &'a ModelDeployment,
    spec_docker_image: Option<&'a str>,
    replica_id: usize,
) -> Option<&'a str> {
    deployment
        .replica_specs
        .as_ref()
        .and_then(|specs| specs.get(replica_id))
        .and_then(|s| s.image_id.as_deref())
        .or(deployment.image_id.as_deref())
        .or(spec_docker_image)
}

/// Normalize a platform identifier into its canonical hardware family
/// (`"nvidia-cuda"` / `"cuda-12.4-sm89"` -> `"cuda"`,
/// `"ascend-cann8.0-910b"` / `"huawei-ascend"` -> `"ascend"`,
/// `"amd-rocm"` -> `"rocm"`).
pub fn hardware_family_of_platform(platform: &str) -> String {
    let lower = platform.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return String::new();
    }
    if lower.contains("cuda") || lower.starts_with("nvidia") {
        return "cuda".to_string();
    }
    if lower.contains("ascend") || lower.contains("cann") || lower.contains("npu") {
        return "ascend".to_string();
    }
    if lower.contains("rocm") || lower.starts_with("amd") {
        return "rocm".to_string();
    }
    lower
        .split(['-', '_', ':'])
        .next()
        .unwrap_or("")
        .to_string()
}

/// Shared validation for pd_split deployments (pool.md §pd_split v1.1).
/// Called from BFF create/update AND CLI deploy — not BFF-only (assessment #4).
///
/// Rules:
/// 1. pd_split requires kv_sharing with a valid remote_url (lm://…)
/// 2. pd_split requires every replica_spec to have a non-mixed role,
///    with at least 1 prefill and 1 decode
/// 3. pd_split requires engine_type=vllm (LMCacheConnector)
/// 4. replicated + explicit roles = reject (prevent misunderstanding)
/// 5. pd_split serde must be "naive" (only verified value)
/// 6. pd_split v1 forbids autoscaling (min_replicas / max_replicas != replicas)
/// 7. v1 conservative KV-compat scope: TP=1 per replica, uniform kv_cache_dtype,
///    uniform block_size=16, and uniform CUDA hardware family across replicas
#[cfg(test)]
pub fn validate_pd_split(
    deployment: &ModelDeployment,
    engine_type: Option<&str>,
) -> Result<(), String> {
    validate_pd_split_with_context(deployment, engine_type, None, &[])
}

pub fn validate_pd_split_with_context(
    deployment: &ModelDeployment,
    engine_type: Option<&str>,
    base_config: Option<&ModelConfig>,
    replica_platforms: &[Option<String>],
) -> Result<(), String> {
    let topology = deployment
        .topology
        .unwrap_or(DeploymentTopology::Replicated);

    match topology {
        DeploymentTopology::Replicated => {
            // Rule 4: replicated + explicit roles = reject
            if let Some(specs) = deployment.replica_specs.as_ref() {
                if specs.iter().any(|s| s.role.is_some()) {
                    return Err(
                        "replica role is only valid with topology=pd_split; remove role fields or set topology".into(),
                    );
                }
            }
            Ok(())
        }
        DeploymentTopology::PdSplit => {
            // Rule 3: engine must be vllm
            let engine = engine_type.unwrap_or("vllm");
            if engine != "vllm" {
                return Err(format!(
                    "pd_split requires engine_type=vllm (LMCacheConnector); got '{engine}'"
                ));
            }

            // Rule 6: autoscaling is forbidden in pd_split v1
            if let Some(min) = deployment.min_replicas {
                if min != deployment.replicas {
                    return Err(format!(
                        "pd_split v1 does not support autoscaling; min_replicas ({min}) must equal replicas ({})",
                        deployment.replicas
                    ));
                }
            }
            if let Some(max) = deployment.max_replicas {
                if max != deployment.replicas {
                    return Err(format!(
                        "pd_split v1 does not support autoscaling; max_replicas ({max}) must equal replicas ({})",
                        deployment.replicas
                    ));
                }
            }

            // Rule 1: kv_sharing required
            let kv = deployment
                .kv_sharing
                .as_ref()
                .ok_or_else(|| "pd_split requires kv_sharing (remote_url + serde)".to_string())?;
            if !kv.remote_url.starts_with("lm://") {
                return Err(format!(
                    "kv_sharing.remote_url must start with lm:// (got '{}')",
                    kv.remote_url
                ));
            }
            // Rule 5: serde must be naive (only verified value)
            if kv.serde != "naive" {
                return Err(format!(
                    "kv_sharing.serde must be 'naive' (only verified value; got '{}')",
                    kv.serde
                ));
            }

            // Rule 2: roles required and complete
            let specs = deployment.replica_specs.as_ref().ok_or_else(|| {
                "pd_split requires replica_specs with explicit roles (at least 1 prefill and 1 decode)"
                    .to_string()
            })?;
            if specs.len() != deployment.replicas as usize {
                return Err(format!(
                    "replica_specs length {} must match replicas {}",
                    specs.len(),
                    deployment.replicas
                ));
            }
            let mut has_prefill = false;
            let mut has_decode = false;
            let mut expected_dtype: Option<String> = None;
            let mut expected_block_size: Option<u32> = None;
            let mut expected_hw_family: Option<String> = None;

            for (idx, spec) in specs.iter().enumerate() {
                match spec.role {
                    Some(ReplicaRole::Prefill) => has_prefill = true,
                    Some(ReplicaRole::Decode) => has_decode = true,
                    Some(ReplicaRole::Mixed) | None => {
                        return Err(format!(
                            "pd_split replica_specs[{idx}] must have role=prefill or decode (not mixed)"
                        ));
                    }
                }

                // Rule: declared tier quotas must be sane. A disk tier without a size is
                // silently 0 GB in LMCache, i.e. declared but disabled.
                if let Some(gb) = kv.local_cpu_size_gb {
                    if gb == 0 {
                        return Err("kv_sharing.local_cpu_size_gb must be > 0".into());
                    }
                }
                if let Some(path) = kv.local_disk_path.as_deref() {
                    if path.trim().is_empty() {
                        return Err("kv_sharing.local_disk_path is empty".into());
                    }
                    match kv.local_disk_size_gb {
                        Some(0) | None => {
                            return Err(
                                "kv_sharing.local_disk_path requires local_disk_size_gb > 0".into(),
                            )
                        }
                        Some(_) => {}
                    }
                }

                // Rule 7: conservative KV-compatibility scope per replica
                let cfg = effective_replica_config(deployment, base_config, idx);
                let tp = cfg
                    .as_ref()
                    .and_then(|c| c.tensor_parallel_size)
                    .unwrap_or(1);
                if tp != 1 {
                    return Err(format!(
                        "pd_split v1 requires tensor_parallel_size=1 per replica (replica_specs[{idx}] has tensor_parallel_size={tp})"
                    ));
                }
                let gpus = spec
                    .gpu_indices
                    .as_ref()
                    .or(deployment.gpu_affinity.as_ref());
                if let Some(g) = gpus {
                    if g.len() > 1 {
                        return Err(format!(
                            "pd_split v1 requires 1 GPU per replica (TP=1); replica_specs[{idx}] specifies {} GPUs",
                            g.len()
                        ));
                    }
                }

                let dtype = cfg
                    .as_ref()
                    .and_then(|c| c.kv_cache_dtype.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("auto")
                    .to_ascii_lowercase();
                if let Some(ref exp) = expected_dtype {
                    if exp != &dtype {
                        return Err(format!(
                            "pd_split requires identical kv_cache_dtype across all replicas (replica 0 has '{exp}', replica {idx} has '{dtype}')"
                        ));
                    }
                } else {
                    expected_dtype = Some(dtype);
                }

                let block_size = cfg.as_ref().and_then(|c| c.block_size).unwrap_or(16);
                if let Some(exp) = expected_block_size {
                    if exp != block_size {
                        return Err(format!(
                            "pd_split requires identical block_size across all replicas (replica 0 has {exp}, replica {idx} has {block_size})"
                        ));
                    }
                } else {
                    expected_block_size = Some(block_size);
                }
                if block_size != 16 {
                    return Err(format!(
                        "pd_split v1 requires block_size=16 (replica {idx} has block_size={block_size})"
                    ));
                }

                if let Some(Some(plat)) = replica_platforms.get(idx) {
                    let fam = hardware_family_of_platform(plat);
                    if !fam.is_empty() {
                        if fam != "cuda" {
                            return Err(format!(
                                "pd_split v1 KV sharing is only verified on CUDA hardware family (replica {idx} resolved to platform '{plat}')"
                            ));
                        }
                        if let Some(ref exp) = expected_hw_family {
                            if exp != &fam {
                                return Err(format!(
                                    "pd_split requires identical hardware family across all replicas (replica 0 has '{exp}', replica {idx} has '{fam}')"
                                ));
                            }
                        } else {
                            expected_hw_family = Some(fam);
                        }
                    }
                }
            }
            if !has_prefill {
                return Err("pd_split requires at least 1 prefill replica".into());
            }
            if !has_decode {
                return Err("pd_split requires at least 1 decode replica".into());
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pd_dep() -> ModelDeployment {
        ModelDeployment {
            model_uid: "m1".into(),
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
            kv_sharing: Some(KvSharingSpec {
                remote_url: "lm://host:6800".into(),
                serde: "naive".into(),
                ..Default::default()
            }),
            version: 1,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn kv_sharing_env_matches_rig_verified_values() {
        let kv = KvSharingSpec {
            remote_url: "lm://h:6800".into(),
            serde: "naive".into(),
            ..Default::default()
        };
        let prefill = kv_sharing_env(ReplicaRole::Prefill, &kv);
        let get = |env: &Vec<(&str, String)>, k: &str| {
            env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
        };
        assert_eq!(
            get(&prefill, "LMCACHE_REMOTE_URL"),
            Some("lm://h:6800".into())
        );
        assert_eq!(
            get(&prefill, "LMCACHE_SAVE_UNFULL_CHUNK"),
            Some("True".into())
        );
        assert_eq!(get(&prefill, "LMCACHE_RETRIEVE_LOCATIONS"), None);

        let decode = kv_sharing_env(ReplicaRole::Decode, &kv);
        assert_eq!(
            get(&decode, "LMCACHE_RETRIEVE_LOCATIONS"),
            Some("RemoteBackend".into())
        );
        assert_eq!(get(&decode, "LMCACHE_SAVE_UNFULL_CHUNK"), None);

        // Mixed → no injection (replicated deployments unchanged)
        assert!(kv_sharing_env(ReplicaRole::Mixed, &kv).is_empty());
    }

    #[test]
    fn controller_env_matches_lmcache_config_keys() {
        let env = lmcache_controller_env("host:8300", "host:8400", 8001, "m-0");
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("LMCACHE_ENABLE_CONTROLLER"), Some("True".into()));
        assert_eq!(get("LMCACHE_CONTROLLER_PULL_URL"), Some("host:8300".into()));
        assert_eq!(
            get("LMCACHE_CONTROLLER_REPLY_URL"),
            Some("host:8400".into())
        );
        assert_eq!(get("LMCACHE_LMCACHE_WORKER_PORTS"), Some("8001".into()));
        assert_eq!(get("LMCACHE_LMCACHE_INSTANCE_ID"), Some("m-0".into()));
    }

    #[test]
    fn kv_transfer_config_roles() {
        let pf = kv_transfer_config(ReplicaRole::Prefill).unwrap();
        assert!(pf.contains("\"kv_role\":\"kv_producer\""), "{pf}");
        assert!(pf.contains("LMCacheConnectorV1"));
        let dec = kv_transfer_config(ReplicaRole::Decode).unwrap();
        assert!(dec.contains("\"kv_role\":\"kv_both\""), "{dec}");
        assert!(kv_transfer_config(ReplicaRole::Mixed).is_none());
        // valid JSON
        serde_json::from_str::<serde_json::Value>(&pf).expect("valid json");
        serde_json::from_str::<serde_json::Value>(&dec).expect("valid json");
    }

    /// Wire format is the documented contract (pool.md): lowercase
    /// `prefill`/`decode`/`mixed` and `pd_split` — the rig controller
    /// rejected the deployment when this mismatched (serde default is
    /// PascalCase), silently creating no pods.
    #[test]
    fn role_and_topology_wire_format_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&ReplicaRole::Prefill).unwrap(),
            "\"prefill\""
        );
        assert_eq!(
            serde_json::to_string(&ReplicaRole::Decode).unwrap(),
            "\"decode\""
        );
        assert_eq!(
            serde_json::to_string(&ReplicaRole::Mixed).unwrap(),
            "\"mixed\""
        );
        assert_eq!(
            serde_json::to_string(&DeploymentTopology::PdSplit).unwrap(),
            "\"pd_split\""
        );
        let d: ModelDeployment = serde_json::from_str(
            r#"{"model_uid":"m","desired_state":"running","replicas":2,
                "replica_specs":[{"role":"prefill"},{"role":"decode"}],
                "topology":"pd_split",
                "kv_sharing":{"remote_url":"lm://h:1"}}"#,
        )
        .unwrap();
        assert_eq!(d.topology, Some(DeploymentTopology::PdSplit));
        assert_eq!(
            d.replica_specs.as_ref().unwrap()[1].role,
            Some(ReplicaRole::Decode)
        );
    }

    #[test]
    fn approved_image_gate() {
        let approved = vec!["nebula/vllm:0.23.0-lmcache0.5.5".to_string()];
        assert!(
            validate_pd_split_image(Some("nebula/vllm:0.23.0-lmcache0.5.5"), &approved).is_ok()
        );
        assert!(validate_pd_split_image(Some("vllm/vllm-openai:latest"), &approved).is_err());
        assert!(validate_pd_split_image(None, &approved).is_err());
        // Empty allowlist = pd_split disabled entirely.
        assert!(validate_pd_split_image(Some("nebula/vllm:0.23.0-lmcache0.5.5"), &[]).is_err());
    }

    #[test]
    fn pd_split_valid() {
        assert!(validate_pd_split(&pd_dep(), Some("vllm")).is_ok());
    }

    #[test]
    fn pd_split_requires_kv_sharing() {
        let mut d = pd_dep();
        d.kv_sharing = None;
        assert!(validate_pd_split(&d, Some("vllm")).is_err());
    }

    #[test]
    fn pd_split_rejects_sglang() {
        assert!(validate_pd_split(&pd_dep(), Some("sglang")).is_err());
    }

    #[test]
    fn pd_split_rejects_mixed_role() {
        let mut d = pd_dep();
        d.replica_specs.as_mut().unwrap()[0].role = Some(ReplicaRole::Mixed);
        assert!(validate_pd_split(&d, Some("vllm")).is_err());
    }

    #[test]
    fn pd_split_requires_both_roles() {
        let mut d = pd_dep();
        d.replica_specs.as_mut().unwrap()[1].role = Some(ReplicaRole::Prefill);
        assert!(validate_pd_split(&d, Some("vllm")).is_err()); // no decode
    }

    #[test]
    fn pd_split_rejects_non_naive_serde() {
        let mut d = pd_dep();
        d.kv_sharing.as_mut().unwrap().serde = "cachegen".into();
        assert!(validate_pd_split(&d, Some("vllm")).is_err());
    }

    #[test]
    fn replicated_rejects_explicit_roles() {
        let mut d = pd_dep();
        d.topology = None; // replicated
        assert!(validate_pd_split(&d, Some("vllm")).is_err());
    }

    #[test]
    fn replicated_without_roles_is_fine() {
        let d = ModelDeployment {
            topology: None,
            kv_sharing: None,
            replica_specs: None,
            ..pd_dep()
        };
        assert!(validate_pd_split(&d, Some("vllm")).is_ok());
    }

    #[test]
    fn validate_replica_specs_length_mismatch() {
        let err = validate_replica_specs(2, &[ReplicaPlacementSpec::default()]).unwrap_err();
        assert!(err.contains("must match replicas"));
    }

    #[test]
    fn validate_replica_specs_rejects_duplicate_gpu() {
        let specs = vec![
            ReplicaPlacementSpec {
                node_id: Some("n1".into()),
                gpu_indices: Some(vec![0, 1]),
                ..Default::default()
            },
            ReplicaPlacementSpec {
                node_id: Some("n1".into()),
                gpu_indices: Some(vec![1]),
                ..Default::default()
            },
        ];
        let err = validate_replica_specs(2, &specs).unwrap_err();
        assert!(err.contains("duplicates gpu 1"));
    }

    #[test]
    fn pd_split_rejects_autoscaling_bounds() {
        let mut d = pd_dep();
        d.min_replicas = Some(1);
        assert!(validate_pd_split(&d, Some("vllm")).is_err());

        let mut d2 = pd_dep();
        d2.max_replicas = Some(4);
        assert!(validate_pd_split(&d2, Some("vllm")).is_err());

        let mut d3 = pd_dep();
        d3.min_replicas = Some(2);
        d3.max_replicas = Some(2);
        assert!(validate_pd_split(&d3, Some("vllm")).is_ok());
    }

    #[test]
    fn pd_split_enforces_kv_compat_scope() {
        // TP > 1 in base config rejected
        let base_tp2 = ModelConfig {
            tensor_parallel_size: Some(2),
            ..Default::default()
        };
        let err = validate_pd_split_with_context(&pd_dep(), Some("vllm"), Some(&base_tp2), &[])
            .unwrap_err();
        assert!(err.contains("tensor_parallel_size=1"), "{err}");

        // TP > 1 in replica override rejected
        let mut d_tp = pd_dep();
        d_tp.replica_specs.as_mut().unwrap()[1].config_overrides = Some(ModelConfig {
            tensor_parallel_size: Some(2),
            ..Default::default()
        });
        assert!(validate_pd_split(&d_tp, Some("vllm")).is_err());

        // Mismatched kv_cache_dtype across replicas rejected
        let mut d_dtype = pd_dep();
        d_dtype.replica_specs.as_mut().unwrap()[0].config_overrides = Some(ModelConfig {
            kv_cache_dtype: Some("fp8".into()),
            ..Default::default()
        });
        let err = validate_pd_split(&d_dtype, Some("vllm")).unwrap_err();
        assert!(err.contains("kv_cache_dtype"), "{err}");

        // Uniform kv_cache_dtype across replicas accepted
        d_dtype.replica_specs.as_mut().unwrap()[1].config_overrides = Some(ModelConfig {
            kv_cache_dtype: Some("fp8".into()),
            ..Default::default()
        });
        assert!(validate_pd_split(&d_dtype, Some("vllm")).is_ok());

        // Non-16 block_size rejected
        let mut d_bs = pd_dep();
        d_bs.config_overrides = Some(ModelConfig {
            block_size: Some(32),
            ..Default::default()
        });
        let err = validate_pd_split(&d_bs, Some("vllm")).unwrap_err();
        assert!(err.contains("block_size=16"), "{err}");

        // Non-CUDA hardware family rejected
        let err = validate_pd_split_with_context(
            &pd_dep(),
            Some("vllm"),
            None,
            &[
                Some(crate::DEFAULT_NODE_PLATFORM.into()),
                Some("huawei-ascend".into()),
            ],
        )
        .unwrap_err();
        assert!(err.contains("CUDA hardware family"), "{err}");

        // Real production platform identifier ("nvidia-cuda") and CUDA variants accepted
        assert_eq!(
            hardware_family_of_platform(crate::DEFAULT_NODE_PLATFORM),
            "cuda"
        );
        assert!(validate_pd_split_with_context(
            &pd_dep(),
            Some("vllm"),
            None,
            &[
                Some(crate::DEFAULT_NODE_PLATFORM.into()),
                Some("nvidia-cuda".into()),
            ],
        )
        .is_ok());
        assert!(validate_pd_split_with_context(
            &pd_dep(),
            Some("vllm"),
            None,
            &[Some("cuda-12.4-sm89".into()), Some("cuda-12.4-sm90".into())],
        )
        .is_ok());
    }

    #[test]
    fn controller_url_is_reduced_to_a_socket_address() {
        assert_eq!(
            controller_addr("http://127.0.0.1:9000").as_deref(),
            Some("127.0.0.1:9000")
        );
        assert_eq!(
            controller_addr("127.0.0.1:9001/").as_deref(),
            Some("127.0.0.1:9001")
        );
        assert_eq!(
            controller_addr(" http://lmcache-ctrl.ns:9000 ").as_deref(),
            Some("lmcache-ctrl.ns:9000")
        );
        assert_eq!(controller_addr("   "), None);
    }

    #[tokio::test]
    async fn controller_gate_fails_fast_when_nothing_listens() {
        // No controller configured -> no wait, launch proceeds.
        assert!(
            wait_for_lmcache_controller(None, std::time::Duration::from_millis(10))
                .await
                .is_ok()
        );

        // Nothing listening on this port -> the gate must fail (so the scheduler retries
        // instead of starting an engine that would never report its KV keys).
        let started = std::time::Instant::now();
        let err =
            wait_for_lmcache_controller(Some("127.0.0.1:9"), std::time::Duration::from_millis(300))
                .await
                .expect_err("must not succeed without a listener");
        assert!(err.contains("not reachable"), "{err}");
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(300),
            "the gate must wait for its budget, not give up instantly"
        );
    }

    #[tokio::test]
    async fn controller_gate_passes_once_something_listens() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Accept in the background so the connect succeeds.
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        assert!(wait_for_lmcache_controller(
            Some(&format!("http://{addr}")),
            std::time::Duration::from_secs(2)
        )
        .await
        .is_ok());
    }

    #[test]
    fn kv_tier_declaration_maps_to_lmcache_env_and_defaults_stay_unchanged() {
        // Undeclared: byte-for-byte the historical behaviour (L1 on, 5 GB, no L2).
        let kv = KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            ..Default::default()
        };
        let env = kv_sharing_env(ReplicaRole::Prefill, &kv);
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("LMCACHE_MAX_LOCAL_CPU_SIZE").as_deref(), Some("5"));
        assert!(!env.iter().any(|(n, _)| *n == "LMCACHE_LOCAL_DISK"));

        // Declared L1 + L2.
        let kv = KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(24),
            local_disk_path: Some("/nvme/kv".into()),
            local_disk_size_gb: Some(512),
        };
        let env = kv_sharing_env(ReplicaRole::Decode, &kv);
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("LMCACHE_MAX_LOCAL_CPU_SIZE").as_deref(), Some("24"));
        assert_eq!(get("LMCACHE_LOCAL_DISK").as_deref(), Some("/nvme/kv"));
        assert_eq!(get("LMCACHE_MAX_LOCAL_DISK_SIZE").as_deref(), Some("512"));
        // role-specific keys still present
        assert_eq!(
            get("LMCACHE_RETRIEVE_LOCATIONS").as_deref(),
            Some("RemoteBackend")
        );
    }

    #[test]
    fn kv_tier_declaration_is_validated() {
        let mut d = pd_dep();
        d.kv_sharing = Some(KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(0),
            ..Default::default()
        });
        assert!(validate_pd_split(&d, Some("vllm"))
            .unwrap_err()
            .contains("local_cpu_size_gb"));

        let mut d2 = pd_dep();
        d2.kv_sharing = Some(KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_disk_path: Some("/nvme/kv".into()),
            local_disk_size_gb: None,
            ..Default::default()
        });
        assert!(validate_pd_split(&d2, Some("vllm"))
            .unwrap_err()
            .contains("local_disk_size_gb"));

        let mut ok = pd_dep();
        ok.kv_sharing = Some(KvSharingSpec {
            remote_url: "lm://store:6800".into(),
            serde: "naive".into(),
            local_cpu_size_gb: Some(8),
            local_disk_path: Some("/nvme/kv".into()),
            local_disk_size_gb: Some(64),
        });
        assert!(validate_pd_split(&ok, Some("vllm")).is_ok());
    }

    #[test]
    fn effective_replica_image_precedence() {
        let mut d = pd_dep();
        assert_eq!(
            effective_replica_image(&d, Some("spec/img:1"), 0),
            Some("spec/img:1")
        );
        d.image_id = Some("dep/img:2".into());
        assert_eq!(
            effective_replica_image(&d, Some("spec/img:1"), 0),
            Some("dep/img:2")
        );
        d.replica_specs.as_mut().unwrap()[1].image_id = Some("rep/img:3".into());
        assert_eq!(
            effective_replica_image(&d, Some("spec/img:1"), 0),
            Some("dep/img:2")
        );
        assert_eq!(
            effective_replica_image(&d, Some("spec/img:1"), 1),
            Some("rep/img:3")
        );
    }
}
