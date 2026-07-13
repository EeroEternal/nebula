//! Multi-tenant quotas, admission, and cost attribution (Product P6).

use serde::{Deserialize, Serialize};

/// Stable deny codes returned to clients and recorded in audit / usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantDenyCode {
    TenantDisabled,
    RpsExceeded,
    ConcurrencyExceeded,
    TokenBudgetExceeded,
    ModelDenied,
}

impl TenantDenyCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TenantDisabled => "tenant_disabled",
            Self::RpsExceeded => "tenant_rps_exceeded",
            Self::ConcurrencyExceeded => "tenant_concurrency_exceeded",
            Self::TokenBudgetExceeded => "tenant_token_budget_exceeded",
            Self::ModelDenied => "tenant_model_denied",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::TenantDisabled => "tenant disabled",
            Self::RpsExceeded => "tenant RPS quota exceeded",
            Self::ConcurrencyExceeded => "tenant concurrency quota exceeded",
            Self::TokenBudgetExceeded => "tenant token budget exceeded",
            Self::ModelDenied => "model not allowed for tenant",
        }
    }
}

/// Per-tenant quota policy. `None` fields mean "unlimited / inherit global".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantQuota {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rps_per_minute: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_per_minute: Option<u64>,
    /// When set, only listed model names / uids are allowed. Empty vec denies all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_models: Option<Vec<String>>,
}

impl Default for TenantQuota {
    fn default() -> Self {
        Self {
            rps_per_minute: None,
            max_concurrency: None,
            max_tokens_per_minute: None,
            allowed_models: None,
        }
    }
}

/// Tenant entity. etcd: `/tenants/{tenant_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tenant {
    pub tenant_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub quotas: TenantQuota,
    /// Documented API token principals bound to this tenant (binding enforced via auth env).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_token_principals: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_default: Option<i32>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_true() -> bool {
    true
}

/// Engine / hardware price table for cost attribution.
/// etcd: `/pricing/{price_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostPriceConfig {
    pub price_id: String,
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Currency unit cost per 1k input tokens.
    pub price_per_1k_input: f64,
    /// Currency unit cost per 1k output tokens.
    pub price_per_1k_output: f64,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_currency() -> String {
    "USD".into()
}

/// Aggregated usage for a tenant in a fixed window.
/// etcd: `/usage/{tenant_id}/{window_start_ms}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageWindow {
    pub tenant_id: String,
    /// Window start (epoch ms). Default window length is 15 minutes.
    pub window_start_ms: u64,
    #[serde(default = "default_window")]
    pub window: String,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub denied_rps: u64,
    #[serde(default)]
    pub denied_concurrency: u64,
    #[serde(default)]
    pub denied_model: u64,
    #[serde(default)]
    pub denied_token_budget: u64,
    #[serde(default)]
    pub denied_disabled: u64,
    /// Estimated cost in price currency (from pricing table).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_window() -> String {
    "15m".into()
}

/// Console / API cost summary for one tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantCostSummary {
    pub tenant_id: String,
    pub window: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub denied_total: u64,
    pub deny_breakdown: TenantDenyBreakdown,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub windows_merged: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TenantDenyBreakdown {
    pub rps: u64,
    pub concurrency: u64,
    pub model: u64,
    pub token_budget: u64,
    pub disabled: u64,
}

/// Result of static quota checks (before live counters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitDecision {
    Allow,
    Deny(TenantDenyCode),
}

/// Check model ACL and enabled flag without live counters.
pub fn admit_static(tenant: &Tenant, model: Option<&str>) -> AdmitDecision {
    if !tenant.enabled {
        return AdmitDecision::Deny(TenantDenyCode::TenantDisabled);
    }
    if let Some(allowed) = &tenant.quotas.allowed_models {
        let Some(m) = model.filter(|s| !s.is_empty()) else {
            return AdmitDecision::Deny(TenantDenyCode::ModelDenied);
        };
        if !allowed.iter().any(|a| a == m) {
            return AdmitDecision::Deny(TenantDenyCode::ModelDenied);
        }
    }
    AdmitDecision::Allow
}

/// Estimate cost from token counts and a price row.
pub fn estimate_cost(
    price: &CostPriceConfig,
    input_tokens: u64,
    output_tokens: u64,
) -> f64 {
    (input_tokens as f64 / 1000.0) * price.price_per_1k_input
        + (output_tokens as f64 / 1000.0) * price.price_per_1k_output
}

/// Merge usage windows into a tenant cost summary.
pub fn summarize_usage(
    tenant_id: &str,
    windows: &[UsageWindow],
    currency: Option<String>,
) -> TenantCostSummary {
    let mut summary = TenantCostSummary {
        tenant_id: tenant_id.to_string(),
        window: windows
            .first()
            .map(|w| w.window.clone())
            .unwrap_or_else(|| "15m".into()),
        requests: 0,
        input_tokens: 0,
        output_tokens: 0,
        denied_total: 0,
        deny_breakdown: TenantDenyBreakdown::default(),
        cost_estimate: None,
        currency,
        windows_merged: windows.len() as u64,
    };
    let mut cost = 0.0;
    let mut has_cost = false;
    for w in windows {
        summary.requests += w.requests;
        summary.input_tokens += w.input_tokens;
        summary.output_tokens += w.output_tokens;
        summary.deny_breakdown.rps += w.denied_rps;
        summary.deny_breakdown.concurrency += w.denied_concurrency;
        summary.deny_breakdown.model += w.denied_model;
        summary.deny_breakdown.token_budget += w.denied_token_budget;
        summary.deny_breakdown.disabled += w.denied_disabled;
        if let Some(c) = w.cost_estimate {
            cost += c;
            has_cost = true;
        }
    }
    summary.denied_total = summary.deny_breakdown.rps
        + summary.deny_breakdown.concurrency
        + summary.deny_breakdown.model
        + summary.deny_breakdown.token_budget
        + summary.deny_breakdown.disabled;
    if has_cost {
        summary.cost_estimate = Some(cost);
    }
    summary
}

/// Align epoch ms down to a 15-minute window start.
pub fn usage_window_start_ms(now_ms: u64) -> u64 {
    const FIFTEEN_MIN_MS: u64 = 15 * 60 * 1000;
    (now_ms / FIFTEEN_MIN_MS) * FIFTEEN_MIN_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant_with_models(models: Option<Vec<&str>>) -> Tenant {
        Tenant {
            tenant_id: "t1".into(),
            display_name: "T1".into(),
            enabled: true,
            quotas: TenantQuota {
                allowed_models: models.map(|m| m.into_iter().map(str::to_string).collect()),
                ..Default::default()
            },
            api_token_principals: vec![],
            priority_default: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn disabled_tenant_denied() {
        let mut t = tenant_with_models(None);
        t.enabled = false;
        assert_eq!(
            admit_static(&t, Some("m")),
            AdmitDecision::Deny(TenantDenyCode::TenantDisabled)
        );
    }

    #[test]
    fn model_acl_enforced() {
        let t = tenant_with_models(Some(vec!["allowed"]));
        assert_eq!(admit_static(&t, Some("allowed")), AdmitDecision::Allow);
        assert_eq!(
            admit_static(&t, Some("other")),
            AdmitDecision::Deny(TenantDenyCode::ModelDenied)
        );
        assert_eq!(
            admit_static(&t, None),
            AdmitDecision::Deny(TenantDenyCode::ModelDenied)
        );
    }

    #[test]
    fn cost_estimate_and_summary() {
        let price = CostPriceConfig {
            price_id: "vllm-cuda".into(),
            engine_type: "vllm".into(),
            platform: Some("nvidia-cuda".into()),
            price_per_1k_input: 0.1,
            price_per_1k_output: 0.2,
            currency: "USD".into(),
            notes: None,
            updated_at_ms: 0,
        };
        assert!((estimate_cost(&price, 1000, 500) - 0.2).abs() < 1e-9);

        let w = UsageWindow {
            tenant_id: "t1".into(),
            window_start_ms: 0,
            window: "15m".into(),
            requests: 10,
            input_tokens: 1000,
            output_tokens: 500,
            denied_rps: 2,
            denied_concurrency: 0,
            denied_model: 1,
            denied_token_budget: 0,
            denied_disabled: 0,
            cost_estimate: Some(0.2),
            model_uid: None,
            engine_type: Some("vllm".into()),
            cell_id: None,
            updated_at_ms: 0,
        };
        let s = summarize_usage("t1", &[w], Some("USD".into()));
        assert_eq!(s.denied_total, 3);
        assert_eq!(s.cost_estimate, Some(0.2));
    }
}
