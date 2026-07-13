//! P6 tenant, pricing, and usage / cost APIs.

use serde::Deserialize;

use nebula_common::{
    estimate_cost, summarize_usage, usage_window_start_ms, CostPriceConfig, Tenant, TenantCostSummary,
    TenantQuota, UsageWindow,
};
use nebula_meta::MetaStore;

use crate::service::{now_ms, ServiceError};

fn tenant_key(id: &str) -> String {
    format!("/tenants/{id}")
}

fn pricing_key(id: &str) -> String {
    format!("/pricing/{id}")
}

fn usage_key(tenant_id: &str, window_start_ms: u64) -> String {
    format!("/usage/{tenant_id}/{window_start_ms}")
}

pub async fn list_tenants(store: &dyn MetaStore) -> Result<Vec<Tenant>, ServiceError> {
    let entries = store.list_prefix("/tenants/").await?;
    let mut out: Vec<Tenant> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| a.tenant_id.cmp(&b.tenant_id));
    Ok(out)
}

pub async fn get_tenant(store: &dyn MetaStore, tenant_id: &str) -> Result<Tenant, ServiceError> {
    match store.get(&tenant_key(tenant_id)).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(format!("tenant {tenant_id} not found"))),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpsertTenantRequest {
    pub tenant_id: String,
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub quotas: Option<TenantQuota>,
    pub api_token_principals: Option<Vec<String>>,
    pub priority_default: Option<i32>,
}

pub async fn upsert_tenant(
    store: &dyn MetaStore,
    req: UpsertTenantRequest,
) -> Result<Tenant, ServiceError> {
    let id = req.tenant_id.trim();
    if id.is_empty() {
        return Err(ServiceError::BadRequest("tenant_id required".into()));
    }
    let now = now_ms();
    let existing = store.get(&tenant_key(id)).await?;
    let mut tenant = if let Some((data, _)) = existing {
        serde_json::from_slice::<Tenant>(&data)?
    } else {
        Tenant {
            tenant_id: id.to_string(),
            display_name: id.to_string(),
            enabled: true,
            quotas: TenantQuota::default(),
            api_token_principals: vec![],
            priority_default: None,
            created_at_ms: now,
            updated_at_ms: now,
        }
    };
    if let Some(name) = req.display_name {
        tenant.display_name = name;
    }
    if let Some(en) = req.enabled {
        tenant.enabled = en;
    }
    if let Some(q) = req.quotas {
        tenant.quotas = q;
    }
    if let Some(p) = req.api_token_principals {
        tenant.api_token_principals = p;
    }
    if req.priority_default.is_some() {
        tenant.priority_default = req.priority_default;
    }
    tenant.updated_at_ms = now;
    if tenant.created_at_ms == 0 {
        tenant.created_at_ms = now;
    }
    store
        .put(&tenant_key(id), serde_json::to_vec(&tenant)?, None)
        .await?;
    Ok(tenant)
}

pub async fn delete_tenant(store: &dyn MetaStore, tenant_id: &str) -> Result<(), ServiceError> {
    store.delete(&tenant_key(tenant_id)).await?;
    Ok(())
}

pub async fn list_pricing(store: &dyn MetaStore) -> Result<Vec<CostPriceConfig>, ServiceError> {
    let entries = store.list_prefix("/pricing/").await?;
    let mut out: Vec<CostPriceConfig> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| a.price_id.cmp(&b.price_id));
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct UpsertPricingRequest {
    pub price_id: String,
    pub engine_type: String,
    pub platform: Option<String>,
    pub price_per_1k_input: f64,
    pub price_per_1k_output: f64,
    pub currency: Option<String>,
    pub notes: Option<String>,
}

pub async fn upsert_pricing(
    store: &dyn MetaStore,
    req: UpsertPricingRequest,
) -> Result<CostPriceConfig, ServiceError> {
    if req.price_id.trim().is_empty() {
        return Err(ServiceError::BadRequest("price_id required".into()));
    }
    let cfg = CostPriceConfig {
        price_id: req.price_id.trim().to_string(),
        engine_type: req.engine_type,
        platform: req.platform,
        price_per_1k_input: req.price_per_1k_input,
        price_per_1k_output: req.price_per_1k_output,
        currency: req.currency.unwrap_or_else(|| "USD".into()),
        notes: req.notes,
        updated_at_ms: now_ms(),
    };
    store
        .put(&pricing_key(&cfg.price_id), serde_json::to_vec(&cfg)?, None)
        .await?;
    Ok(cfg)
}

pub async fn delete_pricing(store: &dyn MetaStore, price_id: &str) -> Result<(), ServiceError> {
    store.delete(&pricing_key(price_id)).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct IngestUsageRequest {
    pub tenant_id: String,
    pub requests: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub denied_rps: Option<u64>,
    pub denied_concurrency: Option<u64>,
    pub denied_model: Option<u64>,
    pub denied_token_budget: Option<u64>,
    pub denied_disabled: Option<u64>,
    pub model_uid: Option<String>,
    pub engine_type: Option<String>,
    pub price_id: Option<String>,
    pub window_start_ms: Option<u64>,
}

pub async fn ingest_usage(
    store: &dyn MetaStore,
    req: IngestUsageRequest,
) -> Result<UsageWindow, ServiceError> {
    let tenant_id = req.tenant_id.trim();
    if tenant_id.is_empty() {
        return Err(ServiceError::BadRequest("tenant_id required".into()));
    }
    let now = now_ms();
    let window_start = req
        .window_start_ms
        .unwrap_or_else(|| usage_window_start_ms(now));
    let key = usage_key(tenant_id, window_start);
    let mut window = match store.get(&key).await? {
        Some((data, _)) => serde_json::from_slice::<UsageWindow>(&data)?,
        None => UsageWindow {
            tenant_id: tenant_id.to_string(),
            window_start_ms: window_start,
            window: "15m".into(),
            requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            denied_rps: 0,
            denied_concurrency: 0,
            denied_model: 0,
            denied_token_budget: 0,
            denied_disabled: 0,
            cost_estimate: None,
            model_uid: req.model_uid.clone(),
            engine_type: req.engine_type.clone(),
            updated_at_ms: now,
        },
    };
    window.requests += req.requests.unwrap_or(0);
    window.input_tokens += req.input_tokens.unwrap_or(0);
    window.output_tokens += req.output_tokens.unwrap_or(0);
    window.denied_rps += req.denied_rps.unwrap_or(0);
    window.denied_concurrency += req.denied_concurrency.unwrap_or(0);
    window.denied_model += req.denied_model.unwrap_or(0);
    window.denied_token_budget += req.denied_token_budget.unwrap_or(0);
    window.denied_disabled += req.denied_disabled.unwrap_or(0);
    if req.model_uid.is_some() {
        window.model_uid = req.model_uid;
    }
    if req.engine_type.is_some() {
        window.engine_type = req.engine_type;
    }

    // Attach cost estimate when a price row is provided or engine matches a price.
    let prices = list_pricing(store).await.unwrap_or_default();
    let price = req
        .price_id
        .as_ref()
        .and_then(|id| prices.iter().find(|p| &p.price_id == id))
        .or_else(|| {
            window
                .engine_type
                .as_ref()
                .and_then(|eng| prices.iter().find(|p| &p.engine_type == eng))
        });
    if let Some(p) = price {
        window.cost_estimate = Some(estimate_cost(p, window.input_tokens, window.output_tokens));
    }
    window.updated_at_ms = now;
    store.put(&key, serde_json::to_vec(&window)?, None).await?;
    Ok(window)
}

pub async fn list_usage(
    store: &dyn MetaStore,
    tenant_id: &str,
) -> Result<Vec<UsageWindow>, ServiceError> {
    let prefix = format!("/usage/{tenant_id}/");
    let entries = store.list_prefix(&prefix).await?;
    let mut out: Vec<UsageWindow> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| b.window_start_ms.cmp(&a.window_start_ms));
    Ok(out)
}

pub async fn tenant_cost_summary(
    store: &dyn MetaStore,
    tenant_id: &str,
) -> Result<TenantCostSummary, ServiceError> {
    let windows = list_usage(store, tenant_id).await?;
    let currency = list_pricing(store)
        .await?
        .first()
        .map(|p| p.currency.clone());
    Ok(summarize_usage(tenant_id, &windows, currency))
}
