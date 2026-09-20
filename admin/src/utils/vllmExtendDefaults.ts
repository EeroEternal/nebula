import type { ValueType } from '@/types/Public/data';
import { MODEL_EXTENDS_CONFIG_TEMPLATE } from '@/constants/modelData';

export const VLLM_DEFAULT_EXTEND: ValueType[] = [
  { key: 'max_model_len', value: '32768' },
  { key: 'gpu_memory_utilization', value: '0.90' },
  { key: 'max_num_seqs', value: '256' },
];

export function isVllmFamilyEngine(engine?: string) {
  const key = String(engine || '').toLowerCase();
  return key === 'vllm' || key === 'vllm-ascend';
}

/** vllm-ascend 与 vllm 共用同一套推荐附加参数。 */
export function extendConfigOptionsForEngine(engine?: string) {
  const key = String(engine || '').toLowerCase();
  const table = MODEL_EXTENDS_CONFIG_TEMPLATE as Record<string, unknown>;
  if (key === 'vllm-ascend') return table.vllm || [];
  return table[key] || [];
}

/** 每次部署把上下文 / 显存比例 / 并发固定在附加参数前三项；已有值保留。 */
export function ensureVllmDefaultExtend(list?: ValueType[]): ValueType[] {
  const rest = [...(list || [])];
  const pinned: ValueType[] = [];
  for (const def of VLLM_DEFAULT_EXTEND) {
    const idx = rest.findIndex(
      (item) => String(item?.key || '').toLowerCase() === def.key,
    );
    if (idx >= 0) {
      pinned.push(rest[idx]);
      rest.splice(idx, 1);
    } else {
      pinned.push({ ...def });
    }
  }
  return [...pinned, ...rest];
}
