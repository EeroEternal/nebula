export type HamiStockRow = {
  free_mem?: number | null;
  free_core?: number | null;
  product?: string | null;
  capacity_ready?: boolean;
};

export type HamiDeviceReq = {
  n_gpu?: number | string;
  gpu_mem_gb?: number;
  gpu_cores?: number;
  gpu_type?: string;
};

export type HamiQuotaResult = {
  ok: boolean;
  reason?: 'capacity' | 'mem' | 'core';
  needMemMib?: number;
  needCores?: number;
  fits?: number;
};

function nGpuOf(d: HamiDeviceReq): number {
  const n = d.n_gpu;
  if (n == null || n === '' || n === 'auto') return 1;
  const v = Number(n);
  return Number.isFinite(v) && v > 0 ? v : 1;
}

/** 对照库存判断四元组能否放下。未填显存则不拦。 */
export function evaluateHamiQuota(
  devices: HamiDeviceReq[] | undefined,
  rows: HamiStockRow[] | undefined,
): HamiQuotaResult {
  const reqs = (devices || []).filter((d) => d && Number(d.gpu_mem_gb) > 0);
  if (!reqs.length) return { ok: true };
  const stock = rows || [];
  if (!stock.some((r) => r.capacity_ready)) {
    return { ok: false, reason: 'capacity' };
  }
  let needMem = 0;
  let needCores = 0;
  let typeFilter = '';
  for (const d of reqs) {
    const n = nGpuOf(d);
    needMem += n * Number(d.gpu_mem_gb) * 1024;
    if (d.gpu_cores) needCores += n * Number(d.gpu_cores);
    if (d.gpu_type) typeFilter = String(d.gpu_type);
  }
  const filtered = typeFilter
    ? stock.filter((r) => !r.product || String(r.product) === typeFilter)
    : stock;
  const ready = filtered.filter((r) => r.capacity_ready);
  const freeMem = ready.reduce((s, r) => s + Number(r.free_mem || 0), 0);
  const freeCore = ready.reduce((s, r) => s + Number(r.free_core || 0), 0);
  if (needMem > freeMem) {
    return { ok: false, reason: 'mem', needMemMib: needMem, fits: 0 };
  }
  if (needCores > 0 && needCores > freeCore) {
    return { ok: false, reason: 'core', needCores, fits: 0 };
  }
  const fitsMem = needMem > 0 ? Math.floor(freeMem / needMem) : undefined;
  const fitsCore = needCores > 0 ? Math.floor(freeCore / needCores) : undefined;
  const fits =
    fitsMem == null
      ? fitsCore
      : fitsCore == null
        ? fitsMem
        : Math.min(fitsMem, fitsCore);
  return { ok: true, needMemMib: needMem, needCores, fits };
}
