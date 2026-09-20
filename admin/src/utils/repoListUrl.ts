const STORAGE_KEY = 'powerllm.repoList.lastUrl';
/** 与 `REPOSITORY` 类型 Tab 对齐，不含 downloads */
const LIST_TYPES = new Set([
  'LLM',
  'image',
  'embedding',
  'rerank',
  'audio',
  'video',
  'custom',
]);
const ALLOWED_QUERY = new Set([
  'model_name',
  'cache_status',
  'model_ability',
  'model_lang',
  'sort_by',
  'curPageNum',
  'custom_model_type',
  'model_path',
]);
const MAX_URL_LEN = 2048;

export const DEFAULT_REPO_LIST_PATH = '/models/repository/LLM';

export function isRepoListPath(pathname: string): boolean {
  const m = /^\/models\/repository\/([^/]+)$/.exec(pathname || '');
  return !!(m && LIST_TYPES.has(m[1]));
}

const normalizeSearch = (search?: string): string => {
  if (!search) return '';
  const raw = search.startsWith('?') ? search.slice(1) : search;
  if (!raw) return '';
  const params = new URLSearchParams(raw);
  const next = new URLSearchParams();
  params.forEach((value, key) => {
    if (ALLOWED_QUERY.has(key) && value) next.set(key, value);
  });
  return next.toString();
};

export function saveLastRepoListUrl(pathname: string, search?: string): void {
  if (typeof sessionStorage === 'undefined') return;
  if (!isRepoListPath(pathname)) return;
  const q = normalizeSearch(search);
  const url = q ? `${pathname}?${q}` : pathname;
  if (url.length > MAX_URL_LEN) return;
  try {
    sessionStorage.setItem(STORAGE_KEY, url);
  } catch {
    /* quota / private mode */
  }
}

export function getLastRepoListUrl(): string | null {
  if (typeof sessionStorage === 'undefined') return null;
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw || raw.length > MAX_URL_LEN || !raw.startsWith('/models/repository/')) {
      return null;
    }
    const qIndex = raw.indexOf('?');
    const path = qIndex === -1 ? raw : raw.slice(0, qIndex);
    const search = qIndex === -1 ? '' : raw.slice(qIndex + 1);
    if (!isRepoListPath(path)) return null;
    const q = normalizeSearch(search);
    return q ? `${path}?${q}` : path;
  } catch {
    return null;
  }
}

/** 侧栏 / 裸 /models/repository：有上次列表则回该 URL，否则走默认 LLM */
export function resolveRepoMenuPath(): string {
  return getLastRepoListUrl() || '/models/repository';
}
