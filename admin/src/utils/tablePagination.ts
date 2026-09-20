/**
 * Ant Design Table 异步分页约定：
 * 当 `pagination.total > dataSource.length` 且 `dataSource.length > pageSize` 时，
 * Table 会按 `current` 再切片 —— 服务端已分页的数据会被切错/切空。
 * 服务端模式务必保证 `dataSource.length <= pageSize`。
 */

export function normalizePageSize(pageSize?: number, fallback = 10): number {
  const n = Number(pageSize);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : fallback;
}

export function normalizePage(current?: number): number {
  const n = Number(current);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 1;
}

/** 截断到一页，避免 Table 二次切片 */
export function asServerPageRows<T>(rows: T[] | undefined | null, pageSize?: number): T[] {
  const size = normalizePageSize(pageSize);
  const list = Array.isArray(rows) ? rows : [];
  return list.length > size ? list.slice(0, size) : list;
}

/** 本地全量结果上的客户端分页（data 长度 ≤ pageSize，total 为全量） */
export function sliceClientPage<T>(
  rows: T[] | undefined | null,
  current?: number,
  pageSize?: number,
): { data: T[]; total: number } {
  const size = normalizePageSize(pageSize);
  const page = normalizePage(current);
  const list = Array.isArray(rows) ? rows : [];
  const start = (page - 1) * size;
  return {
    data: list.slice(start, start + size),
    total: list.length,
  };
}
