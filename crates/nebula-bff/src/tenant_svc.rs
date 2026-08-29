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

pub async fn list_pricing_db(db: &sqlx::PgPool) -> Result<Vec<CostPriceConfig>, ServiceError> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"
        SELECT price_id, engine_type, platform, price_per_1k_input, price_per_1k_output, currency, notes, updated_at_ms
        FROM bff_pricing
        ORDER BY price_id ASC
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error listing pricing: {e}")))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(CostPriceConfig {
            price_id: r.get("price_id"),
            engine_type: r.get("engine_type"),
            platform: r.get("platform"),
            price_per_1k_input: r.get("price_per_1k_input"),
            price_per_1k_output: r.get("price_per_1k_output"),
            currency: r.get("currency"),
            notes: r.get("notes"),
            updated_at_ms: r.get::<i64, _>("updated_at_ms") as u64,
        });
    }
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

pub async fn upsert_pricing_db(
    db: &sqlx::PgPool,
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

    sqlx::query(
        r#"
        INSERT INTO bff_pricing (
            price_id, engine_type, platform, price_per_1k_input, price_per_1k_output, currency, notes, updated_at_ms
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (price_id) DO UPDATE SET
            engine_type = EXCLUDED.engine_type,
            platform = EXCLUDED.platform,
            price_per_1k_input = EXCLUDED.price_per_1k_input,
            price_per_1k_output = EXCLUDED.price_per_1k_output,
            currency = EXCLUDED.currency,
            notes = EXCLUDED.notes,
            updated_at_ms = EXCLUDED.updated_at_ms
        "#,
    )
    .bind(&cfg.price_id)
    .bind(&cfg.engine_type)
    .bind(&cfg.platform)
    .bind(cfg.price_per_1k_input)
    .bind(cfg.price_per_1k_output)
    .bind(&cfg.currency)
    .bind(&cfg.notes)
    .bind(cfg.updated_at_ms as i64)
    .execute(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error saving pricing: {e}")))?;

    Ok(cfg)
}

pub async fn delete_pricing_db(db: &sqlx::PgPool, price_id: &str) -> Result<(), ServiceError> {
    sqlx::query("DELETE FROM bff_pricing WHERE price_id = $1")
        .bind(price_id)
        .execute(db)
        .await
        .map_err(|e| ServiceError::Internal(format!("db error deleting pricing: {e}")))?;
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

pub async fn ingest_usage_db(
    db: &sqlx::PgPool,
    req: IngestUsageRequest,
) -> Result<UsageWindow, ServiceError> {
    use sqlx::Row;
    let tenant_id = req.tenant_id.trim();
    if tenant_id.is_empty() {
        return Err(ServiceError::BadRequest("tenant_id required".into()));
    }
    let now = now_ms();
    let window_start = req
        .window_start_ms
        .unwrap_or_else(|| usage_window_start_ms(now));

    let existing_row = sqlx::query(
        r#"
        SELECT window_duration, requests, input_tokens, output_tokens,
               denied_rps, denied_concurrency, denied_model, denied_token_budget, denied_disabled,
               cost_estimate, model_uid, engine_type
        FROM bff_usage
        WHERE tenant_id = $1 AND window_start_ms = $2
        "#,
    )
    .bind(tenant_id)
    .bind(window_start as i64)
    .fetch_optional(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error fetching usage: {e}")))?;

    let mut window = match existing_row {
        Some(r) => UsageWindow {
            tenant_id: tenant_id.to_string(),
            window_start_ms: window_start,
            window: r.get("window_duration"),
            requests: r.get::<i64, _>("requests") as u64,
            input_tokens: r.get::<i64, _>("input_tokens") as u64,
            output_tokens: r.get::<i64, _>("output_tokens") as u64,
            denied_rps: r.get::<i64, _>("denied_rps") as u64,
            denied_concurrency: r.get::<i64, _>("denied_concurrency") as u64,
            denied_model: r.get::<i64, _>("denied_model") as u64,
            denied_token_budget: r.get::<i64, _>("denied_token_budget") as u64,
            denied_disabled: r.get::<i64, _>("denied_disabled") as u64,
            cost_estimate: r.get("cost_estimate"),
            model_uid: r.get("model_uid"),
            engine_type: r.get("engine_type"),
            updated_at_ms: now,
        },
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

    let prices = list_pricing_db(db).await.unwrap_or_default();
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

    sqlx::query(
        r#"
        INSERT INTO bff_usage (
            tenant_id, window_start_ms, window_duration, requests, input_tokens, output_tokens,
            denied_rps, denied_concurrency, denied_model, denied_token_budget, denied_disabled,
            cost_estimate, model_uid, engine_type, updated_at_ms
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (tenant_id, window_start_ms) DO UPDATE SET
            window_duration = EXCLUDED.window_duration,
            requests = EXCLUDED.requests,
            input_tokens = EXCLUDED.input_tokens,
            output_tokens = EXCLUDED.output_tokens,
            denied_rps = EXCLUDED.denied_rps,
            denied_concurrency = EXCLUDED.denied_concurrency,
            denied_model = EXCLUDED.denied_model,
            denied_token_budget = EXCLUDED.denied_token_budget,
            denied_disabled = EXCLUDED.denied_disabled,
            cost_estimate = EXCLUDED.cost_estimate,
            model_uid = EXCLUDED.model_uid,
            engine_type = EXCLUDED.engine_type,
            updated_at_ms = EXCLUDED.updated_at_ms
        "#,
    )
    .bind(&window.tenant_id)
    .bind(window.window_start_ms as i64)
    .bind(&window.window)
    .bind(window.requests as i64)
    .bind(window.input_tokens as i64)
    .bind(window.output_tokens as i64)
    .bind(window.denied_rps as i64)
    .bind(window.denied_concurrency as i64)
    .bind(window.denied_model as i64)
    .bind(window.denied_token_budget as i64)
    .bind(window.denied_disabled as i64)
    .bind(window.cost_estimate)
    .bind(&window.model_uid)
    .bind(&window.engine_type)
    .bind(window.updated_at_ms as i64)
    .execute(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error saving usage: {e}")))?;

    Ok(window)
}

pub async fn list_usage_db(
    db: &sqlx::PgPool,
    tenant_id: &str,
) -> Result<Vec<UsageWindow>, ServiceError> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"
        SELECT tenant_id, window_start_ms, window_duration, requests, input_tokens, output_tokens,
               denied_rps, denied_concurrency, denied_model, denied_token_budget, denied_disabled,
               cost_estimate, model_uid, engine_type, updated_at_ms
        FROM bff_usage
        WHERE tenant_id = $1
        ORDER BY window_start_ms DESC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
    .map_err(|e| ServiceError::Internal(format!("db error listing usage: {e}")))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(UsageWindow {
            tenant_id: r.get("tenant_id"),
            window_start_ms: r.get::<i64, _>("window_start_ms") as u64,
            window: r.get("window_duration"),
            requests: r.get::<i64, _>("requests") as u64,
            input_tokens: r.get::<i64, _>("input_tokens") as u64,
            output_tokens: r.get::<i64, _>("output_tokens") as u64,
            denied_rps: r.get::<i64, _>("denied_rps") as u64,
            denied_concurrency: r.get::<i64, _>("denied_concurrency") as u64,
            denied_model: r.get::<i64, _>("denied_model") as u64,
            denied_token_budget: r.get::<i64, _>("denied_token_budget") as u64,
            denied_disabled: r.get::<i64, _>("denied_disabled") as u64,
            cost_estimate: r.get("cost_estimate"),
            model_uid: r.get("model_uid"),
            engine_type: r.get("engine_type"),
            updated_at_ms: r.get::<i64, _>("updated_at_ms") as u64,
        });
    }
    Ok(out)
}

pub async fn tenant_cost_summary_db(
    db: &sqlx::PgPool,
    tenant_id: &str,
) -> Result<TenantCostSummary, ServiceError> {
    let windows = list_usage_db(db, tenant_id).await?;
    let currency = list_pricing_db(db)
        .await?
        .first()
        .map(|p| p.currency.clone());
    Ok(summarize_usage(tenant_id, &windows, currency))
}
