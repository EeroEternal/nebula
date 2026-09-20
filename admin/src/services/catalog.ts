import request from '@/utils/request';

export type CatalogSourceType = 'local' | 's3' | 'harbor' | 'modelscope';

export interface CatalogSource {
  type: CatalogSourceType;
  endpoint?: string;
  bucket?: string;
  prefix?: string;
  registry?: string;
  project?: string;
  tag?: string;
  model_id?: string;
  revision?: string;
  root_path?: string;
}

export interface CatalogEntry {
  id: string;
  model_name: string;
  version: string;
  status: string;
  hardware_tags?: string[];
  source: CatalogSource;
  local_path?: string | null;
  warmup_status?: string;
  warmup_error?: string | null;
  warmup_progress?: number;
  warmup_message?: string | null;
  created_at?: string;
  updated_at?: string;
}

export async function listCatalogEntries() {
  return request('/catalog/entries', { method: 'GET' });
}

export async function createCatalogEntry(data: {
  model_name: string;
  version: string;
  hardware_tags?: string[];
  source: CatalogSource;
}) {
  return request('/catalog/entries', { method: 'POST', data });
}

export async function warmupCatalogEntry(entryId: string) {
  return request(`/catalog/entries/${entryId}/warmup`, { method: 'POST' });
}

export async function getCatalogEntry(entryId: string) {
  return request(`/catalog/entries/${entryId}`, { method: 'GET' });
}

export async function deleteCatalogEntry(entryId: string) {
  return request(`/catalog/entries/${entryId}`, { method: 'DELETE' });
}
