use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::state::AppState;

/// A single audit log entry capturing who/when/what/result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub principal: String,
    pub role: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub latency_ms: u64,
}

/// Background writer that batches audit entries and sends them to xtrace.
pub struct AuditWriter {
    tx: mpsc::Sender<AuditEntry>,
}

impl AuditWriter {
    /// Spawn a background task that drains audit entries and writes them to xtrace.
    /// Returns `None` if xtrace is not configured.
    pub fn spawn(xtrace_url: Option<&str>, xtrace_token: Option<&str>) -> Option<Arc<Self>> {
        let (url, token) = match (xtrace_url, xtrace_token) {
            (Some(u), Some(t)) if !u.is_empty() && !t.is_empty() => (u.to_string(), t.to_string()),
            _ => return None,
        };

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;

        let (tx, rx) = mpsc::channel::<AuditEntry>(4096);

        tokio::spawn(audit_worker(http, url, token, rx));

        tracing::info!("audit logger enabled, writing to xtrace");
        Some(Arc::new(Self { tx }))
    }

    pub fn send(&self, entry: AuditEntry) {
        // Fire-and-forget; drop if channel is full.
        let _ = self.tx.try_send(entry);
    }
}

async fn audit_worker(
    http: reqwest::Client,
    base_url: String,
    token: String,
    mut rx: mpsc::Receiver<AuditEntry>,
) {
    let ingest_url = format!("{}/v1/l/batch", base_url.trim_end_matches('/'));
    let mut buf: Vec<AuditEntry> = Vec::with_capacity(64);

    loop {
        // Wait for the first entry.
        let entry = match rx.recv().await {
            Some(e) => e,
            None => break, // channel closed
        };
        buf.push(entry);

        // Drain any additional entries already in the channel (up to 64).
        while buf.len() < 64 {
            match rx.try_recv() {
                Ok(e) => buf.push(e),
                Err(_) => break,
            }
        }

        // Send each entry to xtrace as a trace via batch ingest.
        for entry in buf.drain(..) {
            let body = serde_json::json!({
                "trace": {
                    "id": entry.id,
                    "timestamp": entry.timestamp.to_rfc3339(),
                    "name": format!("{} {}", entry.method, entry.path),
                    "input": { "method": entry.method, "path": entry.path },
                    "output": { "status": entry.status },
                    "userId": entry.principal,
                    "metadata": {
                        "role": entry.role,
                        "latency_ms": entry.latency_ms,
                        "status": entry.status,
                    },
                    "tags": ["audit", format!("role:{}", entry.role)],
                    "environment": "production",
                    "latency": entry.latency_ms as f64 / 1000.0,
                },
                "observations": [],
            });

            let res = http
                .post(&ingest_url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await;

            if let Err(e) = res {
                tracing::warn!(error=%e, "failed to send audit log to xtrace");
            }
        }
    }
}

/// Axum middleware that records audit log entries after each request.
pub async fn audit_middleware(
    State(st): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let writer = match &st.audit {
        Some(w) => w.clone(),
        None => return next.run(req).await,
    };

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let ctx = req.extensions().get::<AuthContext>().cloned();

    let start = Instant::now();
    let resp = next.run(req).await;
    let elapsed = start.elapsed();

    let (principal, role) = match ctx {
        Some(c) => {
            let role_str = match c.role {
                crate::auth::Role::Admin => "admin",
                crate::auth::Role::Operator => "operator",
                crate::auth::Role::Viewer => "viewer",
            };
            (c.principal, role_str.to_string())
        }
        None => ("anonymous".to_string(), "none".to_string()),
    };

    let entry = AuditEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        principal,
        role,
        method,
        path,
        status: resp.status().as_u16(),
        latency_ms: elapsed.as_millis() as u64,
    };

    writer.send(entry);

    resp
}

/// Query parameters for the audit log listing endpoint.
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub user_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}
