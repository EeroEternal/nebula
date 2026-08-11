//! Postgres-backed API keys for machine integration (I1.5).
//!
//! Env tokens (`NEBULA_AUTH_TOKENS`) remain supported; when `NEBULA_PLATFORM_DB_URL` is set,
//! Bearer tokens are also looked up in `platform_api_keys`.

use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use nebula_common::auth::{
    self, parse_auth_from_env, AuthConfig, AuthContext, RateWindow, Role, TokenBinding,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiKeyScope {
    Inference,
    Control,
    Admin,
}

impl ApiKeyScope {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "inference" => Some(Self::Inference),
            "control" => Some(Self::Control),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    fn allows_path(self, path: &str) -> bool {
        match self {
            ApiKeyScope::Admin => true,
            ApiKeyScope::Control => {
                path.starts_with("/platform/v1") || path.starts_with("/v1/admin")
            }
            ApiKeyScope::Inference => {
                path.starts_with("/v1/")
                    && !path.starts_with("/v1/admin")
                    && !path.starts_with("/platform/")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub principal: String,
    pub role: Role,
    pub scopes: HashSet<ApiKeyScope>,
    pub tenant_id: Option<String>,
}

#[derive(Clone)]
pub struct ApiKeyStore {
    pool: PgPool,
}

impl ApiKeyStore {
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
            CREATE TABLE IF NOT EXISTS platform_api_keys (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                role TEXT NOT NULL CHECK (role IN ('viewer', 'operator', 'admin')),
                scopes TEXT[] NOT NULL DEFAULT '{inference,control}',
                tenant_id TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                revoked_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_platform_api_keys_hash ON platform_api_keys(key_hash) WHERE revoked_at IS NULL",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub fn hash_key(raw: &str) -> String {
        let digest = Sha256::digest(raw.as_bytes());
        hex::encode(digest)
    }

    pub async fn lookup(&self, raw_token: &str) -> Result<Option<ApiKeyRecord>, sqlx::Error> {
        let key_hash = Self::hash_key(raw_token);
        let row = sqlx::query_as::<_, (String, String, Vec<String>, Option<String>)>(
            r#"
            SELECT name, role, scopes, tenant_id
            FROM platform_api_keys
            WHERE key_hash = $1 AND revoked_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(name, role, scopes, tenant_id)| {
            let role = match role.to_ascii_lowercase().as_str() {
                "admin" => Role::Admin,
                "operator" => Role::Operator,
                _ => Role::Viewer,
            };
            let scopes = scopes
                .iter()
                .filter_map(|s| ApiKeyScope::parse(s))
                .collect();
            ApiKeyRecord {
                principal: name,
                role,
                scopes,
                tenant_id,
            }
        }))
    }
}

#[derive(Clone)]
pub struct GatewayAuth {
    pub env: AuthConfig,
    pub api_keys: Option<Arc<ApiKeyStore>>,
}

impl AsRef<AuthConfig> for GatewayAuth {
    fn as_ref(&self) -> &AuthConfig {
        &self.env
    }
}

pub async fn build_gateway_auth() -> GatewayAuth {
    let env = parse_auth_from_env();
    let api_keys = match std::env::var("NEBULA_PLATFORM_DB_URL")
        .or_else(|_| std::env::var("NEBULA_BFF_DATABASE_URL"))
    {
        Ok(url) if !url.trim().is_empty() => match ApiKeyStore::connect(&url).await {
            Ok(store) => {
                tracing::info!("platform API key store enabled (Postgres)");
                Some(Arc::new(store))
            }
            Err(e) => {
                tracing::error!(error=%e, "failed to connect platform API key database");
                None
            }
        },
        _ => None,
    };
    GatewayAuth { env, api_keys }
}

fn extract_bearer(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
}

fn scope_allows(record: &ApiKeyRecord, path: &str) -> bool {
    if record.scopes.contains(&ApiKeyScope::Admin) {
        return true;
    }
    record
        .scopes
        .iter()
        .any(|scope| scope.allows_path(path))
}

pub async fn gateway_auth_middleware(
    State(st): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, std::convert::Infallible> {
    let auth = &st.auth;
    let path = req.uri().path().to_string();

    if !auth.env.enabled {
        let ctx = AuthContext {
            principal: "guest".into(),
            role: Role::Admin,
            tenant_id: None,
        };
        req.extensions_mut().insert(ctx);
        return Ok(next.run(req).await);
    }

    let Some(token) = extract_bearer(&req) else {
        return Ok(auth::unauthorized("missing token"));
    };

    if let Some(binding) = auth.env.tokens.get(&token).cloned() {
        return apply_env_token(auth, req, next, token, binding).await;
    }

    if let Some(store) = &auth.api_keys {
        match store.lookup(&token).await {
            Ok(Some(record)) if scope_allows(&record, &path) => {
                if auth.env.limit_per_minute > 0 {
                    let key = nebula_common::admission::rate_limit_key(
                        record.tenant_id.as_deref(),
                        &token,
                    );
                    let mut guard = auth.env.rate_limits.lock().await;
                    let entry = guard.entry(key).or_insert(RateWindow {
                        window_start: std::time::Instant::now(),
                        count: 0,
                    });
                    let now = std::time::Instant::now();
                    if now.duration_since(entry.window_start) >= std::time::Duration::from_secs(60)
                    {
                        entry.window_start = now;
                        entry.count = 0;
                    }
                    if entry.count >= auth.env.limit_per_minute {
                        return Ok(auth::quota_denied(
                            "tenant_rps_exceeded",
                            "rate limited",
                        ));
                    }
                    entry.count += 1;
                }
                let ctx = AuthContext {
                    principal: record.principal,
                    role: record.role,
                    tenant_id: record.tenant_id,
                };
                req.extensions_mut().insert(ctx);
                return Ok(next.run(req).await);
            }
            Ok(Some(_)) => {
                return Ok(forbidden_scope());
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error=%e, "api key lookup failed");
            }
        }
    }

    Ok(auth::forbidden("invalid token"))
}

async fn apply_env_token(
    auth: &GatewayAuth,
    mut req: Request<Body>,
    next: Next,
    token: String,
    binding: TokenBinding,
) -> Result<Response, std::convert::Infallible> {
    if auth.env.limit_per_minute > 0 {
        let key = nebula_common::admission::rate_limit_key(binding.tenant_id.as_deref(), &token);
        let mut guard = auth.env.rate_limits.lock().await;
        let entry = guard.entry(key).or_insert(RateWindow {
            window_start: std::time::Instant::now(),
            count: 0,
        });
        let now = std::time::Instant::now();
        if now.duration_since(entry.window_start) >= std::time::Duration::from_secs(60) {
            entry.window_start = now;
            entry.count = 0;
        }
        if entry.count >= auth.env.limit_per_minute {
            return Ok(auth::quota_denied("tenant_rps_exceeded", "rate limited"));
        }
        entry.count += 1;
    }

    let ctx = AuthContext {
        principal: token,
        role: binding.role,
        tenant_id: binding.tenant_id,
    };
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

fn forbidden_scope() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": {
                "message": "api key scope does not allow this path",
                "code": "forbidden",
                "type": "authentication_error",
            }
        })),
    )
        .into_response()
}
