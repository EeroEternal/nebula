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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KvSharingSpec {
    /// lm:// address of the lmcache_server (TCP, hardware-neutral).
    pub remote_url: String,
    /// v1 fixed "naive" (other values rejected until verified).
    #[serde(default = "default_kv_serde")]
    pub serde: String,
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
    let mut env = vec![
        ("LMCACHE_USE_EXPERIMENTAL", "True".to_string()),
        ("LMCACHE_LOCAL_CPU", "True".to_string()),
        ("LMCACHE_MAX_LOCAL_CPU_SIZE", "5".to_string()),
        ("LMCACHE_REMOTE_URL", kv.remote_url.clone()),
        ("LMCACHE_REMOTE_SERDE", kv.serde.clone()),
    ];
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
/// 6. v1 conservative KV-compat scope: TP=1 per replica (untested beyond)
pub fn validate_pd_split(
    deployment: &ModelDeployment,
    engine_type: Option<&str>,
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
}
