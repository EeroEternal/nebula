import { evaluateHamiQuota } from '../src/utils/hamiQuotaFit';

describe('evaluateHamiQuota', () => {
  it('skips when gpu_mem_gb is empty', () => {
    expect(evaluateHamiQuota([{ n_gpu: 1 }], [{ capacity_ready: true, free_mem: 0 }])).toEqual({
      ok: true,
    });
  });

  it('fails when no node reported capacity', () => {
    const r = evaluateHamiQuota(
      [{ n_gpu: 1, gpu_mem_gb: 8 }],
      [{ capacity_ready: false, free_mem: 0 }],
    );
    expect(r.ok).toBe(false);
    expect(r.reason).toBe('capacity');
  });

  it('fails when free memory is not enough', () => {
    const r = evaluateHamiQuota(
      [{ n_gpu: 2, gpu_mem_gb: 16 }],
      [{ capacity_ready: true, free_mem: 8 * 1024, free_core: 100 }],
    );
    expect(r.ok).toBe(false);
    expect(r.reason).toBe('mem');
    expect(r.fits).toBe(0);
  });

  it('reports how many copies still fit', () => {
    const r = evaluateHamiQuota(
      [{ n_gpu: 1, gpu_mem_gb: 8, gpu_cores: 20 }],
      [{ capacity_ready: true, free_mem: 40 * 1024, free_core: 80, product: 'L40S' }],
    );
    expect(r.ok).toBe(true);
    expect(r.fits).toBe(4);
  });
});
