import type { ValueType } from '@/types/Public/data';

export const IB_CACHE_KEY = 'powerllm.deploy.ib.env.v1';

export const IB_DEFAULT_ENV: Record<string, string> = {
  NCCL_IB_DISABLE: '0',
  NCCL_IB_HCA: 'mlx5',
  NCCL_SOCKET_IFNAME: '',
};

export const IB_MANAGED_KEYS = Object.keys(IB_DEFAULT_ENV);

export function loadIbEnvCache(): Record<string, string> | null {
  try {
    const raw = sessionStorage.getItem(IB_CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
    return Object.fromEntries(
      Object.entries(parsed).map(([k, v]) => [String(k), v == null ? '' : String(v)]),
    );
  } catch {
    return null;
  }
}

export function saveIbEnvCache(env: Record<string, string>) {
  try {
    sessionStorage.setItem(IB_CACHE_KEY, JSON.stringify(env));
  } catch {
    // ignore quota / private mode
  }
}

export function envsListToMap(list: ValueType[] | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  for (const item of list || []) {
    const k = String(item?.key || '').trim();
    if (!k) continue;
    out[k] = item.value == null ? '' : String(item.value);
  }
  return out;
}

export function envsMapToList(
  map: Record<string, string>,
  prev?: ValueType[],
): ValueType[] {
  const seen = new Set<string>();
  const out: ValueType[] = [];
  for (const item of prev || []) {
    const k = String(item?.key || '').trim();
    if (!k || !(k in map) || seen.has(k)) continue;
    seen.add(k);
    out.push({ ...item, key: k, value: map[k] });
  }
  for (const [k, v] of Object.entries(map)) {
    if (seen.has(k)) continue;
    out.push({ key: k, value: v });
  }
  return out;
}

export function applyIbEnv(current: ValueType[]) {
  const cache = loadIbEnvCache();
  const toApply =
    cache && Object.keys(cache).length > 0 ? cache : { ...IB_DEFAULT_ENV };
  const map = envsListToMap(current);
  const pre: Record<string, string> = {};
  const owned = Object.keys(toApply);
  for (const k of owned) {
    if (k in map) pre[k] = map[k];
  }
  return {
    next: envsMapToList({ ...map, ...toApply }, current),
    owned,
    pre,
  };
}

export function removeIbEnv(
  current: ValueType[],
  owned: string[],
  pre: Record<string, string>,
) {
  const map = envsListToMap(current);
  const cache: Record<string, string> = {};
  for (const k of owned) {
    if (k in map) cache[k] = map[k];
  }
  const nextMap = { ...map };
  for (const k of owned) {
    if (Object.prototype.hasOwnProperty.call(pre, k)) nextMap[k] = pre[k];
    else delete nextMap[k];
  }
  return {
    next: envsMapToList(nextMap, current),
    cache,
  };
}

export function snapshotOwned(
  current: ValueType[] | undefined,
  owned: string[],
): Record<string, string> {
  const map = envsListToMap(current);
  const snap: Record<string, string> = {};
  for (const k of owned) {
    if (k in map) snap[k] = map[k];
  }
  return snap;
}

export function inferIbOwnedKeys(current: ValueType[] | undefined): string[] {
  const map = envsListToMap(current);
  return IB_MANAGED_KEYS.filter((k) => k in map);
}

export function envsSuggestIbEnabled(envs: ValueType[] | undefined): boolean {
  const map = envsListToMap(envs);
  const raw = (map.NCCL_IB_DISABLE || '').trim().toLowerCase();
  return raw === '0' || raw === 'false' || raw === 'off';
}
