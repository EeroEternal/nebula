pub mod admission;
pub mod benchmark;
pub mod capability;
pub mod capacity;
pub mod cluster;
pub mod compat;
pub mod endpoint;
pub mod engine_args;
pub mod engine_image;
pub mod engine_metrics;
pub mod execution_context;
pub mod json_model;
pub mod model_cache;
pub mod model_deployment;
pub mod model_request;
pub mod model_spec;
pub mod model_template;
pub mod node_status;
pub mod placement;
pub mod pool;
pub mod queue;
pub mod selection;
pub mod slo;
pub mod tenant;

pub use admission::{rate_limit_key, TenantAdmission};
pub use benchmark::{
    build_profile_from_runs, builtin_workloads, canary_should_rollback, recommend_from_profiles,
    BenchmarkRun, BenchmarkRunStatus, BenchmarkWorkload, CanaryRelease, CanaryState,
    PerformanceProfile, ProfileKey, RecommendCandidate, RecommendConfidence, RecommendRequest,
    RecommendResponse, WorkloadClass,
};
pub use capability::{
    parse_version_tuple, resolve_engine_type, score_api_for_engine, static_capability,
    static_capability_sglang, static_capability_vllm, static_version_support,
    tool_calling_for_engine, validate_engine_and_config, validate_engine_type,
    validate_engine_version, validate_model_config, CapabilitySource, EngineCapability,
    EngineVersionSupport, ObservabilityCapability, ReplicaCapability, ServingTopologyKind,
    SupportLevel, DEFAULT_ENGINE_TYPE, KNOWN_ENGINE_TYPES,
};
pub use capacity::{build_capacity_snapshot, CapacitySnapshot, ModelCapacityRow};
pub use cluster::ClusterStatus;
pub use compat::{
    default_compatibility_rules, evaluate_compatibility, CompatCheckInput, CompatVerdict,
    CompatibilityRule, PlacementRejectReason,
};
pub use endpoint::{EndpointInfo, EndpointKind, EndpointStats, EndpointStatus};
pub use engine_args::{build_engine_extra_args, build_engine_extra_args_lenient};
pub use engine_image::{EngineImage, ImagePullStatus, NodeImageStatus, VersionPolicy};
pub use engine_metrics::{
    diagnose_hybrid_prefix_cache, parse_engine_metrics, parse_engine_scrape_diagnostics,
    EngineScrapeDiagnostics,
};
pub use execution_context::{
    build_execution_context, inject_execution_context, parse_and_sanitize_inference_hint,
    ExecutionContext, InferenceHint, HEADER_BUDGET_TOKENS, HEADER_DEADLINE_MS, HEADER_HINT_TRUSTED,
    HEADER_INFERENCE_HINT, HEADER_INTERNAL_AUTH, HEADER_PRIORITY, HEADER_REPLICA_ID,
    HEADER_REQUEST_ID, HEADER_SESSION_ID, HEADER_TENANT_ID,
};
pub use json_model::{
    peek_json_model_field, rewrite_json_model_field, HEADER_NEBULA_MODEL, HEADER_NEBULA_MODEL_UID,
};
pub use model_cache::{
    AlertType, DiskAlert, DownloadPhase, DownloadProgress, EngineAlertType, EngineProbeAlert,
    ModelCacheEntry, NodeDiskStatus,
};
pub use model_deployment::{
    approved_pd_images_from_env, controller_addr, effective_replica_config,
    effective_replica_image, hardware_family_of_platform, kv_sharing_env, kv_transfer_config,
    lmcache_controller_env, merge_model_config, validate_pd_split_image,
    validate_pd_split_with_context, validate_replica_specs, wait_for_lmcache_controller,
    DeploymentTopology, DesiredState, KvSharingSpec, ModelDeployment, ReplicaPlacementSpec,
    ReplicaRole,
};
pub use model_request::*;
pub use model_spec::{ModelSource, ModelSpec};
pub use model_template::{ModelTemplate, TemplateCategory, TemplateSource};
pub use node_status::{
    image_platforms_match, resolve_node_platform, GpuStatus, NodeStatus, DEFAULT_NODE_PLATFORM,
};
pub use placement::{next_placement_version, PlacementAssignment, PlacementPlan};
pub use pool::{HardwarePool, PoolRole};
pub use selection::{
    draft_from_candidate, select_backends, switching_cost, BackendCandidate, CurrentBackend,
    DeploymentDraft, DraftRequest, ModelArchitecture, ModelProfile, SelectionConstraints,
    SelectionPreference, SelectionRequest, SelectionResponse, WorkloadHint,
    ROOFLINE_COMPUTE_BOUND_FP8_REASON, ROOFLINE_MEMORY_BOUND_4BIT_REASON,
    ROOFLINE_NATIVE_W4A4_REASON,
};
pub use slo::{
    evaluate_slo, evaluate_slo_with_pools, DiagnosticEvent, ModelSlo, PdPoolTelemetry,
    SloComplianceStatus, SloEvaluation, SloMetricSample, SloSuggestion,
};
pub use tenant::{
    admit_static, estimate_cost, summarize_usage, usage_window_start_ms, AdmitDecision,
    CostPriceConfig, Tenant, TenantCostSummary, TenantDenyBreakdown, TenantDenyCode, TenantQuota,
    UsageWindow,
};

pub mod args;
pub mod auth;
pub mod dual_write;
pub mod http;
pub mod telemetry;
pub use args::{parse_etcd_endpoints, CommonArgs};
pub use dual_write::DualWriteEmitter;
pub use http::{
    audit_http_client, build_http_client, control_plane_http_client, first_byte_timeout,
    first_byte_total_budget, health_http_client, proxy_http_client, stream_idle_timeout,
    ChunkDeadline, HttpClientOptions, DEFAULT_FIRST_BYTE_TOTAL_BUDGET_MS,
    DEFAULT_PROXY_FIRST_BYTE_TIMEOUT_MS, DEFAULT_STREAM_IDLE_TIMEOUT_MS,
};
