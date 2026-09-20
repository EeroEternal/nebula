export type AppNotificationType =
  | 'deploy_ready'
  | 'supervisor_change'
  | 'runtime_mismatch';

export type AppNotification = {
  id: string;
  type: AppNotificationType;
  title: string;
  desc?: string;
  modelUid?: string;
  modelName?: string;
  createdAt: number;
};

const STORAGE_KEY = 'powerllm.appNotifications.v1';
const SESSION_SEEN_KEY = 'powerllm.deployReady.seen';
const SESSION_PENDING_KEY = 'powerllm.deployReady.pending';

export function loadAppNotifications(): AppNotification[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as AppNotification[]) : [];
  } catch {
    return [];
  }
}

export function saveAppNotifications(items: AppNotification[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items.slice(0, 100)));
  } catch {
    /* ignore quota */
  }
}

export function makeRuntimeMismatchNotification(
  title: string,
  desc: string,
): AppNotification {
  return {
    id: `runtime_mismatch_${Date.now()}`,
    type: 'runtime_mismatch',
    title,
    desc,
    createdAt: Date.now(),
  };
}

export function makeDeployReadyNotification(
  modelUid: string,
  modelName?: string,
): AppNotification {
  return {
    id: `deploy_ready_${modelUid}_${Date.now()}`,
    type: 'deploy_ready',
    title: '模型部署成功',
    desc: modelName ? `${modelName} (${modelUid})` : modelUid,
    modelUid,
    modelName,
    createdAt: Date.now(),
  };
}

function readUidSet(key: string): Set<string> {
  try {
    const raw = sessionStorage.getItem(key);
    const arr = raw ? (JSON.parse(raw) as string[]) : [];
    return new Set(Array.isArray(arr) ? arr : []);
  } catch {
    return new Set();
  }
}

function writeUidSet(key: string, set: Set<string>) {
  try {
    sessionStorage.setItem(key, JSON.stringify(Array.from(set)));
  } catch {
    /* ignore */
  }
}

export function hasSeenDeployReady(modelUid: string): boolean {
  return readUidSet(SESSION_SEEN_KEY).has(modelUid);
}

export function markSeenDeployReady(modelUid: string) {
  const set = readUidSet(SESSION_SEEN_KEY);
  set.add(modelUid);
  writeUidSet(SESSION_SEEN_KEY, set);
  // Clear pending once prompted
  const pending = readUidSet(SESSION_PENDING_KEY);
  if (pending.delete(modelUid)) {
    writeUidSet(SESSION_PENDING_KEY, pending);
  }
}

/** Mark instance as launching so READY can prompt even if watcher remounted. */
export function markPendingDeployReady(modelUid: string) {
  const set = readUidSet(SESSION_PENDING_KEY);
  set.add(modelUid);
  writeUidSet(SESSION_PENDING_KEY, set);
}

export function isPendingDeployReady(modelUid: string): boolean {
  return readUidSet(SESSION_PENDING_KEY).has(modelUid);
}

export function clearPendingDeployReady(modelUid: string) {
  const set = readUidSet(SESSION_PENDING_KEY);
  if (set.delete(modelUid)) {
    writeUidSet(SESSION_PENDING_KEY, set);
  }
}
