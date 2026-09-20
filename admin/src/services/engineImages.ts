import request from '@/utils/request';

export interface EngineImageSpec {
  engine: string;
  version: string;
  image: string;
  cuda?: string;
  description?: string;
}

export type EnginePullStatus =
  | 'pending'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface EnginePullProgress {
  task_id: string;
  engine: string;
  version: string;
  image: string;
  status: EnginePullStatus;
  progress: number;
  message?: string;
  attempt?: number;
  max_attempts?: number;
  error?: string | null;
  target?: string | null;
  kind?: string;
  source?: string | null;
  bytes_done?: number;
  bytes_total?: number;
  phase?: string;
  tar_path?: string | null;
  created_at?: number | null;
  relay_tasks?: EnginePullProgress[];
}

export interface EngineProbeResult {
  ok: boolean;
  image?: string;
  target?: string;
  message?: string;
  registry?: string;
  repository?: string;
  tag?: string;
}

export type EngineRuntimeKind = 'docker' | 'k8s';

export interface EngineNodeInfo {
  node_id: string;
  role: string;
  label?: string;
  ip_address?: string;
  runtime?: string | null;
}

export interface EngineRuntimeMismatch {
  runtime_mismatch?: boolean;
  runtimes?: Array<{
    node_id?: string;
    role?: string;
    label?: string;
    runtime?: string;
  }>;
}

export interface EngineNodeImages {
  node_id: string;
  role: string;
  runtime?: string;
  images: string[];
  error?: string | null;
  ip_address?: string;
  label?: string;
}

export interface LocalEngineImagesResult {
  runtime?: string;
  nodes: EngineNodeImages[];
  images: string[];
  /** true when response is supervisor-only (quick first paint) */
  partial?: boolean;
  runtime_mismatch?: boolean;
  runtimes?: EngineRuntimeMismatch['runtimes'];
}

export interface RemoteEngineTag {
  name: string;
  image: string;
  engine?: string;
  full_size?: number | null;
  last_updated?: string;
  /** 服务端标记：本机（集群并集）已有 */
  local_present?: boolean;
  registry?: string;
}

export interface RemoteEngineTagsResult {
  registry?: string;
  repository?: string;
  page?: number;
  page_size?: number;
  count?: number;
  next?: string | boolean | null;
  engine?: string;
  synced?: boolean;
  updated_at?: number | null;
  tags: RemoteEngineTag[];
  errors?: Record<string, string>;
}

export async function listEngineImages() {
  return request('/engine-images', { method: 'GET' });
}

export interface EngineCompatWorker {
  worker_address?: string;
  accelerator_kind?: string;
  machine_model?: string;
  allowed_engines?: string[];
}

export interface EngineCompatResult {
  matrix?: Record<string, string[]>;
  local?: {
    accelerator_kind?: string;
    machine_model?: string;
    allowed_engines?: string[];
  };
  workers?: EngineCompatWorker[];
}

export async function getEngineCompat(params?: { worker?: string }) {
  return request<EngineCompatResult>('/engine-images/compat', {
    method: 'GET',
    params,
  });
}

export async function listLocalEngineImages(params?: {
  runtime?: EngineRuntimeKind;
  prefix?: string;
  force?: boolean;
  /** supervisor-only scan for fast first paint */
  quick?: boolean;
}) {
  return request('/engine-images/local', { method: 'GET', params });
}

export async function listEngineNodes() {
  return request<
    { nodes: EngineNodeInfo[] } & EngineRuntimeMismatch
  >('/engine-images/nodes', { method: 'GET' });
}

export async function listRemoteEngineTags(params?: {
  engine?: string;
  q?: string;
  page?: number;
  page_size?: number;
  /** ready | missing | all — 服务端按 local_present 筛选 */
  local?: string;
}) {
  return request('/engine-images/remote-tags', { method: 'GET', params });
}

export interface RemoteSyncProgress {
  task_id?: string | null;
  status?: 'idle' | 'running' | 'succeeded' | 'failed' | string;
  message?: string;
  engine?: string;
  engines_done?: number;
  engines_total?: number;
  tag_count?: number;
  errors?: Record<string, string>;
  engines?: Record<string, number>;
  updated_at?: number | null;
}

/** Start supervisor-side registry sync (HTTP happens on supervisor). */
export async function syncRemoteEngineTags() {
  return request('/engine-images/remote-sync', { method: 'POST' });
}

export async function getRemoteSyncProgress() {
  return request('/engine-images/remote-sync/progress', { method: 'GET' });
}

export async function probeEngineImage(data: {
  image: string;
  target?: string;
}) {
  return request('/engine-images/probe', {
    method: 'POST',
    data,
  });
}

export async function pullEngineImage(data: {
  engine: string;
  version?: string;
  image?: string;
  runtime?: EngineRuntimeKind;
  target?: string;
  wait?: boolean;
  relay_dests?: string[];
  keep_source_on_relay?: boolean;
}) {
  return request('/engine-images/pull', {
    method: 'POST',
    data: {
      wait: false,
      ...data,
    },
  });
}

export async function deleteEngineImage(data: {
  image: string;
  targets: string[];
  runtime?: EngineRuntimeKind;
  wait?: boolean;
  engine?: string;
  version?: string;
}) {
  return request('/engine-images/delete', {
    method: 'POST',
    data: {
      wait: false,
      ...data,
    },
  });
}

export async function migrateEngineImage(data: {
  image: string;
  source: string;
  dest: string;
  source_runtime?: EngineRuntimeKind;
  dest_runtime?: EngineRuntimeKind;
  keep_source?: boolean;
  wait?: boolean;
  engine?: string;
  version?: string;
}) {
  return request('/engine-images/migrate', {
    method: 'POST',
    data: {
      wait: false,
      keep_source: true,
      ...data,
    },
  });
}
export async function listEnginePullTasks() {
  return request('/engine-images/pull', { method: 'GET' });
}

export async function getEnginePullProgress(taskId: string) {
  return request(`/engine-images/pull/${taskId}`, { method: 'GET' });
}

export async function cancelEnginePull(taskId: string) {
  return request(`/engine-images/pull/${taskId}/cancel`, { method: 'POST' });
}

export async function retryEnginePull(taskId: string) {
  return request(`/engine-images/pull/${taskId}/retry`, { method: 'POST' });
}

export async function deleteEnginePull(taskId: string) {
  return request(`/engine-images/pull/${taskId}`, { method: 'DELETE' });
}

export async function clearEnginePullTasks() {
  return request('/engine-images/pull/clear', { method: 'POST' });
}

export interface EngineRegistryConfig {
  registry: string;
  repos: Record<string, string>;
  username?: string;
  has_password?: boolean;
  source?: 'file' | 'env' | string;
}

export async function getEngineRegistry() {
  return request('/engine-images/registry', { method: 'GET' });
}

export async function updateEngineRegistry(data: {
  registry: string;
  repos: Record<string, string>;
  username?: string;
  password?: string;
}) {
  return request('/engine-images/registry', { method: 'PUT', data });
}

export async function resetEngineRegistry() {
  return request('/engine-images/registry/reset', { method: 'POST' });
}

export async function manualRegisterEngineImage(data: {
  image: string;
  username?: string;
  password?: string;
  runtime?: EngineRuntimeKind;
  target?: string;
  wait?: boolean;
}) {
  return request('/engine-images/manual-register', {
    method: 'POST',
    data: {
      wait: false,
      ...data,
    },
  });
}
