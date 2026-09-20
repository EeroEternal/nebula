/** Prometheus matrix 行（query_range result[]） */
export type PromMatrixRow = {
  metric?: Record<string, string>;
  values?: [number, string][];
};

type PromWrapped = {
  success?: boolean;
  code?: number;
  data?: unknown;
};

/**
 * 从 umi `request` 包装后的 Prom query / query_range 响应取出 result[]。
 * 实际路径：`{ success, data: { code, message, data: { status, data: { result } } } }`
 */
export function extractPromMatrixResult(res: PromWrapped | null | undefined): PromMatrixRow[] {
  if (!res) return [];
  if (res.success === false) return [];
  if (typeof res.code === 'number' && res.code !== 0) return [];

  const body = res.data as Record<string, unknown> | undefined;
  if (!body) return [];

  const candidates = [
    (body as { data?: { data?: { result?: PromMatrixRow[] } } })?.data?.data?.result,
    (body as { data?: { result?: PromMatrixRow[] } })?.data?.result,
    (body as { result?: PromMatrixRow[] })?.result,
  ];
  for (const c of candidates) {
    if (Array.isArray(c)) return c;
  }
  return [];
}
