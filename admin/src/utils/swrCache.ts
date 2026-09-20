/**
 * 轻量 SWR 内存缓存：命中时同步返回旧数据，过期则后台刷新。
 * 用于列表页「感知毫秒级」切换 / 筛选回看。
 */

type Entry<T> = {
  data: T;
  at: number;
  inflight?: Promise<T>;
};

const store = new Map<string, Entry<unknown>>();

export type SwrGetOptions<T> = {
  /** 新鲜窗口（ms），默认 30s */
  staleTime?: number;
  /** 忽略缓存强制拉取 */
  force?: boolean;
  /** 后台刷新完成后回调（用于更新 UI） */
  onUpdate?: (data: T) => void;
};

export function swrPeek<T>(key: string): T | undefined {
  return store.get(key)?.data as T | undefined;
}

export function swrIsStale(key: string, staleTime = 30_000): boolean {
  const hit = store.get(key);
  if (!hit) return true;
  return Date.now() - hit.at >= staleTime;
}

export function swrSet<T>(key: string, data: T): void {
  store.set(key, { data, at: Date.now() });
}

/** 删除 key 或某前缀下全部缓存 */
export function swrInvalidate(keyOrPrefix?: string): void {
  if (!keyOrPrefix) {
    store.clear();
    return;
  }
  if (store.has(keyOrPrefix)) {
    store.delete(keyOrPrefix);
  }
  for (const k of [...store.keys()]) {
    if (k.startsWith(keyOrPrefix)) store.delete(k);
  }
}

/**
 * 读缓存：新鲜则直接返回；过期则先返回旧值并后台刷新；无缓存则等待网络。
 */
export async function swrGet<T>(
  key: string,
  fetcher: () => Promise<T>,
  opts?: SwrGetOptions<T>,
): Promise<{ data: T; fromCache: boolean }> {
  const staleTime = opts?.staleTime ?? 30_000;
  const hit = store.get(key) as Entry<T> | undefined;

  const revalidate = (): Promise<T> => {
    if (hit?.inflight) return hit.inflight;
    const existing = store.get(key) as Entry<T> | undefined;
    if (existing?.inflight) return existing.inflight;

    const inflight = fetcher()
      .then((data) => {
        store.set(key, { data, at: Date.now() });
        opts?.onUpdate?.(data);
        return data;
      })
      .finally(() => {
        const cur = store.get(key) as Entry<T> | undefined;
        if (cur) cur.inflight = undefined;
      });

    store.set(key, {
      data: (existing?.data ?? hit?.data) as T,
      at: existing?.at ?? hit?.at ?? 0,
      inflight,
    });
    return inflight;
  };

  if (!opts?.force && hit && hit.data !== undefined) {
    if (Date.now() - hit.at >= staleTime) {
      void revalidate();
    }
    return { data: hit.data, fromCache: true };
  }

  const data = await revalidate();
  return { data, fromCache: false };
}
