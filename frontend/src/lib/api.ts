const BASE_URL = import.meta.env.VITE_BFF_BASE_URL || '/api'

function buildHeaders(token?: string, json = true) {
  const headers: Record<string, string> = {}
  if (json) {
    headers['Content-Type'] = 'application/json'
  }
  if (token) {
    headers.Authorization = `Bearer ${token}`
  }
  return headers
}

export async function apiGet<T>(path: string, token?: string): Promise<T> {
  const resp = await fetch(`${BASE_URL}${path}`, {
    headers: buildHeaders(token, false),
  })

  if (!resp.ok) {
    const text = await resp.text()
    throw new Error(text || `Request failed: ${resp.status}`)
  }

  return (await resp.json()) as T
}

export async function apiPost<T, Body>(
  path: string,
  body: Body,
  token?: string
): Promise<T> {
  const resp = await fetch(`${BASE_URL}${path}`, {
    method: 'POST',
    headers: buildHeaders(token),
    body: JSON.stringify(body),
  })

  if (!resp.ok) {
    const text = await resp.text()
    throw new Error(text || `Request failed: ${resp.status}`)
  }

  return (await resp.json()) as T
}

export async function apiGetWithParams<T>(
  path: string,
  params: Record<string, string>,
  token?: string
): Promise<T> {
  const query = new URLSearchParams(params).toString()
  const resp = await fetch(`${BASE_URL}${path}?${query}`, {
    headers: buildHeaders(token, false),
  })

  if (!resp.ok) {
    const text = await resp.text()
    throw new Error(text || `Request failed: ${resp.status}`)
  }

  return (await resp.json()) as T
}

export async function apiPut<T, Body>(
  path: string,
  body: Body,
  token?: string
): Promise<T> {
  const resp = await fetch(`${BASE_URL}${path}`, {
    method: 'PUT',
    headers: buildHeaders(token),
    body: JSON.stringify(body),
  })

  if (!resp.ok) {
    const text = await resp.text()
    throw new Error(text || `Request failed: ${resp.status}`)
  }

  return (await resp.json()) as T
}

export async function apiDelete<T>(path: string, token?: string): Promise<T> {
  const resp = await fetch(`${BASE_URL}${path}`, {
    method: 'DELETE',
    headers: buildHeaders(token, false),
  })

  if (!resp.ok) {
    const text = await resp.text()
    throw new Error(text || `Request failed: ${resp.status}`)
  }

  return (await resp.json()) as T
}

// ---------------------------------------------------------------------------
// v2 API convenience functions
// ---------------------------------------------------------------------------
// The v2 routes are mounted at /api/v2 on the BFF. Since BASE_URL is /api,
// we simply prefix paths with /v2 so the final URL becomes /api/v2/...

import type {
  AuthUser,
  CompatibilityRule,
  CreateUserPayload,
  DiagnosticEvent,
  BenchmarkRun,
  RecommendRequest,
  RecommendResponse,
  SelectionResponse,
  DeploymentDraft,
  CanaryRelease,
  Tenant,
  TenantCostSummary,
  CostPriceConfig,
  TenantQuota,
  GatewayLatency,
  GatewayOverview,
  GatewayProtection,
  GatewayReliability,
  GatewayTraffic,
  HardwareInventory,
  ManagedUser,
  ModelSlo,
  ModelView,
  ModelDetailView,
  ModelTemplate,
  DiskAlert,
  LoginResponse,
  SloEvaluation,
  UpdateUserPayload,
  UserSettings,
} from '@/lib/types'

export const authApi = {
  login: (username: string, password: string) =>
    apiPost<LoginResponse, { username: string; password: string }>('/auth/login', { username, password }),

  logout: (token?: string) =>
    apiPost<{ ok: boolean }, Record<string, never>>('/auth/logout', {}, token),

  me: (token?: string) =>
    apiGet<AuthUser>('/auth/me', token),

  updateProfile: (body: { display_name?: string; email?: string }, token?: string) =>
    apiPut<{ ok: boolean }, { display_name?: string; email?: string }>('/auth/profile', body, token),

  getSettings: (token?: string) =>
    apiGet<UserSettings>('/auth/settings', token),

  updateSettings: (body: Partial<UserSettings>, token?: string) =>
    apiPut<{ ok: boolean }, Partial<UserSettings>>('/auth/settings', body, token),

  listUsers: (token?: string) =>
    apiGet<ManagedUser[]>('/auth/users', token),

  createUser: (body: CreateUserPayload, token?: string) =>
    apiPost<{ ok: boolean; id: string }, CreateUserPayload>('/auth/users', body, token),

  updateUser: (id: string, body: UpdateUserPayload, token?: string) =>
    apiPut<{ ok: boolean }, UpdateUserPayload>(`/auth/users/${id}`, body, token),

  deleteUser: (id: string, token?: string) =>
    apiDelete<{ ok: boolean }>(`/auth/users/${id}`, token),
}

export const v2 = {
  listModels: (token?: string) =>
    apiGet<ModelView[]>('/v2/models', token),

  getModel: (uid: string, token?: string) =>
    apiGet<ModelDetailView>(`/v2/models/${uid}`, token),

  createModel: (body: Record<string, unknown>, token?: string) =>
    apiPost<unknown, Record<string, unknown>>('/v2/models', body, token),

  updateModel: (uid: string, body: Record<string, unknown>, token?: string) =>
    apiPut<unknown, Record<string, unknown>>(`/v2/models/${uid}`, body, token),

  startModel: (uid: string, body?: Record<string, unknown>, token?: string) =>
    apiPost<unknown, Record<string, unknown>>(`/v2/models/${uid}/start`, body || {}, token),

  stopModel: (uid: string, token?: string) =>
    apiPost<unknown, Record<string, unknown>>(`/v2/models/${uid}/stop`, {}, token),

  deleteModel: (uid: string, token?: string) =>
    apiDelete<unknown>(`/v2/models/${uid}`, token),

  scaleModel: (uid: string, replicas: number, token?: string) =>
    apiPut<unknown, { replicas: number }>(`/v2/models/${uid}/scale`, { replicas }, token),

  listTemplates: (token?: string) =>
    apiGet<ModelTemplate[]>('/v2/templates', token),

  deployTemplate: (id: string, body: Record<string, unknown>, token?: string) =>
    apiPost<unknown, Record<string, unknown>>(`/v2/templates/${id}/deploy`, body, token),

  listAlerts: (token?: string) =>
    apiGet<DiskAlert[]>('/v2/alerts', token),

  gatewayOverview: (window: string, token?: string) =>
    apiGetWithParams<GatewayOverview>('/v2/observability/gateway/overview', { window }, token),

  gatewayTraffic: (window: string, token?: string) =>
    apiGetWithParams<GatewayTraffic>('/v2/observability/gateway/traffic', { window }, token),

  gatewayReliability: (window: string, token?: string) =>
    apiGetWithParams<GatewayReliability>('/v2/observability/gateway/reliability', { window }, token),

  gatewayProtection: (window: string, token?: string) =>
    apiGetWithParams<GatewayProtection>('/v2/observability/gateway/protection', { window }, token),

  gatewayLatency: (window: string, token?: string) =>
    apiGetWithParams<GatewayLatency>('/v2/observability/gateway/latency', { window }, token),

  listCompatRules: (token?: string) =>
    apiGet<CompatibilityRule[]>('/v2/compat', token),

  putCompatRule: (rule: CompatibilityRule, token?: string) =>
    apiPut<CompatibilityRule, CompatibilityRule>('/v2/compat', rule, token),

  seedCompatRules: (token?: string) =>
    apiPost<CompatibilityRule[], Record<string, never>>('/v2/compat/seed', {}, token),

  deleteCompatRule: (id: string, token?: string) =>
    apiDelete<unknown>(`/v2/compat/${id}`, token),

  hardwareInventory: (token?: string) =>
    apiGet<HardwareInventory>('/v2/inventory/hardware', token),

  listSlos: (token?: string) =>
    apiGet<ModelSlo[]>('/v2/slos', token),

  getSlo: (modelUid: string, token?: string) =>
    apiGet<ModelSlo>(`/v2/slos/${modelUid}`, token),

  upsertSlo: (modelUid: string, body: Partial<ModelSlo>, token?: string) =>
    apiPut<ModelSlo, Partial<ModelSlo>>(`/v2/slos/${modelUid}`, body, token),

  evaluateSlo: (modelUid: string, token?: string) =>
    apiGet<SloEvaluation>(`/v2/slos/${modelUid}/evaluate`, token),

  listDiagnostics: (modelUid?: string, token?: string) =>
    apiGetWithParams<DiagnosticEvent[]>(
      '/v2/diagnostics/events',
      modelUid ? { model_uid: modelUid } : {},
      token,
    ),

  listBenchmarkRuns: (token?: string) =>
    apiGet<BenchmarkRun[]>('/v2/benchmarks/runs', token),

  recommendEngines: (body: RecommendRequest, token?: string) =>
    apiPost<RecommendResponse, RecommendRequest>('/v2/benchmarks/recommend', body, token),

  selectionRecommend: (
    body: {
      model: {
        profile_id: string
        model_uid?: string
        model_name: string
        architecture?: string
        parameter_billions?: number
        quantization?: string
        context_length?: number
      }
      workload?: { workload_id?: string; concurrency?: number; target_qps?: number }
      constraints?: {
        platform?: string
        ttft_p95_ms_max?: number
        throughput_tps_min?: number
        preference?: string
        max_candidates?: number
        allowed_engines?: string[]
      }
      current?: { engine_type?: string; image_id?: string; platform?: string }
    },
    token?: string,
  ) => apiPost<SelectionResponse, typeof body>('/v2/selection/recommend', body, token),

  selectionDraft: (
    body: {
      selection: {
        model: {
          profile_id: string
          model_uid?: string
          model_name: string
          architecture?: string
          parameter_billions?: number
          quantization?: string
          context_length?: number
        }
        workload?: { workload_id?: string; concurrency?: number; target_qps?: number }
        constraints?: {
          platform?: string
          ttft_p95_ms_max?: number
          throughput_tps_min?: number
          preference?: string
          max_candidates?: number
          allowed_engines?: string[]
        }
        current?: { engine_type?: string; image_id?: string; platform?: string }
      }
      candidate_index?: number
      model_uid?: string
      replicas?: number
    },
    token?: string,
  ) => apiPost<DeploymentDraft, typeof body>('/v2/selection/draft', body, token),

  selectionApply: (
    body: { draft: DeploymentDraft; upsert_spec?: boolean },
    token?: string,
  ) => apiPost<DeploymentDraft, typeof body>('/v2/selection/apply', body, token),

  listCanaries: (token?: string) =>
    apiGet<CanaryRelease[]>('/v2/canaries', token),

  createCanary: (
    body: {
      model_uid: string
      candidate_image_id: string
      stable_image_id?: string
      traffic_weight_percent?: number
    },
    token?: string,
  ) => apiPost<CanaryRelease, typeof body>('/v2/canaries', body, token),

  evaluateCanary: (id: string, slo_breaching: boolean, token?: string) =>
    apiPost<CanaryRelease, { slo_breaching: boolean }>(
      `/v2/canaries/${id}/evaluate`,
      { slo_breaching },
      token,
    ),

  promoteCanary: (id: string, token?: string) =>
    apiPost<CanaryRelease, Record<string, never>>(`/v2/canaries/${id}/promote`, {}, token),

  rollbackCanary: (id: string, reason?: string, token?: string) =>
    apiPost<CanaryRelease, { reason?: string }>(
      `/v2/canaries/${id}/rollback`,
      { reason },
      token,
    ),

  listTenants: (token?: string) => apiGet<Tenant[]>('/v2/tenants', token),

  upsertTenant: (
    body: {
      tenant_id: string
      display_name?: string
      enabled?: boolean
      quotas?: TenantQuota
      api_token_principals?: string[]
      priority_default?: number
    },
    token?: string,
  ) => apiPut<Tenant, typeof body>('/v2/tenants', body, token),

  deleteTenant: (tenantId: string, token?: string) =>
    apiDelete(`/v2/tenants/${tenantId}`, token),

  tenantCost: (tenantId: string, token?: string) =>
    apiGet<TenantCostSummary>(`/v2/tenants/${tenantId}/cost`, token),

  listPricing: (token?: string) => apiGet<CostPriceConfig[]>('/v2/pricing', token),

  upsertPricing: (
    body: {
      price_id: string
      engine_type: string
      platform?: string
      price_per_1k_input: number
      price_per_1k_output: number
      currency?: string
    },
    token?: string,
  ) => apiPut<CostPriceConfig, typeof body>('/v2/pricing', body, token),
}
