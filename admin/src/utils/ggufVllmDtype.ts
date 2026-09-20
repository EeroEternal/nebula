import type { ValueType } from '@/types/Public/data';

/** model_version 形如 name--size--ggufv2--quant */
export const isGgufModelVersion = (version?: string) =>
  !!version && /--gguf/i.test(version);

/**
 * vLLM + GGUF 必须 float16（bf16 不支持 gguf）。
 * 系统项放在最前面并打 system 标记（前端星标用）。
 */
export const ensureGgufVllmDtype = (extendConfig: ValueType[] | undefined): ValueType[] => {
  const list = [...(extendConfig || [])];
  const idx = list.findIndex(
    (item) => String(item?.key || '').toLowerCase() === 'dtype' && item?.value != null && item.value !== '',
  );
  if (idx >= 0) {
    const existing = list[idx];
    // 已有用户自填的 dtype：不改动顺序与标记
    if (!existing.system) {
      return list;
    }
    const rest = list.filter((_, i) => i !== idx);
    return [{ ...existing, key: 'dtype', system: true }, ...rest];
  }
  return [{ key: 'dtype', value: 'float16', system: true }, ...list];
};

export const needsGgufVllmDtype = (modelEngine?: string, modelVersion?: string) =>
  String(modelEngine || '').toLowerCase() === 'vllm' && isGgufModelVersion(modelVersion);

export const isSystemExtendItem = (item?: ValueType | null) =>
  !!item && (item.system === true || String(item.system) === 'true');

/** 比较 extend_config（忽略 system 的 true/"true" 差异，避免 Form 回写死循环） */
export const extendConfigEquals = (a?: ValueType[], b?: ValueType[]) => {
  const left = a || [];
  const right = b || [];
  if (left.length !== right.length) return false;
  return left.every(
    (item, i) =>
      String(item?.key || '') === String(right[i]?.key || '') &&
      String(item?.value ?? '') === String(right[i]?.value ?? '') &&
      isSystemExtendItem(item) === isSystemExtendItem(right[i]),
  );
};
