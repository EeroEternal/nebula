//! Engine × version × platform × driver compatibility matrix.

use serde::{Deserialize, Serialize};

use crate::capability::{parse_version_tuple, validate_engine_version};
use crate::node_status::{image_platforms_match, NodeStatus, resolve_node_platform};

/// Allow or deny a combination in the compatibility matrix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompatVerdict {
    #[default]
    Allow,
    Deny,
}

/// One row of the compatibility matrix.
///
/// etcd key: `/compat/{id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompatibilityRule {
    pub id: String,
    pub engine_type: String,
    /// Inclusive min engine version; None = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version_min: Option<String>,
    /// Exclusive max engine version; None = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version_max: Option<String>,
    /// Compatible platforms (e.g. `nvidia-cuda`). Empty = all platforms.
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_driver_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_cuda_version: Option<String>,
    pub verdict: CompatVerdict,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

/// Structured placement / deploy rejection for operators and UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementRejectReason {
    /// Machine-readable code, e.g. `platform_incompatible`.
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
}

impl PlacementRejectReason {
    pub fn platform_incompatible(
        platforms: Vec<String>,
        rejected: u32,
        image_id: Option<String>,
    ) -> Self {
        Self {
            code: "platform_incompatible".into(),
            message: format!(
                "no healthy nodes compatible with image platforms {platforms:?}; rejected {rejected} node(s)"
            ),
            rule_id: None,
            node_id: None,
            image_id,
            platforms,
        }
    }

    pub fn compat_denied(rule_id: &str, message: impl Into<String>) -> Self {
        Self {
            code: "compat_denied".into(),
            message: message.into(),
            rule_id: Some(rule_id.to_string()),
            node_id: None,
            image_id: None,
            platforms: vec![],
        }
    }

    pub fn no_healthy_nodes() -> Self {
        Self {
            code: "no_healthy_nodes".into(),
            message: "no healthy nodes available".into(),
            rule_id: None,
            node_id: None,
            image_id: None,
            platforms: vec![],
        }
    }

    pub fn format_error(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}

/// Inputs for evaluating a deploy against the matrix.
#[derive(Debug, Clone)]
pub struct CompatCheckInput<'a> {
    pub engine_type: &'a str,
    pub engine_version: Option<&'a str>,
    pub platforms: &'a [String],
    pub node: Option<&'a NodeStatus>,
    pub image_id: Option<&'a str>,
}

fn version_in_range(ver: &str, min: Option<&str>, max: Option<&str>) -> bool {
    let Some(v) = parse_version_tuple(ver) else {
        return true;
    };
    let min_ok = min
        .and_then(parse_version_tuple)
        .map(|m| {
            use std::cmp::Ordering;
            let len = v.len().max(m.len());
            for i in 0..len {
                let av = v.get(i).copied().unwrap_or(0);
                let bv = m.get(i).copied().unwrap_or(0);
                match av.cmp(&bv) {
                    Ordering::Less => return false,
                    Ordering::Greater => return true,
                    Ordering::Equal => {}
                }
            }
            true
        })
        .unwrap_or(true);
    let max_ok = max
        .and_then(parse_version_tuple)
        .map(|m| {
            use std::cmp::Ordering;
            let len = v.len().max(m.len());
            for i in 0..len {
                let av = v.get(i).copied().unwrap_or(0);
                let bv = m.get(i).copied().unwrap_or(0);
                match av.cmp(&bv) {
                    Ordering::Less => return true,
                    Ordering::Greater => return false,
                    Ordering::Equal => {}
                }
            }
            false // equal to exclusive max → out
        })
        .unwrap_or(true);
    min_ok && max_ok
}

fn rule_applies(rule: &CompatibilityRule, input: &CompatCheckInput<'_>) -> bool {
    if rule.engine_type != input.engine_type {
        return false;
    }
    if let Some(ver) = input.engine_version {
        if !version_in_range(
            ver,
            rule.engine_version_min.as_deref(),
            rule.engine_version_max.as_deref(),
        ) {
            return false;
        }
    }
    if !rule.platforms.is_empty() {
        if input.platforms.is_empty() {
            if let Some(node) = input.node {
                let p = resolve_node_platform(node);
                if !image_platforms_match(&rule.platforms, &p) {
                    return false;
                }
            }
        } else if !input
            .platforms
            .iter()
            .any(|p| rule.platforms.iter().any(|rp| rp == p))
        {
            return false;
        }
    }
    true
}

fn driver_too_old(node: &NodeStatus, min_driver: &str) -> bool {
    let Some(gpu) = node.gpus.first() else {
        return false;
    };
    let Some(ref drv) = gpu.driver_version else {
        return false;
    };
    !version_in_range(drv, Some(min_driver), None)
}

fn cuda_too_old(node: &NodeStatus, min_cuda: &str) -> bool {
    let Some(gpu) = node.gpus.first() else {
        return false;
    };
    let Some(ref cuda) = gpu.cuda_version else {
        return false;
    };
    !version_in_range(cuda, Some(min_cuda), None)
}

/// Evaluate rules against a deploy intent. Deny rules that match win.
pub fn evaluate_compatibility(
    rules: &[CompatibilityRule],
    input: &CompatCheckInput<'_>,
) -> Result<Vec<String>, PlacementRejectReason> {
    // Adapter static version floor first.
    if let Err(msg) = validate_engine_version(input.engine_type, input.engine_version) {
        return Err(PlacementRejectReason {
            code: "engine_version_unsupported".into(),
            message: msg,
            rule_id: None,
            node_id: input.node.map(|n| n.node_id.clone()),
            image_id: input.image_id.map(|s| s.to_string()),
            platforms: input.platforms.to_vec(),
        });
    }

    let mut matched_allows = Vec::new();
    for rule in rules {
        if !rule_applies(rule, input) {
            continue;
        }
        if let Some(node) = input.node {
            if let Some(ref min_d) = rule.min_driver_version {
                if driver_too_old(node, min_d) {
                    return Err(PlacementRejectReason {
                        code: "driver_too_old".into(),
                        message: format!(
                            "node '{}' driver below rule '{}' min {}",
                            node.node_id, rule.id, min_d
                        ),
                        rule_id: Some(rule.id.clone()),
                        node_id: Some(node.node_id.clone()),
                        image_id: input.image_id.map(|s| s.to_string()),
                        platforms: vec![],
                    });
                }
            }
            if let Some(ref min_c) = rule.min_cuda_version {
                if cuda_too_old(node, min_c) {
                    return Err(PlacementRejectReason {
                        code: "cuda_too_old".into(),
                        message: format!(
                            "node '{}' CUDA below rule '{}' min {}",
                            node.node_id, rule.id, min_c
                        ),
                        rule_id: Some(rule.id.clone()),
                        node_id: Some(node.node_id.clone()),
                        image_id: input.image_id.map(|s| s.to_string()),
                        platforms: vec![],
                    });
                }
            }
        }
        match rule.verdict {
            CompatVerdict::Deny => {
                return Err(PlacementRejectReason::compat_denied(
                    &rule.id,
                    format!(
                        "compatibility rule '{}' denies engine '{}' on platforms {:?}",
                        rule.id, input.engine_type, rule.platforms
                    ),
                ));
            }
            CompatVerdict::Allow => matched_allows.push(rule.id.clone()),
        }
    }
    Ok(matched_allows)
}

/// Seed rules shipped with Nebula (also used when etcd matrix is empty).
pub fn default_compatibility_rules(now_ms: u64) -> Vec<CompatibilityRule> {
    vec![
        CompatibilityRule {
            id: "vllm-nvidia-cuda".into(),
            engine_type: "vllm".into(),
            engine_version_min: Some("0.6.0".into()),
            engine_version_max: None,
            platforms: vec!["nvidia-cuda".into()],
            min_driver_version: Some("525.0".into()),
            min_cuda_version: Some("12.0".into()),
            verdict: CompatVerdict::Allow,
            known_issues: vec![],
            notes: Some("Default NVIDIA path for vLLM".into()),
            updated_at_ms: now_ms,
        },
        CompatibilityRule {
            id: "sglang-nvidia-cuda".into(),
            engine_type: "sglang".into(),
            engine_version_min: Some("0.3.0".into()),
            engine_version_max: None,
            platforms: vec!["nvidia-cuda".into()],
            min_driver_version: Some("525.0".into()),
            min_cuda_version: Some("12.0".into()),
            verdict: CompatVerdict::Allow,
            known_issues: vec![],
            notes: Some("Default NVIDIA path for SGLang".into()),
            updated_at_ms: now_ms,
        },
        CompatibilityRule {
            id: "deny-vllm-on-ascend-until-adapter".into(),
            engine_type: "vllm".into(),
            engine_version_min: None,
            engine_version_max: None,
            platforms: vec!["ascend-cann8".into()],
            min_driver_version: None,
            min_cuda_version: None,
            verdict: CompatVerdict::Deny,
            known_issues: vec!["Ascend adapter not production-ready; register manually".into()],
            notes: Some("Other platforms require manual登记 until Adapter ready".into()),
            updated_at_ms: now_ms,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_status::GpuStatus;

    fn node(platform: &str, driver: &str, cuda: &str) -> NodeStatus {
        NodeStatus {
            node_id: "n1".into(),
            last_heartbeat_ms: 1,
            gpus: vec![GpuStatus {
                index: 0,
                memory_total_mb: 40000,
                memory_used_mb: 0,
                temperature_c: None,
                utilization_gpu: None,
                name: Some("A100".into()),
                driver_version: Some(driver.into()),
                cuda_version: Some(cuda.into()),
            }],
            api_addr: None,
            platform: Some(platform.into()),
        }
    }

    #[test]
    fn deny_ascend_vllm() {
        let rules = default_compatibility_rules(0);
        let n = node("ascend-cann8", "1.0", "1.0");
        let input = CompatCheckInput {
            engine_type: "vllm",
            engine_version: Some("0.8.0"),
            platforms: &["ascend-cann8".into()],
            node: Some(&n),
            image_id: Some("vllm-ascend"),
        };
        let err = evaluate_compatibility(&rules, &input).unwrap_err();
        assert_eq!(err.code, "compat_denied");
    }

    #[test]
    fn allow_nvidia_vllm() {
        let rules = default_compatibility_rules(0);
        let n = node("nvidia-cuda", "550.0", "12.4");
        let plats = vec!["nvidia-cuda".into()];
        let input = CompatCheckInput {
            engine_type: "vllm",
            engine_version: Some("0.8.0"),
            platforms: &plats,
            node: Some(&n),
            image_id: Some("vllm-cuda"),
        };
        let ids = evaluate_compatibility(&rules, &input).unwrap();
        assert!(ids.iter().any(|id| id == "vllm-nvidia-cuda"));
    }

    #[test]
    fn rejects_old_driver() {
        let rules = default_compatibility_rules(0);
        let n = node("nvidia-cuda", "470.0", "12.4");
        let plats = vec!["nvidia-cuda".into()];
        let input = CompatCheckInput {
            engine_type: "vllm",
            engine_version: Some("0.8.0"),
            platforms: &plats,
            node: Some(&n),
            image_id: None,
        };
        let err = evaluate_compatibility(&rules, &input).unwrap_err();
        assert_eq!(err.code, "driver_too_old");
    }
}
