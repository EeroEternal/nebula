use std::collections::{HashMap, HashSet};

use nebula_common::{
    build_engine_extra_args_lenient, image_platforms_match, resolve_node_platform, EngineImage,
    HardwarePool, ModelConfig, ModelDeployment, ModelRequest, ModelSpec, NodeStatus,
    PlacementAssignment, PlacementPlan,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::util::now_ms;

const NODE_STALE_MS: u64 = 60_000;

async fn load_allowed_pool_nodes(
    store: &EtcdMetaStore,
    allowed_pools: Option<&[String]>,
) -> anyhow::Result<Option<HashSet<String>>> {
    let Some(pools) = allowed_pools else {
        return Ok(None);
    };
    if pools.is_empty() {
        return Ok(None);
    }

    let kvs = store.list_prefix("/pools/").await?;
    let mut allowed_node_ids = HashSet::new();
    let mut matched_pools = 0usize;

    for (_, val, _) in kvs {
        if let Ok(pool) = serde_json::from_slice::<HardwarePool>(&val) {
            if pools.iter().any(|p| p == &pool.pool_id) {
                matched_pools += 1;
                if pool.schedulable {
                    for node_id in pool.node_ids {
                        allowed_node_ids.insert(node_id);
                    }
                }
            }
        }
    }

    if matched_pools == 0 {
        anyhow::bail!("none of the allowed_pools {:?} exist in /pools/", pools);
    }

    Ok(Some(allowed_node_ids))
}

async fn load_image_platforms(
    store: &EtcdMetaStore,
    docker_image: Option<&str>,
) -> Vec<String> {
    let Some(wanted) = docker_image.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let Ok(kvs) = store.list_prefix("/images/").await else {
        return Vec::new();
    };
    for (_, val, _) in kvs {
        let Ok(img) = serde_json::from_slice::<EngineImage>(&val) else {
            continue;
        };
        if img.id == wanted || img.image == wanted {
            return img.platforms;
        }
    }
    Vec::new()
}

fn node_matches_platforms(node: &NodeStatus, platforms: &[String]) -> bool {
    if platforms.is_empty() {
        return true;
    }
    let platform = resolve_node_platform(node);
    image_platforms_match(platforms, &platform)
}

pub async fn list_used_resources(
    store: &EtcdMetaStore,
) -> anyhow::Result<(HashSet<u16>, HashMap<String, HashSet<u32>>)> {
    let mut used_ports = HashSet::new();
    let mut used_gpus: HashMap<String, HashSet<u32>> = HashMap::new();
    if let Ok(kvs) = store.list_prefix("/placements/").await {
        for (_, val, _) in kvs {
            if let Ok(p) = serde_json::from_slice::<PlacementPlan>(&val) {
                for a in p.assignments {
                    used_ports.insert(a.port);
                    if let Some(indices) = a.effective_gpu_indices() {
                        let entry = used_gpus.entry(a.node_id.clone()).or_default();
                        for idx in indices {
                            entry.insert(idx);
                        }
                    }
                }
            }
        }
    }
    Ok((used_ports, used_gpus))
}

pub async fn select_node_and_gpus(
    store: &EtcdMetaStore,
    req: &ModelRequest,
    used_gpus: &HashMap<String, HashSet<u32>>,
) -> anyhow::Result<(String, Vec<u32>)> {
    let required_vram_mb = req
        .request
        .config
        .as_ref()
        .and_then(|c| c.required_vram_mb)
        .unwrap_or(0);

    let tp_size = req
        .request
        .config
        .as_ref()
        .and_then(|c| c.tensor_parallel_size)
        .unwrap_or(1)
        .max(1) as usize;

    // Manual Override: prefer gpu_indices, fall back to gpu_index
    if let Some(target_node) = &req.request.node_id {
        let indices = req
            .request
            .gpu_indices
            .clone()
            .or_else(|| req.request.gpu_index.map(|i| vec![i]))
            .unwrap_or_default();
        return Ok((target_node.clone(), indices));
    }

    let mut nodes: Vec<NodeStatus> = Vec::new();
    if let Ok(kvs) = store.list_prefix("/nodes/").await {
        for (_, val, _) in kvs {
            if let Ok(status) = serde_json::from_slice::<NodeStatus>(&val) {
                nodes.push(status);
            }
        }
    }

    let now = now_ms();
    let image_platforms = load_image_platforms(store, req.request.docker_image.as_deref()).await;

    // Try to find a node with `tp_size` free GPUs
    let mut best_node: Option<(String, Vec<u32>, u64)> = None;
    let mut rejected_platform = 0u32;

    for node in &nodes {
        if now.saturating_sub(node.last_heartbeat_ms) > NODE_STALE_MS {
            continue;
        }
        if !node_matches_platforms(node, &image_platforms) {
            rejected_platform += 1;
            continue;
        }

        let used = used_gpus.get(&node.node_id);

        // Collect available GPUs sorted by free memory (descending)
        let mut available: Vec<(u32, u64)> = node
            .gpus
            .iter()
            .filter(|gpu| {
                if let Some(used_set) = used {
                    if used_set.contains(&gpu.index) {
                        return false;
                    }
                }
                let free = gpu.memory_total_mb.saturating_sub(gpu.memory_used_mb);
                free >= required_vram_mb
            })
            .map(|gpu| {
                let free = gpu.memory_total_mb.saturating_sub(gpu.memory_used_mb);
                (gpu.index, free)
            })
            .collect();

        available.sort_by(|a, b| b.1.cmp(&a.1));

        if available.len() >= tp_size {
            let selected: Vec<u32> = available[..tp_size].iter().map(|(idx, _)| *idx).collect();
            let total_free: u64 = available[..tp_size].iter().map(|(_, free)| free).sum();

            match best_node {
                Some((_, _, best_free)) if total_free <= best_free => {}
                _ => {
                    best_node = Some((node.node_id.clone(), selected, total_free));
                }
            }
        }
    }

    if let Some((node_id, indices, _)) = best_node {
        return Ok((node_id, indices));
    }

    // Fallback: return any healthy platform-compatible node with no GPU selection
    for node in &nodes {
        if now.saturating_sub(node.last_heartbeat_ms) > NODE_STALE_MS {
            continue;
        }
        if !node_matches_platforms(node, &image_platforms) {
            continue;
        }
        return Ok((node.node_id.clone(), vec![]));
    }

    if rejected_platform > 0 && !image_platforms.is_empty() {
        let reason = nebula_common::PlacementRejectReason::platform_incompatible(
            image_platforms,
            rejected_platform,
            None,
        );
        anyhow::bail!("{}", reason.format_error());
    }
    anyhow::bail!(
        "{}",
        nebula_common::PlacementRejectReason::no_healthy_nodes().format_error()
    )
}

#[allow(dead_code)] // kept for offline tooling / emergency rebuilds of legacy plans
pub fn build_extra_args(req: &ModelRequest) -> Option<Vec<String>> {
    build_extra_args_from_config(req.request.engine_type.as_deref(), req.request.config.as_ref()?)
}

/// Build engine CLI extra args from a ModelConfig for the given engine dialect.
pub fn build_extra_args_from_config(
    engine_type: Option<&str>,
    cfg: &ModelConfig,
) -> Option<Vec<String>> {
    build_engine_extra_args_lenient(engine_type, cfg)
}

/// Find the next available port starting from `start`, skipping any in `used`.
pub fn allocate_port(start: u16, used: &HashSet<u16>) -> u16 {
    let mut port = start;
    while used.contains(&port) {
        port = port.saturating_add(1);
    }
    port
}

fn make_assignment(
    replica_id: u32,
    model_uid: &str,
    node_id: String,
    port: u16,
    gpu_indices: Vec<u32>,
    extra_args: Option<Vec<String>>,
    engine_type: Option<String>,
    docker_image: Option<String>,
) -> PlacementAssignment {
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
    PlacementAssignment {
        replica_id,
        node_id,
        engine_config_path: format!("/tmp/nebula/{}.yaml", model_uid),
        port,
        gpu_index,
        gpu_indices: gpu_indices_field,
        extra_args,
        engine_type,
        docker_image,
        pool_id: None,
    }
}

#[allow(dead_code)] // legacy model_requests path removed in B5; kept for tests/tools
pub async fn build_plan_multi(
    store: &EtcdMetaStore,
    req: &ModelRequest,
    default_port: u16,
    mut used_ports: HashSet<u16>,
    mut used_gpus: HashMap<String, HashSet<u32>>,
) -> anyhow::Result<PlacementPlan> {
    let replicas = req.request.replicas.max(1);
    let extra_args = build_extra_args(req);
    let mut assignments = Vec::with_capacity(replicas as usize);

    for replica_id in 0..replicas {
        let (node_id, gpu_indices) = select_node_and_gpus(store, req, &used_gpus).await?;

        let port = allocate_port(default_port, &used_ports);
        used_ports.insert(port);

        // Mark GPUs as used for subsequent replicas
        if !gpu_indices.is_empty() {
            let entry = used_gpus.entry(node_id.clone()).or_default();
            for &idx in &gpu_indices {
                entry.insert(idx);
            }
        }

        assignments.push(make_assignment(
            replica_id,
            &req.request.model_uid,
            node_id,
            port,
            gpu_indices,
            extra_args.clone(),
            req.request.engine_type.clone(),
            req.request.docker_image.clone(),
        ));
    }

    Ok(PlacementPlan {
        request_id: Some(req.id.clone()),
        model_uid: req.request.model_uid.clone(),
        model_name: req.request.model_name.clone(),
        // Placeholder; `write_placement_cas` / reconcile set logical version + updated_at_ms.
        version: 0,
        updated_at_ms: now_ms(),
        leader_epoch: 0,
        assignments,
    })
}

/// Merge two ModelConfig values. `overrides` fields win when present.
pub fn merge_config(
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
            kv_cache_dtype: o.kv_cache_dtype.clone().or_else(|| b.kv_cache_dtype.clone()),
            trust_remote_code: o.trust_remote_code.or(b.trust_remote_code),
            enable_expert_parallel: o.enable_expert_parallel.or(b.enable_expert_parallel),
            block_size: o.block_size.or(b.block_size),
            tokenizer_mode: o.tokenizer_mode.clone().or_else(|| b.tokenizer_mode.clone()),
        }),
    }
}

/// Build a PlacementPlan from a ModelSpec + ModelDeployment (new declarative path).
pub async fn build_plan_from_deployment(
    store: &EtcdMetaStore,
    spec: &ModelSpec,
    deployment: &ModelDeployment,
    default_port: u16,
    mut used_ports: HashSet<u16>,
    mut used_gpus: HashMap<String, HashSet<u32>>,
) -> anyhow::Result<PlacementPlan> {
    let replicas = deployment.replicas.max(1);
    let mut assignments = Vec::with_capacity(replicas as usize);

    for replica_id in 0..replicas {
        let replica_spec = deployment
            .replica_specs
            .as_ref()
            .and_then(|specs| specs.get(replica_id as usize));

        let merged_config = {
            let base = merge_config(spec.config.as_ref(), deployment.config_overrides.as_ref());
            merge_config(
                base.as_ref(),
                replica_spec.and_then(|s| s.config_overrides.as_ref()),
            )
        };
        let extra_args = merged_config
            .as_ref()
            .and_then(|c| build_extra_args_from_config(spec.engine_type.as_deref(), c));

        let node_affinity = replica_spec
            .and_then(|s| s.node_id.as_deref())
            .or(deployment.node_affinity.as_deref());
        let gpu_affinity = replica_spec
            .and_then(|s| s.gpu_indices.as_deref())
            .or(deployment.gpu_affinity.as_deref());

        let image_ref = replica_spec
            .and_then(|s| s.image_id.as_deref())
            .or(deployment.image_id.as_deref())
            .or(spec.docker_image.as_deref());

        let (node_id, gpu_indices) = select_node_and_gpus_for_deployment(
            store,
            &merged_config,
            node_affinity,
            gpu_affinity,
            &used_gpus,
            image_ref,
            deployment.allowed_pools.as_deref(),
        )
        .await?;

        let port = allocate_port(default_port, &used_ports);
        used_ports.insert(port);

        // Mark GPUs as used for subsequent replicas
        if !gpu_indices.is_empty() {
            let entry = used_gpus.entry(node_id.clone()).or_default();
            for &idx in &gpu_indices {
                entry.insert(idx);
            }
        }

        assignments.push(make_assignment(
            replica_id,
            &spec.model_uid,
            node_id,
            port,
            gpu_indices,
            extra_args.clone(),
            spec.engine_type.clone(),
            replica_spec
                .and_then(|s| s.image_id.clone())
                .or_else(|| deployment.image_id.clone())
                .or_else(|| spec.docker_image.clone()),
        ));
    }

    Ok(PlacementPlan {
        request_id: None,
        model_uid: spec.model_uid.clone(),
        model_name: spec.model_name.clone(),
        // Placeholder; writer stamps logical version + updated_at_ms.
        version: 0,
        updated_at_ms: now_ms(),
        leader_epoch: 0,
        assignments,
    })
}

/// Node/GPU selection for deployment path. Respects optional affinity and pool constraints.
async fn select_node_and_gpus_for_deployment(
    store: &EtcdMetaStore,
    config: &Option<ModelConfig>,
    node_affinity: Option<&str>,
    gpu_affinity: Option<&[u32]>,
    used_gpus: &HashMap<String, HashSet<u32>>,
    docker_image: Option<&str>,
    allowed_pools: Option<&[String]>,
) -> anyhow::Result<(String, Vec<u32>)> {
    let image_platforms = load_image_platforms(store, docker_image).await;
    let allowed_nodes_from_pool = load_allowed_pool_nodes(store, allowed_pools).await?;

    // If both node and GPU affinity are specified, use them directly (still check platform & pool).
    if let Some(target_node) = node_affinity {
        if let Some(ref allowed) = allowed_nodes_from_pool {
            if !allowed.contains(target_node) {
                anyhow::bail!(
                    "node_affinity '{target_node}' is not in allowed_pools {:?}",
                    allowed_pools.unwrap_or_default()
                );
            }
        }
        if !image_platforms.is_empty() {
            if let Ok(kvs) = store.list_prefix("/nodes/").await {
                for (_, val, _) in kvs {
                    let Ok(status) = serde_json::from_slice::<NodeStatus>(&val) else {
                        continue;
                    };
                    if status.node_id == target_node
                        && !node_matches_platforms(&status, &image_platforms)
                    {
                        anyhow::bail!(
                            "node '{target_node}' platform incompatible with image platforms {:?}",
                            image_platforms
                        );
                    }
                }
            }
        }
        let indices = gpu_affinity.map(|g| g.to_vec()).unwrap_or_default();
        return Ok((target_node.to_string(), indices));
    }

    let required_vram_mb = config
        .as_ref()
        .and_then(|c| c.required_vram_mb)
        .unwrap_or(0);

    let tp_size = config
        .as_ref()
        .and_then(|c| c.tensor_parallel_size)
        .unwrap_or(1)
        .max(1) as usize;

    let mut nodes: Vec<NodeStatus> = Vec::new();
    if let Ok(kvs) = store.list_prefix("/nodes/").await {
        for (_, val, _) in kvs {
            if let Ok(status) = serde_json::from_slice::<NodeStatus>(&val) {
                nodes.push(status);
            }
        }
    }

    let now = now_ms();
    let mut best_node: Option<(String, Vec<u32>, u64)> = None;
    let mut rejected_platform = 0u32;
    let mut rejected_pool = 0u32;

    for node in &nodes {
        if now.saturating_sub(node.last_heartbeat_ms) > NODE_STALE_MS {
            continue;
        }
        if let Some(ref allowed) = allowed_nodes_from_pool {
            if !allowed.contains(&node.node_id) {
                rejected_pool += 1;
                continue;
            }
        }
        if !node_matches_platforms(node, &image_platforms) {
            rejected_platform += 1;
            continue;
        }

        let used = used_gpus.get(&node.node_id);

        let mut available: Vec<(u32, u64)> = node
            .gpus
            .iter()
            .filter(|gpu| {
                if let Some(used_set) = used {
                    if used_set.contains(&gpu.index) {
                        return false;
                    }
                }
                let free = gpu.memory_total_mb.saturating_sub(gpu.memory_used_mb);
                free >= required_vram_mb
            })
            .map(|gpu| {
                let free = gpu.memory_total_mb.saturating_sub(gpu.memory_used_mb);
                (gpu.index, free)
            })
            .collect();

        available.sort_by(|a, b| b.1.cmp(&a.1));

        if available.len() >= tp_size {
            // If gpu_affinity is set (without node_affinity), check if this node has those GPUs
            if let Some(affinity) = gpu_affinity {
                let avail_indices: HashSet<u32> = available.iter().map(|(idx, _)| *idx).collect();
                if affinity.iter().all(|g| avail_indices.contains(g)) {
                    return Ok((node.node_id.clone(), affinity.to_vec()));
                }
                continue;
            }

            let selected: Vec<u32> = available[..tp_size].iter().map(|(idx, _)| *idx).collect();
            let total_free: u64 = available[..tp_size].iter().map(|(_, free)| free).sum();

            match best_node {
                Some((_, _, best_free)) if total_free <= best_free => {}
                _ => {
                    best_node = Some((node.node_id.clone(), selected, total_free));
                }
            }
        }
    }

    if let Some((node_id, indices, _)) = best_node {
        return Ok((node_id, indices));
    }

    for node in &nodes {
        if now.saturating_sub(node.last_heartbeat_ms) > NODE_STALE_MS {
            continue;
        }
        if let Some(ref allowed) = allowed_nodes_from_pool {
            if !allowed.contains(&node.node_id) {
                continue;
            }
        }
        if !node_matches_platforms(node, &image_platforms) {
            continue;
        }
        return Ok((node.node_id.clone(), vec![]));
    }

    if rejected_pool > 0 && allowed_nodes_from_pool.is_some() {
        anyhow::bail!(
            "no eligible nodes in allowed_pools {:?} (rejected {} nodes by pool constraint)",
            allowed_pools.unwrap_or_default(),
            rejected_pool
        );
    }

    if rejected_platform > 0 && !image_platforms.is_empty() {
        let reason = nebula_common::PlacementRejectReason::platform_incompatible(
            image_platforms,
            rejected_platform,
            None,
        );
        anyhow::bail!("{}", reason.format_error());
    }
    anyhow::bail!(
        "{}",
        nebula_common::PlacementRejectReason::no_healthy_nodes().format_error()
    )
}
