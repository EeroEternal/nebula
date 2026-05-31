use nebula_common::{ClusterStatus, EndpointStatus, ModelRequest};
use serde_json::Value;

pub fn print_cluster_status(status: ClusterStatus) {
    println!("\n=== Nebula Cluster Status ===");

    println!("\n[Nodes]");
    if status.nodes.is_empty() {
        println!("  (No nodes registered)");
    } else {
        println!("  {:<20} {:<20}", "Node ID", "Last Heartbeat");
        for node in status.nodes {
            println!(
                "  {:<20} {:>10}ms ago",
                node.node_id,
                last_seen(node.last_heartbeat_ms)
            );
        }
    }

    println!("\n[Endpoints]");
    if status.endpoints.is_empty() {
        println!("  (No active endpoints)");
    } else {
        println!(
            "  {:<20} {:<10} {:<10} {:<30}",
            "Model UID", "Replica", "Status", "Base URL"
        );
        for ep in status.endpoints {
            let status_str = match ep.status {
                EndpointStatus::Ready => "READY",
                EndpointStatus::Starting => "STARTING",
                EndpointStatus::Unhealthy => "UNHEALTHY",
                EndpointStatus::Draining => "DRAINING",
            };
            println!(
                "  {:<20} {:<10} {:<10} {:<30}",
                ep.model_uid,
                ep.replica_id,
                status_str,
                ep.base_url.as_deref().unwrap_or("N/A")
            );
        }
    }
    println!();
}

#[allow(dead_code)]
pub fn print_model_requests(requests: Vec<ModelRequest>) {
    println!("\n=== Nebula Model Requests ===");
    if requests.is_empty() {
        println!("No active or past model requests found.");
        return;
    }

    println!(
        "{:<40} {:<25} {:<15} {:<10}",
        "Request ID", "Model Name", "Status", "Replicas"
    );
    println!("{:-<100}", "");
    for req in requests {
        let status_str = format!("{:?}", req.status);
        println!(
            "{:<40} {:<25} {:<15} {:<10}",
            req.id, req.request.model_name, status_str, req.request.replicas
        );
    }
    println!();
}

// ── v2 output functions ──────────────────────────────────────────────

pub fn print_models_v2(models: &[Value]) {
    println!("\n=== Nebula Models ===\n");
    if models.is_empty() {
        println!("No models found.");
        return;
    }
    println!(
        "{:<25} {:<35} {:<12} {:<10} {:<10}",
        "UID", "Name", "State", "Engine", "Replicas"
    );
    println!("{:-<95}", "");
    for m in models {
        let uid = m["model_uid"].as_str().unwrap_or("");
        let name = m["model_name"].as_str().unwrap_or("");
        let state = m["state"].as_str().unwrap_or("unknown");
        let engine = m["engine_type"].as_str().unwrap_or("vllm");
        let ready = m["replicas"]["ready"].as_u64().unwrap_or(0);
        let desired = m["replicas"]["desired"].as_u64().unwrap_or(0);
        println!(
            "{:<25} {:<35} {:<12} {:<10} {}/{}",
            uid, name, state, engine, ready, desired
        );
    }
    println!();
}

pub fn print_model_detail_v2(model: &Value) {
    println!("\n=== Model Detail ===\n");
    println!("  UID:      {}", model["model_uid"].as_str().unwrap_or(""));
    println!("  Name:     {}", model["model_name"].as_str().unwrap_or(""));
    println!(
        "  State:    {}",
        model["state"].as_str().unwrap_or("unknown")
    );
    println!(
        "  Engine:   {}",
        model["engine_type"].as_str().unwrap_or("vllm")
    );
    println!(
        "  Source:   {}",
        model["source"].as_str().unwrap_or("unknown")
    );

    if let Some(replicas) = model.get("replicas") {
        println!(
            "  Replicas: {}/{} ready",
            replicas["ready"].as_u64().unwrap_or(0),
            replicas["desired"].as_u64().unwrap_or(0)
        );
    }

    if let Some(cache) = model.get("cache") {
        println!("  Cached:   {}", cache["cached"].as_bool().unwrap_or(false));
        if let Some(pct) = cache.get("progress_percent") {
            println!("  Cache %:  {}%", pct.as_f64().unwrap_or(0.0));
        }
    }

    if let Some(endpoints) = model.get("endpoints").and_then(|e| e.as_array()) {
        if !endpoints.is_empty() {
            println!("\n  [Endpoints]");
            println!("  {:<10} {:<12} {:<30}", "Replica", "Status", "URL");
            for ep in endpoints {
                println!(
                    "  {:<10} {:<12} {:<30}",
                    ep["replica_id"].as_u64().unwrap_or(0),
                    ep["status"].as_str().unwrap_or("unknown"),
                    ep["base_url"].as_str().unwrap_or("N/A"),
                );
            }
        }
    }
    println!();
}

pub fn print_templates(templates: &[Value]) {
    println!("\n=== Nebula Templates ===\n");
    if templates.is_empty() {
        println!("No templates found.");
        return;
    }
    println!(
        "{:<20} {:<25} {:<35} {:<10}",
        "ID", "Name", "Model", "Engine"
    );
    println!("{:-<95}", "");
    for t in templates {
        let id = t["id"].as_str().unwrap_or("");
        let name = t["name"].as_str().unwrap_or("");
        let model = t["model_name"].as_str().unwrap_or("");
        let engine = t["engine_type"].as_str().unwrap_or("vllm");
        println!("{:<20} {:<25} {:<35} {:<10}", id, name, model, engine);
    }
    println!();
}

pub fn print_cache_summary(data: &Value) {
    println!("\n=== Cache Summary ===\n");
    if let Some(nodes) = data.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            let node_id = node["node_id"].as_str().unwrap_or("unknown");
            println!("  Node: {}", node_id);
            if let Some(models) = node.get("cached_models").and_then(|m| m.as_array()) {
                if models.is_empty() {
                    println!("    (no cached models)");
                } else {
                    println!("    {:<35} {:<15}", "Model", "Size");
                    for m in models {
                        let name = m["model_name"].as_str().unwrap_or("");
                        let size = m["size_bytes"]
                            .as_u64()
                            .map(|b| format_bytes(b))
                            .unwrap_or_else(|| "N/A".to_string());
                        println!("    {:<35} {:<15}", name, size);
                    }
                }
            }
            println!();
        }
    } else {
        println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
    }
}

pub fn print_node_cache(data: &Value) {
    println!("\n=== Node Cache ===\n");
    if let Some(models) = data.get("cached_models").and_then(|m| m.as_array()) {
        if models.is_empty() {
            println!("  (no cached models)");
        } else {
            println!("  {:<35} {:<15}", "Model", "Size");
            println!("  {:-<55}", "");
            for m in models {
                let name = m["model_name"].as_str().unwrap_or("");
                let size = m["size_bytes"]
                    .as_u64()
                    .map(|b| format_bytes(b))
                    .unwrap_or_else(|| "N/A".to_string());
                println!("  {:<35} {:<15}", name, size);
            }
        }
    } else {
        println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
    }
    println!();
}

pub fn print_disk_status(data: &Value) {
    println!("\n=== Disk Status ===\n");
    println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
    println!();
}

fn last_seen(ts_ms: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now.saturating_sub(ts_ms)
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}
