export interface GpuStatus {
  index: number
  memory_total_mb: number
  memory_used_mb: number
  temperature_c?: number | null
  utilization_gpu?: number | null
  name?: string | null
  driver_version?: string | null
  cuda_version?: string | null
}

export interface NodeStatus {
  node_id: string
  last_heartbeat_ms: number
  gpus: GpuStatus[]
  api_addr?: string | null
  platform?: string | null
}

export interface EndpointInfo {
  model_uid: string
  replica_id: number
  plan_version: number
  node_id: string
  endpoint_kind: string
  api_flavor: string
  status: string
  last_heartbeat_ms: number
  status_detail?: string | null
  grpc_target?: string | null
  base_url?: string | null
}

export interface PlacementAssignment {
  replica_id: number
  node_id: string
  engine_config_path: string
  port: number
  gpu_index?: number | null
  gpu_indices?: number[] | null
  extra_args?: string[] | null
  engine_type?: string | null
  docker_image?: string | null
}

export interface PlacementPlan {
  request_id?: string | null
  model_uid: string
  model_name?: string
  version: number
  assignments: PlacementAssignment[]
}

export interface ModelConfig {
  tensor_parallel_size?: number | null
  gpu_memory_utilization?: number | null
  max_model_len?: number | null
  required_vram_mb?: number | null
  lora_modules?: string[] | null
}

export interface ModelLoadRequest {
  model_name: string
  model_uid: string
  replicas?: number
  config?: ModelConfig | null
  node_id?: string | null
  gpu_index?: number | null
  gpu_indices?: number[] | null
  engine_type?: string | null
  docker_image?: string | null
}

export interface ModelRequest {
  id: string
  request: ModelLoadRequest
  status: unknown
  created_at_ms: number
}

export interface ModelSearchResult {
  id: string
  name: string
  author: string | null
  downloads: number
  likes: number
  tags: string[]
  pipeline_tag: string | null
  source: string
}

export interface EndpointStats {
  model_uid: string
  replica_id: number
  last_updated_ms: number
  pending_requests: number
  prefix_cache_hit_rate?: number | null
  prompt_cache_hit_rate?: number | null
  /** KV / cache occupancy ratio in [0, 1]. Absent means unsupported/unknown — not zero. */
  kv_cache_usage?: number | null
}

export interface ClusterStatus {
  nodes: NodeStatus[]
  endpoints: EndpointInfo[]
  placements: PlacementPlan[]
  model_requests: ModelRequest[]
}

export interface EngineImage {
  id: string
  engine_type: string
  image: string
  platforms: string[]
  version_policy: 'pin' | 'rolling'
  pre_pull: boolean
  description?: string | null
  created_at_ms: number
  updated_at_ms: number
}

export interface NodeImageStatus {
  node_id: string
  image_id: string
  image: string
  status: 'pending' | 'pulling' | 'ready' | 'failed'
  error?: string | null
  updated_at_ms: number
}

// ---------------------------------------------------------------------------
// v2 API types
// ---------------------------------------------------------------------------

export type AggregatedModelState = 'stopped' | 'downloading' | 'starting' | 'running' | 'degraded' | 'failed' | 'stopping'

export interface ReplicaCount {
  desired: number
  ready: number
  unhealthy: number
}

export interface ModelView {
  model_uid: string
  model_name: string
  engine_type: string | null
  state: AggregatedModelState
  replicas: ReplicaCount
  endpoints: EndpointInfo[]
  labels: Record<string, string>
  created_at_ms: number
  updated_at_ms: number
}

export interface DownloadProgress {
  model_uid: string
  replica_id: number
  node_id: string
  model_name: string
  phase: 'downloading' | 'verifying' | 'complete' | 'failed'
  total_bytes: number
  downloaded_bytes: number
  files_total: number
  files_done: number
  updated_at_ms: number
}

export interface DownloadProgressView {
  replicas: DownloadProgress[]
}

export interface CacheStatusView {
  cached_on_nodes: string[]
  total_size_bytes: number
}

export interface ModelDetailView {
  model_uid: string
  model_name: string
  engine_type: string | null
  state: AggregatedModelState
  replicas: ReplicaCount
  labels: Record<string, string>
  created_at_ms: number
  updated_at_ms: number
  spec: ModelSpec
  deployment: ModelDeployment | null
  placement: PlacementPlan | null
  endpoints: EndpointInfo[]
  stats: EndpointStats[]
  capabilities: ReplicaCapability[]
  download_progress: DownloadProgressView | null
  cache_status: CacheStatusView | null
}

export interface ReplicaCapability {
  model_uid: string
  replica_id: number
  updated_at_ms: number
  capability: {
    engine_type: string
    engine_version?: string | null
    source: string
    openai_compatible?: boolean | null
    observability?: {
      pending_requests?: string
      kv_cache_usage?: string
      prefix_cache_hit_rate?: string
      prompt_cache_hit_rate?: string
    }
    notes?: string | null
  }
}

export interface ModelSpec {
  model_uid: string
  model_name: string
  model_source: 'hugging_face' | 'model_scope' | 'local'
  model_path?: string | null
  engine_type?: string | null
  docker_image?: string | null
  config?: ModelConfig | null
  labels: Record<string, string>
  created_at_ms: number
  updated_at_ms: number
  created_by?: string | null
}

export interface ModelDeployment {
  model_uid: string
  desired_state: 'running' | 'stopped'
  replicas: number
  min_replicas?: number | null
  max_replicas?: number | null
  node_affinity?: string | null
  gpu_affinity?: number[] | null
  config_overrides?: ModelConfig | null
  version: number
  updated_at_ms: number
}

export interface ModelTemplate {
  template_id: string
  name: string
  description?: string | null
  category?: 'llm' | 'embedding' | 'rerank' | 'vlm' | 'audio' | null
  model_name: string
  model_source?: 'hugging_face' | 'model_scope' | 'local' | null
  engine_type?: string | null
  docker_image?: string | null
  config?: ModelConfig | null
  default_replicas: number
  labels: Record<string, string>
  source: 'system' | 'user' | 'saved'
  created_at_ms: number
  updated_at_ms: number
}

export interface ModelCacheEntry {
  node_id: string
  model_name: string
  cache_path: string
  size_bytes: number
  file_count: number
  complete: boolean
  last_accessed_ms: number
  discovered_at_ms: number
}

export interface NodeDiskStatus {
  node_id: string
  model_dir: string
  total_bytes: number
  used_bytes: number
  available_bytes: number
  usage_pct: number
  model_cache_bytes: number
  model_count: number
  updated_at_ms: number
}

export interface DiskAlert {
  node_id: string
  alert_type: 'disk_warning' | 'disk_critical'
  message: string
  model_dir: string
  usage_pct: number
  available_bytes: number
  created_at_ms: number
}

export type EngineAlertType =
  | 'oom_killed'
  | 'container_exited'
  | 'gpu_memory_pressure'
  | 'kv_cache_high'
  | 'health_probe_failed'

export interface EngineProbeAlert {
  node_id: string
  model_uid: string
  replica_id: number
  alert_type: EngineAlertType
  message: string
  exit_code?: number | null
  created_at_ms: number
}

export interface AlertsSummary {
  disk: DiskAlert[]
  engine: EngineProbeAlert[]
}

export interface GatewayTimePoint {
  ts: string
  value: number
}

export interface GatewayOverview {
  window: string
  /** Prometheus source; currently "router". */
  data_source?: string
  rps: number
  error_5xx_ratio: number
  retry_success_ratio: number
  circuit_open_count: number
}

export interface GatewayTraffic {
  window: string
  data_source?: string
  series: {
    requests_total: GatewayTimePoint[]
    responses_2xx: GatewayTimePoint[]
    responses_4xx: GatewayTimePoint[]
    responses_5xx: GatewayTimePoint[]
  }
}

export interface GatewayReliability {
  window: string
  data_source?: string
  series: {
    retry_total: GatewayTimePoint[]
    retry_success_total: GatewayTimePoint[]
    upstream_error_connect: GatewayTimePoint[]
    upstream_error_timeout: GatewayTimePoint[]
    upstream_error_5xx: GatewayTimePoint[]
    upstream_error_other: GatewayTimePoint[]
  }
}

export interface GatewayProtection {
  window: string
  data_source?: string
  request_too_large_count: number
  circuit_skipped_count: number
  circuit_open_count: number
}

export interface GatewayLatency {
  window: string
  /** Prometheus source; currently always "router" (`nebula_route_*`). */
  data_source?: string
  series: {
    latency_p50_ms: GatewayTimePoint[]
    latency_p95_ms: GatewayTimePoint[]
    latency_p99_ms: GatewayTimePoint[]
    ttft_p50_ms: GatewayTimePoint[]
    ttft_p95_ms: GatewayTimePoint[]
  }
}

export interface AuthUser {
  id: string
  username: string
  role: 'admin' | 'operator' | 'viewer'
  display_name?: string | null
  email?: string | null
}

export interface ManagedUser extends AuthUser {
  is_active: boolean
}

export interface LoginResponse {
  token: string
  expires_at: string
  user: AuthUser
}

export interface UserSettings {
  in_app_alerts: boolean
  email_alerts: boolean
}

export interface CreateUserPayload {
  username: string
  password: string
  role: 'admin' | 'operator' | 'viewer'
  display_name?: string
  email?: string
}

export interface UpdateUserPayload {
  role?: 'admin' | 'operator' | 'viewer'
  display_name?: string
  email?: string
  is_active?: boolean
  password?: string
}

export interface CompatibilityRule {
  id: string
  engine_type: string
  engine_version_min?: string | null
  engine_version_max?: string | null
  platforms: string[]
  min_driver_version?: string | null
  min_cuda_version?: string | null
  verdict: 'allow' | 'deny'
  known_issues?: string[]
  notes?: string | null
  updated_at_ms: number
}

export interface HardwareInventory {
  nodes: Array<{
    node_id: string
    platform?: string | null
    last_heartbeat_ms: number
    gpus: Array<{
      index: number
      name?: string | null
      driver_version?: string | null
      cuda_version?: string | null
      memory_total_mb: number
      memory_used_mb: number
      temperature_c?: number | null
      utilization_gpu?: number | null
      occupied_by?: string | null
    }>
  }>
  placements: Array<{
    model_uid: string
    replica_id: number
    node_id: string
    gpu_indices: number[]
  }>
}

export interface CapacitySnapshot {
  models: Array<{
    model_uid: string
    desired_state: string
    desired_replicas: number
    ready_replicas: number
    unhealthy_replicas: number
    pending_total: number
    avg_kv_usage?: number | null
    replica_gap: number
    hints: string[]
  }>
  gpu_total: number
  gpu_free: number
  hints: string[]
  evaluated_at_ms: number
}

export interface ModelSlo {
  model_uid: string
  availability_target?: number | null
  ttft_p95_ms?: number | null
  tpot_p95_ms?: number | null
  latency_p95_ms?: number | null
  throughput_tps?: number | null
  window: string
  exclude_abort_from_error_budget: boolean
  exclude_drain_from_error_budget: boolean
  notes?: string | null
  updated_at_ms: number
}

export interface SloEvaluation {
  model_uid: string
  window: string
  status: 'compliant' | 'breaching' | 'insufficient_data' | 'unknown'
  samples: Array<{ name: string; value?: number | null; data_source: string; unit: string }>
  breaches: string[]
  suggestions: Array<{ kind: string; message: string; target: string }>
  evaluated_at_ms: number
  abort_excluded: boolean
  drain_excluded: boolean
}

export interface DiagnosticEvent {
  ts_ms: number
  kind: string
  summary: string
  model_uid?: string | null
  node_id?: string | null
  data_source?: string | null
}

export interface BenchmarkRun {
  run_id: string
  profile_key: {
    model_name: string
    engine_type: string
    engine_version?: string | null
    platform?: string | null
    gpu_name?: string | null
    workload_id: string
    param_fingerprint?: string | null
  }
  status: string
  ttft_p95_ms?: number | null
  throughput_tps?: number | null
  error_rate?: number | null
  cost_per_1k_tokens?: number | null
  image_id?: string | null
  finished_at_ms: number
}

export interface RecommendRequest {
  model_name: string
  workload_id?: string
  platform?: string
  ttft_p95_ms_max?: number
  throughput_tps_min?: number
  budget_cost_per_1k?: number
  max_candidates?: number
}

export interface RecommendResponse {
  model_name: string
  status: string
  candidates: Array<{
    engine_type: string
    engine_version?: string | null
    image_id?: string | null
    platform?: string | null
    confidence: string
    evidence_run_ids: string[]
    ttft_p95_ms?: number | null
    throughput_tps?: number | null
    cost_per_1k_tokens?: number | null
    rationale: string
  }>
  message?: string | null
}

export interface SelectionResponse {
  model_name: string
  status: string
  candidates: Array<{
    engine_type: string
    engine_version?: string | null
    image_id?: string | null
    platform?: string | null
    confidence: string
    switching_cost: number
    score?: number
    score_breakdown?: string[]
    evidence_run_ids: string[]
    ttft_p95_ms?: number | null
    throughput_tps?: number | null
    cost_per_1k_tokens?: number | null
    reasons: string[]
  }>
  message?: string | null
}

export interface DeploymentDraft {
  model_uid: string
  model_name: string
  engine_type: string
  image_id?: string | null
  deployment: ModelDeployment
  candidate: SelectionResponse["candidates"][number]
  note: string
}

export interface CanaryRelease {
  canary_id: string
  model_uid: string
  stable_image_id?: string | null
  candidate_image_id: string
  traffic_weight_percent: number
  state: string
  slo_breach?: boolean | null
  rollback_reason?: string | null
  updated_at_ms: number
}

export interface TenantQuota {
  rps_per_minute?: number | null
  max_concurrency?: number | null
  max_tokens_per_minute?: number | null
  allowed_models?: string[] | null
}

export interface Tenant {
  tenant_id: string
  display_name: string
  enabled: boolean
  quotas: TenantQuota
  api_token_principals?: string[]
  priority_default?: number | null
  created_at_ms: number
  updated_at_ms: number
}

export interface CostPriceConfig {
  price_id: string
  engine_type: string
  platform?: string | null
  price_per_1k_input: number
  price_per_1k_output: number
  currency: string
  notes?: string | null
  updated_at_ms: number
}

export interface TenantCostSummary {
  tenant_id: string
  window: string
  requests: number
  input_tokens: number
  output_tokens: number
  denied_total: number
  deny_breakdown: {
    rps: number
    concurrency: number
    model: number
    token_budget: number
    disabled: number
  }
  cost_estimate?: number | null
  currency?: string | null
  windows_merged: number
}
