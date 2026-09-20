import { transformRate, bytesToGB } from '@/utils';
import { getUsageLevel } from '@/utils/usageColor';
import type { DeviceInfo } from '@/types/Public/data';

export type GpuRowStatus = 'ok' | 'warn' | 'crit' | 'idle' | 'offline';

export type GpuOccupant = {
  model_uid: string;
  model_name?: string;
  replica_uids?: string[];
};

export type GpuRow = {
  id: string;
  node: string;
  workerAddress: string;
  nodeStatus: DeviceInfo['status'] | 'online' | 'offline' | 'expired';
  idx: string;
  name: string;
  /** SM / compute util 0–100；未采集时为 null */
  util: number | null;
  /** 显存占用占比 0–100 */
  memUsage: number;
  memUsedGb: number;
  memTotalGb: number;
  /** @deprecated 兼容旧字段：等于 memUsage */
  usage: number;
  status: GpuRowStatus;
  acceleratorKind?: string;
  occupant?: GpuOccupant | null;
  /** 摄氏度；未采集 null */
  temperatureC?: number | null;
  /** 瓦；未采集 null */
  powerW?: number | null;
};

export type AlertItem = {
  id: string;
  level: 'warning' | 'error' | 'info';
  title: string;
  detail: string;
  time: string;
};

export type HeatmapRow = {
  id: string;
  label: string;
  cells: number[];
};

/** Normalize util that may be 0–1 or already 0–100. */
export function toUtilPct(raw: unknown): number | null {
  if (raw === undefined || raw === null || Number.isNaN(Number(raw))) return null;
  const v = Number(raw);
  if (v <= 1) return transformRate(v);
  return Math.round(Math.min(v, 100) * 100) / 100;
}

function classifyStatus(
  memUsage: number,
  util: number | null,
  gpuOffline: boolean,
  nodeOffline: boolean,
): GpuRowStatus {
  if (gpuOffline || nodeOffline) return 'offline';
  if (memUsage >= 90) return 'crit';
  if (memUsage >= 70) return 'warn';
  if (util !== null && util < 10 && memUsage >= 70) return 'warn';
  if (memUsage < 15 && (util === null || util < 10)) return 'idle';
  return 'ok';
}

/** Flatten device_info GPUs into matrix rows (util + mem split). */
export function flattenGpuRows(devices: DeviceInfo[] = []): GpuRow[] {
  return devices.flatMap((node) =>
    Object.entries(node.gpus || {}).map(([idx, gpu]) => {
      const memUsage = transformRate(gpu.mem_usage);
      const util = toUtilPct((gpu as { gpu_util?: number }).gpu_util);
      const status = classifyStatus(
        memUsage,
        util,
        gpu.status === 'offline' || gpu.status === 'expired',
        node.status === 'offline',
      );
      return {
        id: `${node.uuid}-${idx}`,
        node: node.name,
        workerAddress: node.worker_address,
        nodeStatus: node.status,
        idx: String(idx).replace(/^gpu-/, ''),
        name: gpu.name,
        util,
        memUsage,
        usage: memUsage,
        memUsedGb: bytesToGB(gpu.mem_used),
        memTotalGb: bytesToGB(gpu.mem_total),
        status,
        acceleratorKind: (node as { accelerator_kind?: string }).accelerator_kind,
        occupant: null,
        temperatureC: (gpu as { temperature_c?: number | null }).temperature_c ?? null,
        powerW: (gpu as { power_w?: number | null }).power_w ?? null,
      };
    }),
  );
}

/** Map cluster/gpus API rows → GpuRow. */
export function mapClusterGpuApiRows(
  results: Array<{
    id?: string;
    node?: string;
    worker_address?: string;
    gpu_idx?: string;
    gpu_index?: number | string;
    name?: string;
    gpu_util?: number | null;
    mem_used?: number;
    mem_total?: number;
    mem_usage_pct?: number;
    status?: GpuRowStatus;
    accelerator_kind?: string;
    occupant?: GpuOccupant | null;
    temperature_c?: number | null;
    power_w?: number | null;
  }> = [],
): GpuRow[] {
  return results.map((r) => {
    const memUsage = r.mem_usage_pct ?? 0;
    const idx = String(r.gpu_idx ?? r.gpu_index ?? '').replace(/^gpu-/, '');
    return {
      id: r.id || `${r.worker_address}-${idx}`,
      node: r.node || r.worker_address || '',
      workerAddress: r.worker_address || '',
      nodeStatus: r.status === 'offline' ? 'offline' : 'online',
      idx,
      name: r.name || `GPU-${idx}`,
      util: r.gpu_util ?? null,
      memUsage,
      usage: memUsage,
      memUsedGb: bytesToGB(r.mem_used || 0),
      memTotalGb: bytesToGB(r.mem_total || 0),
      status: (r.status as GpuRowStatus) || 'ok',
      acceleratorKind: r.accelerator_kind,
      occupant: r.occupant || null,
      temperatureC: r.temperature_c ?? null,
      powerW: r.power_w ?? null,
    };
  });
}

export function buildGpuAlerts(rows: GpuRow[]): AlertItem[] {
  return rows
    .filter((r) => r.status === 'crit' || r.status === 'warn' || r.status === 'offline')
    .map((r) => ({
      id: r.id,
      level: r.status === 'offline' || r.status === 'crit' ? ('error' as const) : ('warning' as const),
      title:
        r.status === 'offline'
          ? `${r.node} / GPU-${r.idx} 离线`
          : `${r.node} / GPU-${r.idx} 显存${r.status === 'crit' ? '超限' : '偏高'}`,
      detail: `${r.name} · util ${r.util ?? '—'}% · mem ${r.memUsage}% · ${r.memUsedGb}/${r.memTotalGb} GB`,
      time: '刚刚',
    }));
}

/**
 * @deprecated 假 24h 热力图已移除；保留空实现避免旧 import 崩。
 */
export function buildHeatmap(_rows: GpuRow[]): HeatmapRow[] {
  return [];
}

/** Match instance GPU util from device snapshot by worker + gpu index. */
export function matchGpuUtil(
  devices: DeviceInfo[] = [],
  workerAddress?: string,
  gpuIdx?: number[] | string[],
): { usage: number; memUsedGb: number; memTotalGb: number; util: number | null } | null {
  if (!workerAddress) return null;
  const host = workerAddress.split(':')[0];
  const device = devices.find((d) => {
    const addr = d.worker_address || '';
    return addr === workerAddress || addr.startsWith(`${host}:`) || addr === host;
  });
  if (!device?.gpus) return null;
  // Replica gpu_idx 空（adopt 未回填）时用该 Worker 全部卡，避免 util 条消失。
  const idxs = gpuIdx?.length
    ? gpuIdx.map(String)
    : Object.keys(device.gpus);
  let used = 0;
  let total = 0;
  let memSum = 0;
  let utilSum = 0;
  let utilN = 0;
  let n = 0;
  for (const idx of idxs) {
    const key = idx.startsWith('gpu-') ? idx : `gpu-${idx}`;
    const gpu = device.gpus[key] || device.gpus[idx];
    if (!gpu) continue;
    used += gpu.mem_used || 0;
    total += gpu.mem_total || 0;
    memSum += transformRate(gpu.mem_usage);
    const u = toUtilPct((gpu as { gpu_util?: number }).gpu_util);
    if (u !== null) {
      utilSum += u;
      utilN += 1;
    }
    n += 1;
  }
  if (!n) return null;
  return {
    usage: Math.round(memSum / n),
    memUsedGb: bytesToGB(used),
    memTotalGb: bytesToGB(total),
    util: utilN ? Math.round(utilSum / utilN) : null,
  };
}

export function exportGpuRowsCsv(rows: GpuRow[]): Array<Array<unknown>> {
  return rows.map((r) => [
    r.id,
    r.node,
    r.workerAddress,
    r.idx,
    r.name,
    r.util ?? '',
    r.memUsage,
    r.memUsedGb,
    r.memTotalGb,
    r.temperatureC ?? '',
    r.powerW ?? '',
    r.status,
    r.acceleratorKind || '',
    r.occupant?.model_uid || '',
  ]);
}

export type GpuViewMode = 'kind' | 'node' | 'model';

/** User-facing accelerator label: cuda/rocm → GPU (not "CUDA"). */
export function displayAccelKind(kind?: string | null): string {
  const k = String(kind || 'cuda').trim().toLowerCase();
  if (k === 'cuda' || k === 'rocm') return 'GPU';
  if (k === 'npu') return 'NPU';
  if (k === 'mlu') return 'MLU';
  if (k === 'cpu') return 'CPU';
  return k ? k.toUpperCase() : 'GPU';
}

export function groupGpuRowsByKind(rows: GpuRow[]): { key: string; rows: GpuRow[] }[] {
  const map = new Map<string, GpuRow[]>();
  for (const row of rows) {
    const key = (row.acceleratorKind || 'cuda').toLowerCase();
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(row);
  }
  return [...map.entries()].map(([key, grouped]) => ({ key, rows: grouped }));
}

export function groupGpuRowsByName(rows: GpuRow[]): { key: string; rows: GpuRow[] }[] {
  const map = new Map<string, GpuRow[]>();
  for (const row of rows) {
    const key = row.name || 'GPU';
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(row);
  }
  return [...map.entries()].map(([key, grouped]) => ({ key, rows: grouped }));
}

export function groupGpuRowsByModel(
  rows: GpuRow[],
): { key: string; label: string; rows: GpuRow[] }[] {
  const map = new Map<string, { label: string; rows: GpuRow[] }>();
  for (const row of rows) {
    const uid = row.occupant?.model_uid;
    if (!uid) continue;
    const slot = map.get(uid) || { label: row.occupant?.model_name || uid, rows: [] };
    slot.rows.push(row);
    map.set(uid, slot);
  }
  return [...map.entries()].map(([key, v]) => ({ key, label: v.label, rows: v.rows }));
}

export const GPU_CSV_HEADERS = [
  'id',
  'node',
  'worker_address',
  'gpu_idx',
  'name',
  'gpu_util',
  'mem_usage_pct',
  'mem_used_gb',
  'mem_total_gb',
  'temperature_c',
  'power_w',
  'status',
  'accelerator_kind',
  'occupant_model_uid',
];
