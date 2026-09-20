/** 合并 Prometheus 趋势点：按 ts 去重，保留近 windowSeconds。 */

export type TrendPoint = { ts: number; value: number };

export type PerfTrends = {
  ttft_p95_ms?: TrendPoint[];
  qps?: TrendPoint[];
  error_rate?: TrendPoint[];
};

export type PerfLike = {
  trends?: PerfTrends;
  [key: string]: unknown;
};

const DEFAULT_KEEP_S = 3600;

export function mergeTrendPoints(
  prev?: TrendPoint[],
  next?: TrendPoint[],
  keepFromTs?: number,
): TrendPoint[] {
  const map = new Map<number, number>();
  for (const p of prev || []) {
    if (p && Number.isFinite(p.ts) && Number.isFinite(p.value)) {
      map.set(p.ts, p.value);
    }
  }
  for (const p of next || []) {
    if (p && Number.isFinite(p.ts) && Number.isFinite(p.value)) {
      map.set(p.ts, p.value);
    }
  }
  let rows = [...map.entries()]
    .map(([ts, value]) => ({ ts, value }))
    .sort((a, b) => a.ts - b.ts);
  if (keepFromTs != null) {
    rows = rows.filter((p) => p.ts >= keepFromTs);
  }
  return rows;
}

/** live 覆盖卡片/副本；trends 与历史合并，避免实时帧冲掉近期曲线。 */
export function mergePerfPayload<T extends PerfLike>(
  historical: T | null | undefined,
  live: T | null | undefined,
  keepSeconds = DEFAULT_KEEP_S,
): T {
  if (!live && !historical) return {} as T;
  if (!live) return historical as T;
  if (!historical) return live;
  const keepFrom = Math.floor(Date.now() / 1000) - keepSeconds;
  const ht = historical.trends || {};
  const lt = live.trends || {};
  return {
    ...historical,
    ...live,
    trends: {
      ttft_p95_ms: mergeTrendPoints(ht.ttft_p95_ms, lt.ttft_p95_ms, keepFrom),
      qps: mergeTrendPoints(ht.qps, lt.qps, keepFrom),
      error_rate: mergeTrendPoints(ht.error_rate, lt.error_rate, keepFrom),
    },
  };
}
