//! L3 selection APIs (Phase 1): profiles + recommend/draft/apply.

use nebula_common::{
    draft_from_candidate, select_backends, CurrentBackend, DeploymentDraft, DesiredState,
    DraftRequest, ModelProfile, ModelSpec, ModelSource, SelectionRequest, SelectionResponse,
};
use nebula_meta::MetaStore;
use serde::Deserialize;

use crate::service::{get_model_deployment, get_model_spec, put_model_deployment, put_model_spec, ServiceError};

fn current_is_empty(current: &CurrentBackend) -> bool {
    current.engine_type.is_none() && current.image_id.is_none() && current.platform.is_none()
}

/// Fill `current` from ModelSpec / Deployment when the client left it empty.
async fn fill_current_from_model(
    store: &dyn MetaStore,
    req: &mut SelectionRequest,
) -> Result<(), ServiceError> {
    if !current_is_empty(&req.current) {
        return Ok(());
    }
    let Some(uid) = req
        .model
        .model_uid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(());
    };

    let spec = match get_model_spec(store, uid).await {
        Ok(s) => Some(s),
        Err(ServiceError::NotFound(_)) => None,
        Err(e) => return Err(e),
    };
    let deployment = get_model_deployment(store, uid).await?;

    let engine_type = spec
        .as_ref()
        .and_then(|s| s.engine_type.clone())
        .filter(|s| !s.trim().is_empty());
    let image_id = deployment
        .as_ref()
        .and_then(|d| d.image_id.clone())
        .or_else(|| {
            spec.as_ref()
                .and_then(|s| s.docker_image.clone())
                .filter(|s| !s.trim().is_empty())
        });

    if engine_type.is_some() || image_id.is_some() {
        req.current = CurrentBackend {
            engine_type,
            image_id,
            platform: None,
        };
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub async fn put_profile_db(
    db: &sqlx::PgPool,
    mut profile: ModelProfile,
) -> Result<ModelProfile, ServiceError> {
    if profile.profile_id.trim().is_empty() {
        return Err(ServiceError::BadRequest("profile_id required".into()));
    }
    profile.updated_at_ms = now_ms();
    let val = serde_json::to_value(&profile)?;

    sqlx::query(
        r#"
        INSERT INTO bff_model_profiles (profile_id, profile_json, updated_at_ms)
        VALUES ($1, $2, $3)
        ON CONFLICT (profile_id) DO UPDATE SET
            profile_json = EXCLUDED.profile_json,
            updated_at_ms = EXCLUDED.updated_at_ms
        "#,
    )
    .bind(&profile.profile_id)
    .bind(val)
    .bind(profile.updated_at_ms as i64)
    .execute(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error saving profile: {e}")))?;

    Ok(profile)
}

pub async fn get_profile_db(db: &sqlx::PgPool, id: &str) -> Result<ModelProfile, ServiceError> {
    let row = sqlx::query(
        r#"
        SELECT profile_json FROM bff_model_profiles WHERE profile_id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error getting profile: {e}")))?;

    match row {
        Some(r) => {
            use sqlx::Row;
            let val: serde_json::Value = r.get("profile_json");
            serde_json::from_value(val).map_err(Into::into)
        }
        None => Err(ServiceError::NotFound(format!("model profile {id}"))),
    }
}

pub async fn recommend(
    store: &dyn MetaStore,
    db: &sqlx::PgPool,
    mut req: SelectionRequest,
) -> Result<SelectionResponse, ServiceError> {
    fill_current_from_model(store, &mut req).await?;
    let profiles = crate::benchmark_svc::list_profiles_db(db).await?;
    let runs = crate::benchmark_svc::list_runs_db(db).await?;
    Ok(select_backends(&req, &profiles, &runs))
}

pub async fn draft(
    store: &dyn MetaStore,
    db: &sqlx::PgPool,
    req: DraftRequest,
) -> Result<DeploymentDraft, ServiceError> {
    let resp = recommend(store, db, req.selection.clone()).await?;
    let candidate = resp
        .candidates
        .get(req.candidate_index)
        .ok_or_else(|| {
            ServiceError::BadRequest(format!(
                "candidate_index {} out of range ({} candidates)",
                req.candidate_index,
                resp.candidates.len()
            ))
        })?;
    draft_from_candidate(&req, candidate, now_ms()).map_err(ServiceError::BadRequest)
}

#[derive(Deserialize)]
pub struct ApplySelectionRequest {
    pub draft: DeploymentDraft,
    /// If true, create/update ModelSpec engine_type + image before writing deployment.
    #[serde(default = "default_true")]
    pub upsert_spec: bool,
}

fn default_true() -> bool {
    true
}

pub async fn apply(
    store: &dyn MetaStore,
    principal: String,
    req: ApplySelectionRequest,
) -> Result<DeploymentDraft, ServiceError> {
    let draft = req.draft;
    if draft.candidate.confidence == nebula_common::RecommendConfidence::InsufficientData {
        return Err(ServiceError::BadRequest(
            "refuse to apply insufficient_data draft".into(),
        ));
    }

    if req.upsert_spec {
        let now = now_ms();
        match get_model_spec(store, &draft.model_uid).await {
            Ok(mut spec) => {
                spec.engine_type = Some(draft.engine_type.clone());
                if draft.image_id.is_some() {
                    spec.docker_image = draft.image_id.clone();
                }
                spec.updated_at_ms = now;
                put_model_spec(store, &draft.model_uid, &spec).await?;
            }
            Err(ServiceError::NotFound(_)) => {
                let spec = ModelSpec {
                    model_uid: draft.model_uid.clone(),
                    model_name: draft.model_name.clone(),
                    model_source: ModelSource::HuggingFace,
                    model_path: None,
                    engine_type: Some(draft.engine_type.clone()),
                    docker_image: draft.image_id.clone(),
                    config: draft.deployment.config_overrides.clone(),
                    labels: Default::default(),
                    created_at_ms: now,
                    updated_at_ms: now,
                    created_by: Some(principal),
                };
                put_model_spec(store, &draft.model_uid, &spec).await?;
            }
            Err(e) => return Err(e),
        }
    } else {
        let _ = get_model_spec(store, &draft.model_uid).await?;
    }

    let mut dep = draft.deployment.clone();
    dep.desired_state = DesiredState::Running;
    dep.updated_at_ms = now_ms();
    dep.version = get_model_deployment(store, &draft.model_uid)
        .await?
        .map(|d| d.version.saturating_add(1))
        .unwrap_or(1);
    put_model_deployment(store, &draft.model_uid, &dep).await?;

    Ok(DeploymentDraft {
        deployment: dep,
        note: "applied: deployment written; scheduler will reconcile".into(),
        ..draft
    })
}
