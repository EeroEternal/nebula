use std::convert::Infallible;

use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

pub use nebula_common::auth::{forbidden, require_role, unauthorized, AuthContext, Role};

pub fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
    }
}

pub fn role_from_str(role: &str) -> Option<Role> {
    match role.to_ascii_lowercase().as_str() {
        "admin" => Some(Role::Admin),
        "operator" => Some(Role::Operator),
        "viewer" => Some(Role::Viewer),
        _ => None,
    }
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password_hash: &str, password: &str) -> bool {
    let parsed = match PasswordHash::new(password_hash) {
        Ok(v) => v,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub async fn initialize_auth_schema(state: &AppState) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS bff_users (
            id UUID PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            display_name TEXT,
            email TEXT,
            is_active BOOLEAN NOT NULL DEFAULT TRUE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&state.db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS bff_user_settings (
            user_id UUID PRIMARY KEY REFERENCES bff_users(id) ON DELETE CASCADE,
            in_app_alerts BOOLEAN NOT NULL DEFAULT TRUE,
            email_alerts BOOLEAN NOT NULL DEFAULT FALSE,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&state.db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS bff_sessions (
            token TEXT PRIMARY KEY,
            user_id UUID NOT NULL REFERENCES bff_users(id) ON DELETE CASCADE,
            expires_at TIMESTAMPTZ NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&state.db)
    .await?;

    let existing_admin = sqlx::query("SELECT id FROM bff_users WHERE username = $1 LIMIT 1")
        .bind("admin")
        .fetch_optional(&state.db)
        .await?;

    if existing_admin.is_none() {
        let admin_id = Uuid::new_v4();
        let password_hash = hash_password("admin123")?;
        sqlx::query(
            r#"
            INSERT INTO bff_users (id, username, password_hash, role, display_name, email, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, TRUE)
            "#,
        )
        .bind(admin_id)
        .bind("admin")
        .bind(password_hash)
        .bind("admin")
        .bind("Administrator")
        .bind(Option::<String>::None)
        .execute(&state.db)
        .await?;

        sqlx::query(
            "INSERT INTO bff_user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(admin_id)
        .execute(&state.db)
        .await?;
    }

    Ok(())
}

pub fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn db_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Infallible> {
    let token = match extract_bearer_token(&req) {
        Some(v) => v,
        None => return Ok(unauthorized("missing token")),
    };

    let row = match sqlx::query(
        r#"
        SELECT u.username, u.role
        FROM bff_sessions s
        JOIN bff_users u ON u.id = s.user_id
        WHERE s.token = $1
          AND s.expires_at > NOW()
          AND u.is_active = TRUE
        LIMIT 1
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error=%e, "db auth query failed");
            return Ok(forbidden("authentication backend unavailable"));
        }
    };

    let Some(row) = row else {
        return Ok(unauthorized("invalid or expired token"));
    };

    let username: String = row.get("username");
    let role_raw: String = row.get("role");
    let Some(role) = role_from_str(&role_raw) else {
        return Ok(forbidden("invalid role"));
    };

    req.extensions_mut().insert(AuthContext {
        principal: username,
        role,
    });

    Ok(next.run(req).await)
}

pub fn new_session_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn session_expiry(hours: i64) -> DateTime<Utc> {
    Utc::now() + chrono::Duration::hours(hours.max(1))
}
