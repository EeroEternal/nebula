const PREFIX = 'powerllm.repo.pinned.';

export const repoPinKey = (modelType: string, modelName: string) =>
  `${modelType}:${modelName}`;

export const loadRepoPins = (username: string): string[] => {
  try {
    const raw = localStorage.getItem(`${PREFIX}${username || 'anonymous'}`);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.filter((x): x is string => typeof x === 'string')
      : [];
  } catch {
    return [];
  }
};

export const saveRepoPins = (username: string, keys: string[]) => {
  localStorage.setItem(`${PREFIX}${username || 'anonymous'}`, JSON.stringify(keys));
};

/** 后钉的排最前：数组末尾是最近一次置顶 */
export const toggleRepoPin = (username: string, key: string): string[] => {
  const cur = loadRepoPins(username);
  const next = cur.includes(key)
    ? cur.filter((k) => k !== key)
    : [...cur.filter((k) => k !== key), key];
  saveRepoPins(username, next);
  return next;
};

export const applyRepoPins = <T extends { model_name?: string; model_type?: string }>(
  list: T[],
  pinned: string[],
  tabType?: string,
): T[] => {
  if (!pinned.length) return list;
  const rank = new Map(pinned.map((k, i) => [k, i]));
  const pinnedItems: T[] = [];
  const rest: T[] = [];
  for (const item of list) {
    const type =
      tabType === 'custom'
        ? String(item.model_type || 'LLM')
        : String(tabType || item.model_type || 'LLM');
    const key = repoPinKey(type, String(item.model_name || ''));
    if (rank.has(key)) pinnedItems.push(item);
    else rest.push(item);
  }
  pinnedItems.sort((a, b) => {
    const typeA =
      tabType === 'custom' ? String(a.model_type || 'LLM') : String(tabType || 'LLM');
    const typeB =
      tabType === 'custom' ? String(b.model_type || 'LLM') : String(tabType || 'LLM');
    const ra = rank.get(repoPinKey(typeA, String(a.model_name || ''))) ?? 0;
    const rb = rank.get(repoPinKey(typeB, String(b.model_name || ''))) ?? 0;
    return rb - ra;
  });
  return [...pinnedItems, ...rest];
};
