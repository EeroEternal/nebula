//! L3 selection APIs (Phase 1): profiles + recommend/draft/apply.

use nebula_common::{
    draft_from_candidate, select_backends, DeploymentDraft, DesiredState, DraftRequest, ModelProfile,
    ModelSpec, ModelSource, SelectionRequest, SelectionResponse,
};
use nebula_meta::MetaStore;
use serde::Deserialize;

use crate::service::{get_model_deployment, get_model_spec, put_model_deployment, put_model_spec, ServiceError};

fn profile_key(id: &str) -> String {
    format!("/model_profiles/{id}")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub async fn put_profile(
    store: &dyn MetaStore,
    mut profile: ModelProfile,
) -> Result<ModelProfile, ServiceError> {
    if profile.profile_id.trim().is_empty() {
        return Err(ServiceError::BadRequest("profile_id required".into()));
    }
    profile.updated_at_ms = now_ms();
    let val = serde_json::to_vec(&profile)?;
    store
        .put(&profile_key(&profile.profile_id), val, None)
        .await?;
    Ok(profile)
}

pub async fn get_profile(store: &dyn MetaStore, id: &str) -> Result<ModelProfile, ServiceError> {
    match store.get(&profile_key(id)).await? {
        Some((bytes, _)) => Ok(serde_json::from_slice(&bytes)?),
        None => Err(ServiceError::NotFound(format!("model profile {id}"))),
    }
}

pub async fn recommend(
    store: &dyn MetaStore,
    req: SelectionRequest,
) -> Result<SelectionResponse, ServiceError> {
    let profiles = crate::benchmark_svc::list_profiles(store).await?;
    let runs = crate::benchmark_svc::list_runs(store).await?;
    Ok(select_backends(&req, &profiles, &runs))
}

pub async fn draft(
    store: &dyn MetaStore,
    req: DraftRequest,
) -> Result<DeploymentDraft, ServiceError> {
    let resp = recommend(store, req.selection.clone()).await?;
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
