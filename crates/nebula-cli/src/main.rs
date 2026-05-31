mod args;
mod chat;
mod client;
mod config;
mod output;

use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use reqwest::Client;

use nebula_common::{ClusterStatus, ModelLoadRequest};

use crate::args::{
    AdminCommand, Args, CacheCommand, ClusterCommand, Command, DiskCommand, ModelCommand,
    TemplateCommand,
};
use crate::client::auth;
use crate::config::build_config;
use crate::output::{
    print_cache_summary, print_cluster_status, print_disk_status, print_model_detail_v2,
    print_models_v2, print_node_cache, print_templates,
};

/// Build a v2 API URL from the gateway base URL.
fn v2_url(gateway_url: &str, path: &str) -> String {
    format!("{}/v1/admin/v2{}", gateway_url.trim_end_matches('/'), path)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();
    let token = args.token;

    match args.command {
        Command::Cluster { subcommand } => match subcommand {
            ClusterCommand::Status => {
                let url = format!(
                    "{}/v1/admin/cluster/status",
                    args.gateway_url.trim_end_matches('/')
                );
                let status: ClusterStatus = auth(client.get(&url), token.as_ref())
                    .send()
                    .await?
                    .json()
                    .await?;
                print_cluster_status(status);
            }
        },
        Command::Model { subcommand } => match subcommand {
            ModelCommand::List => {
                let url = v2_url(&args.gateway_url, "/models");
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    let models: Vec<serde_json::Value> = resp.json().await?;
                    print_models_v2(&models);
                } else {
                    eprintln!("✗ Failed to list models: {}", resp.text().await?);
                }
            }
            ModelCommand::Get { model_uid } => {
                let url = v2_url(&args.gateway_url, &format!("/models/{}", model_uid));
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    let model: serde_json::Value = resp.json().await?;
                    print_model_detail_v2(&model);
                } else {
                    eprintln!("✗ Failed to get model: {}", resp.text().await?);
                }
            }
            ModelCommand::Create {
                name,
                uid,
                engine,
                source,
                start,
                replicas,
            } => {
                let url = v2_url(&args.gateway_url, "/models");
                let mut body = serde_json::json!({
                    "model_name": name,
                    "engine_type": engine,
                    "model_source": source,
                    "auto_start": start,
                });
                if let Some(u) = &uid {
                    body["model_uid"] = serde_json::json!(u);
                }
                if start {
                    body["replicas"] = serde_json::json!(replicas);
                }
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!("✓ Model created: '{}'", name);
                    println!("{}", resp.text().await?);
                } else {
                    eprintln!("✗ Failed to create model: {}", resp.text().await?);
                }
            }
            ModelCommand::Start {
                model_uid,
                replicas,
            } => {
                let url = v2_url(&args.gateway_url, &format!("/models/{}/start", model_uid));
                let body = serde_json::json!({ "replicas": replicas });
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!(
                        "✓ Model '{}' starting with {} replicas",
                        model_uid, replicas
                    );
                } else {
                    eprintln!("✗ Failed to start model: {}", resp.text().await?);
                }
            }
            ModelCommand::Stop { model_uid } => {
                let url = v2_url(&args.gateway_url, &format!("/models/{}/stop", model_uid));
                let resp = auth(client.post(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    println!("✓ Model '{}' stopped", model_uid);
                } else {
                    eprintln!("✗ Failed to stop model: {}", resp.text().await?);
                }
            }
            ModelCommand::Delete { model_uid } => {
                let url = v2_url(&args.gateway_url, &format!("/models/{}", model_uid));
                let resp = auth(client.delete(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    println!("✓ Model '{}' deleted", model_uid);
                } else {
                    eprintln!("✗ Failed to delete model: {}", resp.text().await?);
                }
            }
            ModelCommand::ScaleModel {
                model_uid,
                replicas,
            } => {
                let url = v2_url(&args.gateway_url, &format!("/models/{}/scale", model_uid));
                let body = serde_json::json!({ "replicas": replicas });
                let resp = auth(client.put(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!("✓ Model '{}' scaled to {} replicas", model_uid, replicas);
                } else {
                    eprintln!("✗ Failed to scale model: {}", resp.text().await?);
                }
            }
            // Legacy v1 commands
            ModelCommand::Load {
                name,
                uid,
                replicas,
                required_vram_mb,
                tensor_parallel_size,
                gpu_memory_utilization,
                max_model_len,
                lora,
            } => {
                let url = format!(
                    "{}/v1/admin/models/load",
                    args.gateway_url.trim_end_matches('/')
                );
                let config = build_config(
                    tensor_parallel_size,
                    gpu_memory_utilization,
                    max_model_len,
                    required_vram_mb,
                    lora,
                );
                let req = ModelLoadRequest {
                    model_name: name.clone(),
                    model_uid: uid,
                    replicas,
                    config,
                    node_id: None,
                    gpu_index: None,
                    gpu_indices: None,
                    min_replicas: None,
                    max_replicas: None,
                    engine_type: None,
                    docker_image: None,
                };
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&req)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!("✓ Model load request submitted for '{}'", name);
                    println!("{}", resp.text().await?);
                } else {
                    eprintln!("✗ Failed to load model: {}", resp.text().await?);
                }
            }
            ModelCommand::Unload { id } => {
                let url = format!(
                    "{}/v1/admin/models/requests/{}",
                    args.gateway_url.trim_end_matches('/'),
                    id
                );
                let resp = auth(client.delete(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    println!("✓ Model unload request submitted for ID: {}", id);
                } else {
                    eprintln!("✗ Failed to unload model: {}", resp.text().await?);
                }
            }
        },
        Command::Template { subcommand } => match subcommand {
            TemplateCommand::List => {
                let url = v2_url(&args.gateway_url, "/templates");
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    let templates: Vec<serde_json::Value> = resp.json().await?;
                    print_templates(&templates);
                } else {
                    eprintln!("✗ Failed to list templates: {}", resp.text().await?);
                }
            }
            TemplateCommand::Create {
                name,
                model_name,
                engine,
                source,
            } => {
                let url = v2_url(&args.gateway_url, "/templates");
                let body = serde_json::json!({
                    "name": name,
                    "model_name": model_name,
                    "engine_type": engine,
                    "model_source": source,
                });
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!("✓ Template '{}' created", name);
                    println!("{}", resp.text().await?);
                } else {
                    eprintln!("✗ Failed to create template: {}", resp.text().await?);
                }
            }
            TemplateCommand::Deploy {
                template_id,
                uid,
                replicas,
            } => {
                let url = v2_url(
                    &args.gateway_url,
                    &format!("/templates/{}/deploy", template_id),
                );
                let mut body = serde_json::json!({ "replicas": replicas });
                if let Some(u) = &uid {
                    body["model_uid"] = serde_json::json!(u);
                }
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!(
                        "✓ Template '{}' deployed with {} replicas",
                        template_id, replicas
                    );
                    println!("{}", resp.text().await?);
                } else {
                    eprintln!("✗ Failed to deploy template: {}", resp.text().await?);
                }
            }
            TemplateCommand::Save { model_uid, name } => {
                let url = v2_url(
                    &args.gateway_url,
                    &format!("/models/{}/save-as-template", model_uid),
                );
                let body = serde_json::json!({ "name": name });
                let resp = auth(client.post(&url), token.as_ref())
                    .json(&body)
                    .send()
                    .await?;
                if resp.status().is_success() {
                    println!("✓ Model '{}' saved as template '{}'", model_uid, name);
                    println!("{}", resp.text().await?);
                } else {
                    eprintln!("✗ Failed to save as template: {}", resp.text().await?);
                }
            }
        },
        Command::Cache { subcommand } => match subcommand {
            CacheCommand::List { node } => {
                if let Some(node_id) = node {
                    let url = v2_url(&args.gateway_url, &format!("/nodes/{}/cache", node_id));
                    let resp = auth(client.get(&url), token.as_ref()).send().await?;
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        print_node_cache(&data);
                    } else {
                        eprintln!("✗ Failed to get node cache: {}", resp.text().await?);
                    }
                } else {
                    let url = v2_url(&args.gateway_url, "/cache/summary");
                    let resp = auth(client.get(&url), token.as_ref()).send().await?;
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        print_cache_summary(&data);
                    } else {
                        eprintln!("✗ Failed to get cache summary: {}", resp.text().await?);
                    }
                }
            }
        },
        Command::Disk { subcommand } => match subcommand {
            DiskCommand::Status => {
                let url = v2_url(&args.gateway_url, "/alerts");
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    let data: serde_json::Value = resp.json().await?;
                    print_disk_status(&data);
                } else {
                    eprintln!("✗ Failed to get disk status: {}", resp.text().await?);
                }
            }
        },
        Command::Whoami => {
            let url = format!("{}/v1/admin/whoami", args.gateway_url.trim_end_matches('/'));
            let resp = auth(client.get(&url), token.as_ref()).send().await?;
            println!("{}", resp.text().await?);
        }
        Command::Metrics => {
            let url = format!(
                "{}/v1/admin/metrics",
                args.gateway_url.trim_end_matches('/')
            );
            let resp = auth(client.get(&url), token.as_ref()).send().await?;
            println!("{}", resp.text().await?);
        }
        Command::Logs { lines, follow } => {
            if follow {
                let url = format!(
                    "{}/v1/admin/logs/stream",
                    args.gateway_url.trim_end_matches('/')
                );
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                if !resp.status().is_success() {
                    eprintln!("✗ Failed to stream logs: {}", resp.status());
                    std::process::exit(1);
                }
                let mut stream = resp.bytes_stream();
                let mut buf = String::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    let s = String::from_utf8_lossy(&chunk);
                    buf.push_str(&s);

                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim().to_string();
                        buf.drain(..=pos);

                        if line.is_empty() {
                            continue;
                        }
                        if let Some(data) = line.strip_prefix("data:") {
                            let data = data.trim();
                            if !data.is_empty() {
                                println!("{}", data);
                            }
                        }
                    }
                }
            } else {
                let mut url = format!("{}/v1/admin/logs", args.gateway_url.trim_end_matches('/'));
                if let Some(n) = lines {
                    url = format!("{}?lines={}", url, n);
                }
                let resp = auth(client.get(&url), token.as_ref()).send().await?;
                println!("{}", resp.text().await?);
            }
        }
        Command::Chat {
            model,
            system,
            message,
            max_tokens,
        } => {
            let base = args.gateway_url.trim_end_matches('/').to_string();
            chat::run_chat(
                &client,
                &base,
                token.as_ref(),
                model,
                system,
                message,
                max_tokens,
            )
            .await?;
        }
        Command::Scale { id, replicas } => {
            let url = format!(
                "{}/v1/admin/models/requests/{}/scale",
                args.gateway_url.trim_end_matches('/'),
                id
            );
            let body = serde_json::json!({ "replicas": replicas });
            let resp = auth(client.put(&url), token.as_ref())
                .json(&body)
                .send()
                .await?;
            if resp.status().is_success() {
                println!("✓ Scaled request '{}' to {} replicas", id, replicas);
            } else {
                eprintln!("✗ Failed to scale: {}", resp.text().await?);
            }
        }
        Command::Drain {
            model_uid,
            replica_id,
        } => {
            let url = format!(
                "{}/v1/admin/endpoints/drain",
                args.gateway_url.trim_end_matches('/')
            );
            let body = serde_json::json!({
                "model_uid": model_uid,
                "replica_id": replica_id
            });
            let resp = auth(client.post(&url), token.as_ref())
                .json(&body)
                .send()
                .await?;
            if resp.status().is_success() {
                println!("✓ Draining endpoint {}/replica-{}", model_uid, replica_id);
            } else {
                eprintln!("✗ Failed to drain: {}", resp.text().await?);
            }
        }
        Command::Admin { subcommand } => match subcommand {
            AdminCommand::Migrate => {
                let url = v2_url(&args.gateway_url, "/migrate");
                let resp = auth(client.post(&url), token.as_ref()).send().await?;
                if resp.status().is_success() {
                    let result: serde_json::Value = resp.json().await?;
                    let migrated = result["migrated"].as_u64().unwrap_or(0);
                    let skipped = result["skipped"].as_u64().unwrap_or(0);
                    let failed = result["failed"].as_u64().unwrap_or(0);
                    println!(
                        "Migration complete: {} migrated, {} skipped, {} failed",
                        migrated, skipped, failed
                    );
                    if let Some(details) = result["details"].as_array() {
                        for d in details {
                            let uid = d["model_uid"].as_str().unwrap_or("?");
                            let action = d["action"].as_str().unwrap_or("?");
                            match action {
                                "migrated" => {
                                    let ds = d["desired_state"].as_str().unwrap_or("?");
                                    println!("  ✓ {} → {}", uid, ds);
                                }
                                "skipped" => {
                                    let reason = d["reason"].as_str().unwrap_or("unknown");
                                    println!("  ○ {} → skipped ({})", uid, reason);
                                }
                                "failed" => {
                                    let reason = d["reason"].as_str().unwrap_or("unknown");
                                    println!("  ✗ {} → failed ({})", uid, reason);
                                }
                                _ => {
                                    println!("  ? {} → {}", uid, action);
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("✗ Migration failed: {}", resp.text().await?);
                }
            }
        },
    }

    Ok(())
}
