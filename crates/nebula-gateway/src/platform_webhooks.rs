//! Operation webhook subscriptions and delivery (I2.5).

use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use nebula_control::{get_operation, Operation, OperationStatus};
use nebula_meta::EtcdMetaStore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub principal: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub secret: String,
}

#[derive(Clone)]
pub struct WebhookStore {
    pool: PgPool,
}

impl WebhookStore {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS platform_webhook_subscriptions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                secret TEXT NOT NULL,
                principal TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                revoked_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_platform_webhooks_principal ON platform_webhook_subscriptions(principal) WHERE revoked_at IS NULL",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn validate_url(url: &str) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err("webhook url must use https://".to_string());
        }
        Ok(())
    }

    pub async fn create(
        &self,
        principal: &str,
        req: CreateWebhookRequest,
    ) -> Result<WebhookSubscription, String> {
        Self::validate_url(&req.url)?;
        if req.secret.len() < 16 {
            return Err("secret must be at least 16 characters".to_string());
        }
        let row = sqlx::query_as::<_, (String, String, String, i64)>(
            r#"
            INSERT INTO platform_webhook_subscriptions (name, url, secret, principal)
            VALUES ($1, $2, $3, $4)
            RETURNING id::text, name, url, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint
            "#,
        )
        .bind(&req.name)
        .bind(&req.url)
        .bind(&req.secret)
        .bind(principal)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(WebhookSubscription {
            id: row.0,
            name: row.1,
            url: row.2,
            principal: principal.to_string(),
            created_at_ms: row.3,
        })
    }

    pub async fn list(&self, principal: &str) -> Result<Vec<WebhookSubscription>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, i64)>(
            r#"
            SELECT id::text, name, url, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint
            FROM platform_webhook_subscriptions
            WHERE principal = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(principal)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, url, created_at_ms)| WebhookSubscription {
                id,
                name,
                url,
                principal: principal.to_string(),
                created_at_ms,
            })
            .collect())
    }

    pub async fn revoke(&self, principal: &str, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE platform_webhook_subscriptions
            SET revoked_at = now()
            WHERE id::text = $1 AND principal = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(id)
        .bind(principal)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_delivery_targets(
        &self,
        principal: &str,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT url, secret
            FROM platform_webhook_subscriptions
            WHERE principal = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(principal)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

fn sign_payload(secret: &str, body: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

async fn post_webhook(
    http: &Client,
    url: &str,
    secret: Option<&str>,
    op: &Operation,
) {
    let body = match serde_json::to_string(op) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, "webhook serialize failed");
            return;
        }
    };
    let mut req = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Nebula-Event", "operation.updated");
    if let Some(secret) = secret {
        req = req.header("X-Nebula-Signature", format!("sha256={}", sign_payload(secret, &body)));
    }
    match req.body(body).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(url=%url, operation_id=%op.operation_id, "webhook delivered");
        }
        Ok(resp) => {
            tracing::warn!(
                url=%url,
                operation_id=%op.operation_id,
                status=%resp.status(),
                "webhook delivery non-success"
            );
        }
        Err(e) => {
            tracing::warn!(url=%url, operation_id=%op.operation_id, error=%e, "webhook delivery failed");
        }
    }
}

async fn deliver_operation_event(
    http: &Client,
    webhook_store: Option<&WebhookStore>,
    principal: &str,
    callback_url: Option<&str>,
    op: &Operation,
) {
    if let Some(url) = callback_url {
        post_webhook(http, url, None, op).await;
    }
    if let Some(store) = webhook_store {
        match store.list_delivery_targets(principal).await {
            Ok(targets) => {
                for (url, secret) in targets {
                    post_webhook(http, &url, Some(&secret), op).await;
                }
            }
            Err(e) => tracing::warn!(error=%e, "webhook subscription lookup failed"),
        }
    }
}

/// Poll operation status and deliver webhooks on transitions until terminal.
pub fn spawn_operation_webhooks(
    http: Client,
    store: Arc<EtcdMetaStore>,
    webhook_store: Option<Arc<WebhookStore>>,
    principal: String,
    operation_id: String,
    callback_url: Option<String>,
) {
    tokio::spawn(async move {
        let mut last_status: Option<OperationStatus> = None;
        loop {
            match get_operation(&*store, &operation_id).await {
                Ok(op) => {
                    if last_status != Some(op.status) {
                        deliver_operation_event(
                            &http,
                            webhook_store.as_deref(),
                            &principal,
                            callback_url.as_deref().or(op.callback_url.as_deref()),
                            &op,
                        )
                        .await;
                        last_status = Some(op.status);
                    }
                    if matches!(
                        op.status,
                        OperationStatus::Succeeded | OperationStatus::Failed
                    ) {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(operation_id=%operation_id, error=%e, "webhook watcher stopped");
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_payload_is_deterministic() {
        let a = sign_payload("secret-key-12345678", r#"{"operation_id":"op_1"}"#);
        let b = sign_payload("secret-key-12345678", r#"{"operation_id":"op_1"}"#);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::json;
use crate::auth::{require_role, AuthContext, Role};
use crate::control::control_error;
use crate::state::AppState;

pub async fn platform_list_webhooks(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let Some(store) = &st.auth.webhooks else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": {
                    "message": "webhook store unavailable (set NEBULA_PLATFORM_DB_URL)",
                    "code": "service_unavailable",
                    "type": "configuration_error",
                }
            })),
        )
            .into_response();
    };
    match store.list(&ctx.principal).await {
        Ok(items) => (StatusCode::OK, Json(json!({ "webhooks": items }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "message": e.to_string() } })),
        )
            .into_response(),
    }
}

pub async fn platform_create_webhook(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateWebhookRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let Some(store) = &st.auth.webhooks else {
        return service_unavailable();
    };
    match store.create(&ctx.principal, req).await {
        Ok(sub) => (StatusCode::CREATED, Json(sub)).into_response(),
        Err(e) => control_error(nebula_control::ServiceError::BadRequest(e)),
    }
}

pub async fn platform_delete_webhook(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(webhook_id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let Some(store) = &st.auth.webhooks else {
        return service_unavailable();
    };
    match store.revoke(&ctx.principal, &webhook_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => control_error(nebula_control::ServiceError::NotFound(format!(
            "webhook '{webhook_id}' not found"
        ))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "message": e.to_string() } })),
        )
            .into_response(),
    }
}

fn service_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "message": "webhook store unavailable (set NEBULA_PLATFORM_DB_URL)",
                "code": "service_unavailable",
                "type": "configuration_error",
            }
        })),
    )
        .into_response()
}
