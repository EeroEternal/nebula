use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::{
    new_session_token, require_role, role_from_str, role_to_str, session_expiry, verify_password,
    AuthContext, Role,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SessionUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user: SessionUser,
}

#[derive(Serialize)]
pub struct UserSettingsResponse {
    pub in_app_alerts: bool,
    pub email_alerts: bool,
}

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub in_app_alerts: Option<bool>,
    pub email_alerts: Option<bool>,
}

#[derive(Serialize)]
pub struct UserItem {
    pub id: String,
    pub username: String,
    pub role: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_active: bool,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub async fn login(State(st): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    let username = req.username.trim();
    if username.is_empty() || req.password.is_empty() {
        return err(
            StatusCode::BAD_REQUEST,
            "username and password are required",
        );
    }

    let row = match sqlx::query(
        r#"
        SELECT id, username, password_hash, role, display_name, email, is_active
        FROM bff_users
        WHERE username = $1
        LIMIT 1
        "#,
    )
    .bind(username)
    .fetch_optional(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "login query failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let Some(row) = row else {
        return err(StatusCode::UNAUTHORIZED, "invalid credentials");
    };

    let is_active: bool = row.get("is_active");
    if !is_active {
        return err(StatusCode::FORBIDDEN, "user disabled");
    }

    let password_hash: String = row.get("password_hash");
    if !verify_password(&password_hash, &req.password) {
        return err(StatusCode::UNAUTHORIZED, "invalid credentials");
    }

    let user_id: Uuid = row.get("id");
    let token = new_session_token();
    let expires_at = session_expiry(st.session_ttl_hours);

    if let Err(e) =
        sqlx::query("INSERT INTO bff_sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
            .bind(&token)
            .bind(user_id)
            .bind(expires_at)
            .execute(&st.db)
            .await
    {
        tracing::error!(error=%e, "insert session failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    let role_raw: String = row.get("role");
    let role = role_from_str(&role_raw).unwrap_or(Role::Viewer);

    let resp = LoginResponse {
        token,
        expires_at,
        user: SessionUser {
            id: user_id.to_string(),
            username: row.get("username"),
            role: role_to_str(role).to_string(),
            display_name: row.get("display_name"),
            email: row.get("email"),
        },
    };

    (StatusCode::OK, Json(resp)).into_response()
}

pub async fn logout(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(token) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
    else {
        return err(StatusCode::BAD_REQUEST, "missing token");
    };

    let _ = sqlx::query("DELETE FROM bff_sessions WHERE token = $1")
        .bind(token)
        .execute(&st.db)
        .await;

    tracing::info!(principal=%ctx.principal, "user logged out");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

pub async fn me(State(st): State<AppState>, Extension(ctx): Extension<AuthContext>) -> Response {
    let row = match sqlx::query(
        r#"
        SELECT id, username, role, display_name, email
        FROM bff_users
        WHERE username = $1
        LIMIT 1
        "#,
    )
    .bind(&ctx.principal)
    .fetch_optional(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "me query failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let Some(row) = row else {
        return err(StatusCode::NOT_FOUND, "user not found").into_response();
    };

    let role_raw: String = row.get("role");
    let role = role_from_str(&role_raw).unwrap_or(Role::Viewer);

    let body = SessionUser {
        id: row.get::<Uuid, _>("id").to_string(),
        username: row.get("username"),
        role: role_to_str(role).to_string(),
        display_name: row.get("display_name"),
        email: row.get("email"),
    };

    (StatusCode::OK, Json(body)).into_response()
}

pub async fn update_profile(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<UpdateProfileRequest>,
) -> Response {
    let display_name = req
        .display_name
        .as_deref()
        .map(str::trim)
        .map(str::to_string);
    let email = req.email.as_deref().map(str::trim).map(str::to_string);

    if let Err(e) = sqlx::query(
        r#"
        UPDATE bff_users
        SET display_name = $1,
            email = $2,
            updated_at = NOW()
        WHERE username = $3
        "#,
    )
    .bind(display_name)
    .bind(email)
    .bind(&ctx.principal)
    .execute(&st.db)
    .await
    {
        tracing::error!(error=%e, "update profile failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

pub async fn get_settings(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Response {
    let user_row = match sqlx::query("SELECT id FROM bff_users WHERE username = $1 LIMIT 1")
        .bind(&ctx.principal)
        .fetch_optional(&st.db)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "get settings user query failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let Some(user_row) = user_row else {
        return err(StatusCode::NOT_FOUND, "user not found").into_response();
    };
    let user_id: Uuid = user_row.get("id");

    let _ = sqlx::query(
        "INSERT INTO bff_user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .execute(&st.db)
    .await;

    let settings = match sqlx::query(
        "SELECT in_app_alerts, email_alerts FROM bff_user_settings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "get settings query failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let body = UserSettingsResponse {
        in_app_alerts: settings.get("in_app_alerts"),
        email_alerts: settings.get("email_alerts"),
    };

    (StatusCode::OK, Json(body)).into_response()
}

pub async fn update_settings(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Response {
    let user_row = match sqlx::query("SELECT id FROM bff_users WHERE username = $1 LIMIT 1")
        .bind(&ctx.principal)
        .fetch_optional(&st.db)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "update settings user query failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let Some(user_row) = user_row else {
        return err(StatusCode::NOT_FOUND, "user not found");
    };
    let user_id: Uuid = user_row.get("id");

    let _ = sqlx::query(
        "INSERT INTO bff_user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .execute(&st.db)
    .await;

    let current = match sqlx::query(
        "SELECT in_app_alerts, email_alerts FROM bff_user_settings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "load current settings failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let in_app_alerts = req
        .in_app_alerts
        .unwrap_or_else(|| current.get("in_app_alerts"));
    let email_alerts = req
        .email_alerts
        .unwrap_or_else(|| current.get("email_alerts"));

    if let Err(e) = sqlx::query(
        "UPDATE bff_user_settings SET in_app_alerts = $1, email_alerts = $2, updated_at = NOW() WHERE user_id = $3",
    )
    .bind(in_app_alerts)
    .bind(email_alerts)
    .bind(user_id)
    .execute(&st.db)
    .await
    {
        tracing::error!(error=%e, "update settings failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

pub async fn list_users(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Response {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return resp;
    }

    let rows = match sqlx::query(
        "SELECT id, username, role, display_name, email, is_active FROM bff_users ORDER BY created_at ASC",
    )
    .fetch_all(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error=%e, "list users failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let users: Vec<UserItem> = rows
        .into_iter()
        .map(|r| {
            let role_raw: String = r.get("role");
            let role = role_from_str(&role_raw).unwrap_or(Role::Viewer);
            UserItem {
                id: r.get::<Uuid, _>("id").to_string(),
                username: r.get("username"),
                role: role_to_str(role).to_string(),
                display_name: r.get("display_name"),
                email: r.get("email"),
                is_active: r.get("is_active"),
            }
        })
        .collect();

    (StatusCode::OK, Json(users)).into_response()
}

pub async fn create_user(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateUserRequest>,
) -> Response {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return resp;
    }

    let username = req.username.trim();
    if username.is_empty() || req.password.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "username/password required").into_response();
    }

    let Some(role) = role_from_str(&req.role) else {
        return err(StatusCode::BAD_REQUEST, "invalid role").into_response();
    };

    let user_id = Uuid::new_v4();
    let password_hash = match crate::auth::hash_password(req.password.trim()) {
        Ok(v) => v,
        Err(_) => {
            return err(StatusCode::INTERNAL_SERVER_ERROR, "failed to hash password")
                .into_response()
        }
    };

    let insert = sqlx::query(
        r#"
        INSERT INTO bff_users (id, username, password_hash, role, display_name, email, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, TRUE)
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(password_hash)
    .bind(role_to_str(role))
    .bind(req.display_name.as_deref().map(str::trim))
    .bind(req.email.as_deref().map(str::trim))
    .execute(&st.db)
    .await;

    if let Err(e) = insert {
        tracing::warn!(error=%e, "create user failed");
        return err(StatusCode::CONFLICT, "user already exists").into_response();
    }

    let _ = sqlx::query(
        "INSERT INTO bff_user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .execute(&st.db)
    .await;

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"ok": true, "id": user_id})),
    )
        .into_response()
}

pub async fn update_user(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Response {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return resp;
    }

    let Ok(user_uuid) = Uuid::parse_str(&user_id) else {
        return err(StatusCode::BAD_REQUEST, "invalid user id").into_response();
    };

    let existing = match sqlx::query(
        "SELECT role, display_name, email, is_active FROM bff_users WHERE id = $1",
    )
    .bind(user_uuid)
    .fetch_optional(&st.db)
    .await
    {
        Ok(v) => v,
        Err(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    };

    let Some(existing) = existing else {
        return err(StatusCode::NOT_FOUND, "user not found").into_response();
    };

    let role = match req.role.as_deref() {
        Some(raw) => match role_from_str(raw) {
            Some(v) => role_to_str(v).to_string(),
            None => return err(StatusCode::BAD_REQUEST, "invalid role").into_response(),
        },
        None => existing.get::<String, _>("role"),
    };

    let display_name = req
        .display_name
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| existing.get::<Option<String>, _>("display_name"));
    let email = req
        .email
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| existing.get::<Option<String>, _>("email"));
    let is_active = req
        .is_active
        .unwrap_or_else(|| existing.get::<bool, _>("is_active"));

    if let Err(_) = sqlx::query(
        "UPDATE bff_users SET role = $1, display_name = $2, email = $3, is_active = $4, updated_at = NOW() WHERE id = $5",
    )
    .bind(role)
    .bind(display_name)
    .bind(email)
    .bind(is_active)
    .bind(user_uuid)
    .execute(&st.db)
    .await
    {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }

    if let Some(password) = req
        .password
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let hash = match crate::auth::hash_password(password) {
            Ok(v) => v,
            Err(_) => {
                return err(StatusCode::INTERNAL_SERVER_ERROR, "failed to hash password")
                    .into_response()
            }
        };
        let _ = sqlx::query(
            "UPDATE bff_users SET password_hash = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(hash)
        .bind(user_uuid)
        .execute(&st.db)
        .await;
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

pub async fn delete_user(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Response {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return resp;
    }

    let Ok(user_uuid) = Uuid::parse_str(&user_id) else {
        return err(StatusCode::BAD_REQUEST, "invalid user id").into_response();
    };

    let row = match sqlx::query("SELECT username FROM bff_users WHERE id = $1")
        .bind(user_uuid)
        .fetch_optional(&st.db)
        .await
    {
        Ok(v) => v,
        Err(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    };

    let Some(row) = row else {
        return err(StatusCode::NOT_FOUND, "user not found").into_response();
    };
    let username: String = row.get("username");
    if username == "admin" {
        return err(StatusCode::BAD_REQUEST, "default admin cannot be deleted").into_response();
    }

    if let Err(_) = sqlx::query("DELETE FROM bff_users WHERE id = $1")
        .bind(user_uuid)
        .execute(&st.db)
        .await
    {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}
