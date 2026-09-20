import request from '@/utils/request';

export type ModelDownloadStatus =
  | 'pending'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface ModelDownloadNode {
  node_id: string;
  role?: string;
  label?: string;
  ip_address?: string;
}

export interface ModelDownloadProgress {
  task_id: string;
  model_type: string;
  model_name: string;
  status: ModelDownloadStatus;
  progress: number;
  message?: string;
  attempt?: number;
  max_attempts?: number;
  error?: string | null;
  target?: string | null;
  kind?: 'download' | 'transfer' | 'migrate' | 'copy' | string;
  source?: string | null;
  local_path?: string | null;
  model_version?: string | null;
  model_format?: string | null;
  model_size_in_billions?: number | string | null;
  quantization?: string | null;
  download_hub?: string | null;
  bytes_done?: number;
  bytes_total?: number;
  created_at?: number | null;
}

export async function listModelDownloadNodes() {
  return request('/model-downloads/nodes', { method: 'GET' });
}

export async function listModelDownloadLocal(params?: { model_name?: string }) {
  return request('/model-downloads/local', { method: 'GET', params });
}

export async function listModelDownloadTasks() {
  return request('/model-downloads/pull', { method: 'GET' });
}

export async function pullModelDownload(data: {
  model_type: string;
  model_name: string;
  model_format?: string;
  model_size_in_billions?: number | string;
  quantization?: string;
  download_hub?: string;
  model_version?: string;
  target?: string;
  wait?: boolean;
}) {
  return request('/model-downloads/pull', {
    method: 'POST',
    data: {
      wait: false,
      ...data,
    },
  });
}

export async function getModelDownloadProgress(taskId: string) {
  return request(`/model-downloads/pull/${taskId}`, { method: 'GET' });
}

export async function cancelModelDownload(taskId: string) {
  return request(`/model-downloads/pull/${taskId}/cancel`, { method: 'POST' });
}

export async function deleteModelDownloadTask(taskId: string) {
  return request(`/model-downloads/pull/${taskId}`, { method: 'DELETE' });
}

export async function clearModelDownloadTasks() {
  return request('/model-downloads/pull/clear', { method: 'POST' });
}

export async function retryModelDownload(taskId: string) {
  return request(`/model-downloads/pull/${taskId}/retry`, { method: 'POST' });
}

export async function transferModelDownload(data: {
  model_type: string;
  model_name: string;
  source: string;
  dest: string;
  src_path?: string;
  model_version?: string;
  model_format?: string;
  model_size_in_billions?: number | string;
  quantization?: string;
  wait?: boolean;
  mode?: 'migrate' | 'copy' | 'transfer';
}) {
  return request('/model-downloads/transfer', {
    method: 'POST',
    data: {
      wait: false,
      ...data,
    },
  });
}
