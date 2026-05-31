use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use tokio::sync::Mutex;

// ── Role ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    pub fn allows(self, required: Role) -> bool {
        matches!(
            (self, required),
            (Role::Admin, _)
                | (Role::Operator, Role::Viewer | Role::Operator)
                | (Role::Viewer, Role::Viewer)
        )
    }
}

// ── AuthContext ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub principal: String,
    pub role: Role,
}

// ── AuthConfig (was AuthState in gateway) ───────────────────────────

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub tokens: Arc<HashMap<String, Role>>,
    pub rate_limits: Arc<Mutex<HashMap<String, RateWindow>>>,
    pub limit_per_minute: u64,
}

#[derive(Debug, Clone)]
pub struct RateWindow {
    pub window_start: Instant,
    pub count: u64,
}

// ── Environment parsing ─────────────────────────────────────────────

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn parse_auth_from_env() -> AuthConfig {
    let tokens_raw = std::env::var("NEBULA_AUTH_TOKENS").ok();
    let disabled_for_dev =
        env_flag_enabled("NEBULA_AUTH_DISABLED") || env_flag_enabled("NEBULA_DEV_AUTH_DISABLED");
    let enabled = !disabled_for_dev;

    let mut tokens = HashMap::new();
    if let Some(raw) = tokens_raw {
        for entry in raw.split(',') {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some((token, role_raw)) = trimmed.split_once(':') else {
                tracing::warn!(entry=%trimmed, "invalid NEBULA_AUTH_TOKENS entry, expected token:role");
                continue;
            };
            let role = match role_raw.to_ascii_lowercase().as_str() {
                "admin" => Role::Admin,
                "operator" => Role::Operator,
                "viewer" => Role::Viewer,
                other => {
                    tracing::warn!(role=%other, "unknown role in NEBULA_AUTH_TOKENS, skipping");
                    continue;
                }
            };
            tokens.insert(token.to_string(), role);
        }
    }

    let limit_per_minute = std::env::var("NEBULA_AUTH_RATE_LIMIT_PER_MINUTE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120);

    if disabled_for_dev {
        tracing::warn!(
            auth_mode = "disabled_for_dev",
            "auth disabled by explicit environment flag; do not use this in production"
        );
    } else if tokens.is_empty() {
        tracing::error!(
            auth_mode = "enabled",
            "auth enabled but NEBULA_AUTH_TOKENS has no valid entries; protected routes will reject requests"
        );
    } else {
        tracing::info!(
            auth_mode = "enabled",
            token_count = tokens.len(),
            "auth enabled"
        );
    }

    AuthConfig {
        enabled,
        tokens: Arc::new(tokens),
        rate_limits: Arc::new(Mutex::new(HashMap::new())),
        limit_per_minute,
    }
}

// ── Middleware ───────────────────────────────────────────────────────
// Generic over any state type S that implements AsRef<AuthConfig>.
// Usage: `middleware::from_fn_with_state(app_state, auth_middleware::<MyAppState>)`

pub async fn auth_middleware<S>(
    State(state): State<S>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, std::convert::Infallible>
where
    S: AsRef<AuthConfig> + Clone + Send + Sync + 'static,
{
    let auth = state.as_ref();

    if !auth.enabled {
        let ctx = AuthContext {
            principal: "guest".into(),
            role: Role::Admin,
        };
        req.extensions_mut().insert(ctx);
        return Ok(next.run(req).await);
    }

    let token = extract_token(&req);

    let Some(token) = token else {
        return Ok(unauthorized("missing token"));
    };

    let Some(role) = auth.tokens.get(&token).copied() else {
        return Ok(forbidden("invalid token"));
    };

    if auth.limit_per_minute > 0 {
        let mut guard = auth.rate_limits.lock().await;
        let entry = guard.entry(token.clone()).or_insert(RateWindow {
            window_start: Instant::now(),
            count: 0,
        });
        let now = Instant::now();
        if now.duration_since(entry.window_start) >= std::time::Duration::from_secs(60) {
            entry.window_start = now;
            entry.count = 0;
        }
        if entry.count >= auth.limit_per_minute {
            return Ok(too_many_requests());
        }
        entry.count += 1;
    }

    let ctx = AuthContext {
        principal: token,
        role,
    };
    req.extensions_mut().insert(ctx);

    Ok(next.run(req).await)
}

fn extract_token(req: &Request<Body>) -> Option<String> {
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

// ── Role check ──────────────────────────────────────────────────────
// Returns None when the caller has sufficient permissions, or
// Some(403 response) when forbidden.  Does NOT depend on Metrics.

pub fn require_role(ctx: &AuthContext, required: Role) -> Option<Response> {
    if ctx.role.allows(required) {
        None
    } else {
        Some(forbidden("insufficient permissions"))
    }
}

// ── Error helpers ───────────────────────────────────────────────────

pub fn unauthorized(msg: &str) -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": {"message": msg}})),
    )
        .into_response()
}

pub fn forbidden(msg: &str) -> Response {
    (
        axum::http::StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": {"message": msg}})),
    )
        .into_response()
}

pub fn too_many_requests() -> Response {
    (
        axum::http::StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({"error": {"message": "rate limited"}})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_auth_env() {
        std::env::remove_var("NEBULA_AUTH_TOKENS");
        std::env::remove_var("NEBULA_AUTH_DISABLED");
        std::env::remove_var("NEBULA_DEV_AUTH_DISABLED");
        std::env::remove_var("NEBULA_AUTH_RATE_LIMIT_PER_MINUTE");
    }

    #[test]
    fn auth_defaults_to_enabled_without_tokens() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();

        let auth = parse_auth_from_env();

        assert!(auth.enabled);
        assert!(auth.tokens.is_empty());
    }

    #[test]
    fn explicit_disable_turns_auth_off_for_dev() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        std::env::set_var("NEBULA_AUTH_DISABLED", "true");

        let auth = parse_auth_from_env();

        assert!(!auth.enabled);
    }

    #[test]
    fn parses_valid_tokens_when_enabled() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        std::env::set_var("NEBULA_AUTH_TOKENS", "admin-token:admin,view-token:viewer");

        let auth = parse_auth_from_env();

        assert!(auth.enabled);
        assert_eq!(auth.tokens.get("admin-token"), Some(&Role::Admin));
        assert_eq!(auth.tokens.get("view-token"), Some(&Role::Viewer));
    }
}
