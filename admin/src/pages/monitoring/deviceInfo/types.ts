import type { DeviceInfo, Gpus, Cpu } from '@/types/Public/data';

export type GpuCardSource = Gpus[keyof Gpus] & {
  /** 显存占用占比 */
  usageRate: number;
  /** 核心利用率（可选） */
  utilRate?: number;
  memoryUsedGB: number;
  memoryTotalGB: number;
};

export interface DeviceCardSource extends Omit<DeviceInfo, 'cpu' | 'gpus'> {
  cpu: Cpu & {
    usageRate: number;
    memoryUsedGB: number;
    memoryTotalGB: number;
    memoryRate: number;
  };
  gpus: GpuCardSource[];
}
