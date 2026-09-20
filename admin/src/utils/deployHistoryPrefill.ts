/** History 回填模型地址：同版本才带上次路径，入口指定其它版本则不沿用。 */

export function shouldBackfillModelPath(
  entryVersion?: string,
  historyVersion?: string,
): boolean {
  const entry = (entryVersion || '').trim();
  if (!entry) return true;
  return entry === (historyVersion || '').trim();
}

export type HistoryDevice = {
  model_path?: string;
  [key: string]: unknown;
};

export type HistoryReplica = {
  devices?: HistoryDevice[];
  [key: string]: unknown;
};

/** Top-level placement keys superseded by replica_config. History/instance
 *  backfill may still echo them onto the form; launch must not send them. */
export const LEGACY_FLAT_LAUNCH_KEYS = [
  'worker_ip',
  'n_gpu',
  'gpu_idx',
  'model_path',
] as const;

export function omitLegacyFlatLaunchFields<T extends Record<string, unknown>>(
  values: T,
): T {
  const next = { ...values };
  for (const key of LEGACY_FLAT_LAUNCH_KEYS) {
    delete next[key];
  }
  return next;
}

export function mapHistoryReplicaConfig(
  replicaConfig: HistoryReplica[] | undefined,
  backfillModelPath: boolean,
): HistoryReplica[] {
  return (replicaConfig || []).map((item) => ({
    ...item,
    replica_uid: '',
    devices: (item.devices || []).map((d) => ({
      ...d,
      model_path: backfillModelPath ? d.model_path : undefined,
    })),
  }));
}
