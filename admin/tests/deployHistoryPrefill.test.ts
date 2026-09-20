import {
  mapHistoryReplicaConfig,
  omitLegacyFlatLaunchFields,
  shouldBackfillModelPath,
} from '../src/utils/deployHistoryPrefill';

describe('shouldBackfillModelPath', () => {
  it('backfills when the form has no entry version', () => {
    expect(shouldBackfillModelPath(undefined, 'v1')).toBe(true);
    expect(shouldBackfillModelPath('', 'v1')).toBe(true);
  });

  it('backfills when entry and history versions match', () => {
    expect(shouldBackfillModelPath('v1', 'v1')).toBe(true);
  });

  it('does not backfill when the entry version differs', () => {
    expect(shouldBackfillModelPath('v2', 'v1')).toBe(false);
  });
});

describe('mapHistoryReplicaConfig', () => {
  const history = [
    {
      devices: [{ gpu_idx: [0], model_path: '/data/.powerllm/modelscope/x' }],
    },
  ];

  it('keeps model_path when backfill is on', () => {
    const next = mapHistoryReplicaConfig(history, true);
    expect(next[0].replica_uid).toBe('');
    expect(next[0].devices?.[0].model_path).toBe('/data/.powerllm/modelscope/x');
  });

  it('strips model_path when backfill is off', () => {
    const next = mapHistoryReplicaConfig(history, false);
    expect(next[0].devices?.[0].model_path).toBeUndefined();
  });

  it('leaves empty paths empty', () => {
    const next = mapHistoryReplicaConfig([{ devices: [{}] }], true);
    expect(next[0].devices?.[0].model_path).toBeUndefined();
  });
});

describe('omitLegacyFlatLaunchFields', () => {
  it('drops leftover history flat keys so replica_config can launch', () => {
    const next = omitLegacyFlatLaunchFields({
      model_uid: 'qwen3.5',
      gpu_idx: [0],
      n_gpu: 1,
      worker_ip: '10.0.0.1:9997',
      model_path: '/old',
      replica_config: [{ devices: [{ worker_ip: '192.168.1.200:30001', gpu_idx: [0] }] }],
    });
    expect(next.gpu_idx).toBeUndefined();
    expect(next.n_gpu).toBeUndefined();
    expect(next.worker_ip).toBeUndefined();
    expect(next.model_path).toBeUndefined();
    expect(next.model_uid).toBe('qwen3.5');
    expect(next.replica_config[0].devices[0].gpu_idx).toEqual([0]);
  });
});
