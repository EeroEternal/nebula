import type { ValueType } from '@/types/Public/data';

/** 转换formValue格式 example: 'true' -> true */
export const transformValueType = (str: unknown) => {
  if (str == null) return '';
  const value = String(str).trim();
  if (value === '') return '';
  if (value.toLowerCase() === 'none') return null;
  if (value.toLowerCase() === 'true') return true;
  if (value.toLowerCase() === 'false') return false;
  if (!isNaN(Number(value))) return Number(value);
  return value;
};

function formListKey(item: ValueType | undefined | null): string {
  if (item == null || item.key == null) return '';
  return String(item.key).trim();
}

/** 格式转换 [{key: 'min', value: 1}] => { min: 1}；空行 / 未填 key 跳过 */
export const transformFormListToObj = (formList: ValueType[] = []) =>
  (formList || []).reduce((acc, item) => {
    const k = formListKey(item);
    if (!k) return acc;
    acc[k] = transformValueType(item?.value);
    return acc;
  }, {} as Record<string, unknown>);

/** 环境变量等 Dict[str,str]：保持字符串，禁止把 "123"/"true" 转成 number/bool */
export const transformFormListToStringObj = (formList: ValueType[] = []) =>
  (formList || []).reduce((acc, item) => {
    const k = formListKey(item);
    if (!k) return acc;
    acc[k] = item?.value == null ? '' : String(item.value);
    return acc;
  }, {} as Record<string, string>);

/** 部署/编辑回填时不应进入「附加参数」FormList 的内部控制字段 */
export const INTERNAL_EXTEND_KWARGS_KEYS = [
  'engine_version',
  'engine-version',
  'engine_image',
  'engine_manifests',
  'engine_manifests_by_key',
  'enable_ib',
  'gpu_cores',
  'gpu-cores',
  'gpu_type',
  'gpu-type',
  'gpu_mem_gb',
  'gpu-mem-gb',
  'gpu_mem_mib',
  'gpu-mem-mib',
  'request_limits',
  'replica_concurrency',
  'replica-concurrency',
] as const;

export const convertObjToFormList = (
  obj: Record<string, unknown> = {},
  options?: { scalarsOnly?: boolean },
) => {
  return Object.entries(obj || {})
    .filter(([, value]) => {
      if (!options?.scalarsOnly) return true;
      if (value == null) return true;
      const t = typeof value;
      return t === 'string' || t === 'number' || t === 'boolean';
    })
    .map(([key, value]) => ({
      key,
      value: value == null ? '' : String(value),
    }));
};

/** 附加参数提交：过滤内部控制字段后再做类型转换 */
export const transformExtendFormListToObj = (formList: ValueType[] = []) => {
  const blocked = new Set<string>(INTERNAL_EXTEND_KWARGS_KEYS as unknown as string[]);
  return transformFormListToObj(
    (formList || []).filter((item) => {
      const k = item?.key == null ? '' : String(item.key).trim();
      return k !== '' && !blocked.has(k);
    }),
  );
};
