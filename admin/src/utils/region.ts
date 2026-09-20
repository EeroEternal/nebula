/**
 * 多地点（Region）支持：当前 region 状态与 DOMAIN_API 前缀改写。
 * 通过统一入口（PowerLLM Console）访问时，站点 API 统一走
 * `${origin}/api/regions/{region}/v1/...` 代理前缀；单 region / 未接入 Console 时保持现状。
 * 设计见 docs/dev/region.md。
 */

const REGION_KEY = 'powerllm_region';
const REGION_ENABLED_KEY = 'powerllm_regions_enabled';

export interface RegionSummary {
  available: boolean;
  gpu_count: number | null;
  gpu_vram_total: number | null;
  gpu_vram_available: number | null;
  gpu_usage_ratio: number | null;
  model_replicas: number | null;
  detail?: string | null;
}

export interface RegionInfo {
  name: string;
  display_name?: string;
  endpoint?: string;
  health?: {
    status: 'online' | 'offline' | 'unknown';
    checked_at: number | null;
    summary?: RegionSummary;
  };
}

export function isRegionModeEnabled(): boolean {
  return localStorage.getItem(REGION_ENABLED_KEY) === '1';
}

export function setRegionModeEnabled(enabled: boolean): void {
  if (enabled) {
    localStorage.setItem(REGION_ENABLED_KEY, '1');
  } else {
    localStorage.removeItem(REGION_ENABLED_KEY);
  }
}

export function getCurrentRegion(): string {
  return localStorage.getItem(REGION_KEY) || '';
}

export function setCurrentRegion(region: string): void {
  if (region) {
    localStorage.setItem(REGION_KEY, region);
  } else {
    localStorage.removeItem(REGION_KEY);
  }
}

/** region 模式下把 DOMAIN_API 改写为 Console 代理前缀（在 global.tsx 中、任何请求发出前调用） */
export function applyRegionToDomainApi(): void {
  const region = getCurrentRegion();
  if (!isRegionModeEnabled() || !region) {
    return;
  }
  const base = `${window.location.origin}/api/regions/${region}`;
  window.DOMAIN_API_RAW = base;
  window.DOMAIN_API = `${base}/v1`;
}

export interface RegionUpsert {
  name?: string;
  endpoint?: string;
  display_name?: string;
  service_token?: string;
}

function getConsoleAdminToken(): string {
  try {
    const fromWindow = (window as Window & { CONSOLE_TOKEN?: string }).CONSOLE_TOKEN;
    if (fromWindow) {
      return fromWindow;
    }
    return localStorage.getItem('powerllm_console_token') || '';
  } catch {
    return '';
  }
}

function consoleHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    Accept: 'application/json',
    'Content-Type': 'application/json',
  };
  const consoleToken = getConsoleAdminToken();
  if (consoleToken) {
    headers.Authorization = `Bearer ${consoleToken}`;
    return headers;
  }
  try {
    const user = JSON.parse(localStorage.getItem('user') || '{}');
    if (user?.token && user?.token_type) {
      headers.Authorization = `${user.token_type} ${user.token}`;
    }
  } catch {
    // ignore
  }
  return headers;
}

async function parseOrThrow(resp: Response) {
  const data = await resp.json().catch(() => ({}));
  if (!resp.ok) {
    throw new Error(data?.detail || `HTTP ${resp.status}`);
  }
  return data;
}

export async function createRegion(body: RegionUpsert): Promise<RegionInfo> {
  const resp = await fetch(`${window.location.origin}/api/console/regions`, {
    method: 'POST',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function updateRegion(name: string, body: RegionUpsert): Promise<RegionInfo> {
  const resp = await fetch(`${window.location.origin}/api/console/regions/${name}`, {
    method: 'PATCH',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function deleteRegion(name: string): Promise<void> {
  const resp = await fetch(`${window.location.origin}/api/console/regions/${name}`, {
    method: 'DELETE',
    headers: consoleHeaders(),
  });
  await parseOrThrow(resp);
}

/**
 * 拉取切换器用的轻量 region 状态（不含 endpoint）。
 * 未接入 Console（404 等）时返回 null。
 */
export async function fetchRegionStatus(): Promise<RegionInfo[] | null> {
  try {
    const resp = await fetch(`${window.location.origin}/api/console/regions/status`, {
      headers: { Accept: 'application/json' },
    });
    if (!resp.ok) {
      return null;
    }
    const data = await resp.json();
    return Array.isArray(data) ? data : null;
  } catch {
    return null;
  }
}

/** 拉取 Console 的完整 region 列表（含 endpoint）；需 Console admin token（若已配置） */
export async function fetchRegions(): Promise<RegionInfo[] | null> {
  try {
    const resp = await fetch(`${window.location.origin}/api/console/regions`, {
      headers: consoleHeaders(),
    });
    if (!resp.ok) {
      return null;
    }
    const data = await resp.json();
    return Array.isArray(data) ? data : null;
  } catch {
    return null;
  }
}
