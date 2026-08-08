pub mod benchmark;
pub mod capability;
pub mod capacity;
pub mod cluster;
pub mod compat;
pub mod endpoint;
pub mod engine_args;
pub mod engine_image;
pub mod execution_context;
pub mod json_model;
pub mod model_cache;
pub mod model_deployment;
pub mod model_request;
pub mod model_spec;
pub mod model_template;
pub mod node_status;
pub mod placement;
pub mod selection;
pub mod slo;
pub mod tenant;
pub mod admission;

pub use capability::{
    parse_version_tuple, resolve_engine_type, static_capability, static_capability_sglang,
    static_capability_vllm, static_version_support, tool_calling_for_engine,
    validate_engine_and_config, validate_engine_type, validate_engine_version, validate_model_config,
    CapabilitySource, EngineCapability, EngineVersionSupport, ObservabilityCapability,
    ReplicaCapability, ServingTopologyKind, SupportLevel, DEFAULT_ENGINE_TYPE, KNOWN_ENGINE_TYPES,
};
pub use cluster::ClusterStatus;
pub use compat::{
    default_compatibility_rules, evaluate_compatibility, CompatCheckInput, CompatVerdict,
    CompatibilityRule, PlacementRejectReason,
};
pub use endpoint::{EndpointInfo, EndpointKind, EndpointStats, EndpointStatus};
pub use engine_args::{build_engine_extra_args, build_engine_extra_args_lenient};
pub use engine_image::{EngineImage, ImagePullStatus, NodeImageStatus, VersionPolicy};
pub use execution_context::{
    build_execution_context, inject_execution_context, ExecutionContext, HEADER_BUDGET_TOKENS,
    HEADER_DEADLINE_MS, HEADER_PRIORITY, HEADER_REQUEST_ID, HEADER_SESSION_ID, HEADER_TENANT_ID,
};
pub use json_model::{
    peek_json_model_field, rewrite_json_model_field, HEADER_NEBULA_MODEL, HEADER_NEBULA_MODEL_UID,
};
pub use model_cache::{
    AlertType, DiskAlert, DownloadPhase, DownloadProgress, ModelCacheEntry, NodeDiskStatus,
};
pub use model_deployment::{DesiredState, ModelDeployment};
pub use model_request::*;
pub use model_spec::{ModelSource, ModelSpec};
pub use model_template::{ModelTemplate, TemplateCategory, TemplateSource};
pub use node_status::{
    image_platforms_match, resolve_node_platform, GpuStatus, NodeStatus, DEFAULT_NODE_PLATFORM,
};
pub use placement::{next_placement_version, PlacementAssignment, PlacementPlan};
pub use capacity::{build_capacity_snapshot, CapacitySnapshot, ModelCapacityRow};
pub use slo::{
    evaluate_slo, DiagnosticEvent, ModelSlo, SloComplianceStatus, SloEvaluation, SloMetricSample,
    SloSuggestion,
};
pub use benchmark::{
    build_profile_from_runs, builtin_workloads, canary_should_rollback, recommend_from_profiles,
    BenchmarkRun, BenchmarkRunStatus, BenchmarkWorkload, CanaryRelease, CanaryState,
    PerformanceProfile, ProfileKey, RecommendCandidate, RecommendConfidence, RecommendRequest,
    RecommendResponse, WorkloadClass,
};
pub use selection::{
    draft_from_candidate, select_backends, switching_cost, BackendCandidate, CurrentBackend,
    DeploymentDraft, DraftRequest, ModelArchitecture, ModelProfile, SelectionConstraints,
    SelectionPreference, SelectionRequest, SelectionResponse, WorkloadHint,
};
pub use tenant::{
    admit_static, estimate_cost, summarize_usage, usage_window_start_ms, AdmitDecision,
    CostPriceConfig, Tenant, TenantCostSummary, TenantDenyBreakdown, TenantDenyCode, TenantQuota,
    UsageWindow,
};
pub use admission::{rate_limit_key, TenantAdmission};

pub mod args;
pub mod auth;
pub mod dual_write;
pub mod http;
pub mod telemetry;
pub use args::{parse_etcd_endpoints, CommonArgs};
pub use dual_write::DualWriteEmitter;
pub use http::{
    audit_http_client, build_http_client, control_plane_http_client, health_http_client,
    proxy_http_client, HttpClientOptions,
};
