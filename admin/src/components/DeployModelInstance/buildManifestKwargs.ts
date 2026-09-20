import { isNil, omitBy, pick, size } from 'lodash';

import type { ValueType } from '@/types/Public/data';
import { transformExtendFormListToObj, transformFormListToObj } from '@/utils/formList';

/** Keys kept as-is under kwargs (not flattened from FormList). */
const BASE_KEY_FOR_KWARGS = [
  'enable_thinking',
  'reasoning_content',
  'cpu_offload',
  'enable_ib',
];

/**
 * Normalize deploy-form kwargs for manifest-preview / engine CLI mapping.
 * Must match submit payload shape in DeployModelInstance handleSubmit
 * (extend_config FormList → flat key/value; quantization_config → object).
 */
export function buildManifestLaunchKwargs(kwargs: Record<string, unknown> | undefined | null) {
  const src = kwargs || {};
  const mindIE = Array.isArray(src.mindIE_config) ? src.mindIE_config : [];
  return {
    ...pick(src, BASE_KEY_FOR_KWARGS),
    mindIE_config: size(mindIE)
      ? mindIE
          .filter((item): item is Record<string, unknown> => !!item && typeof item === 'object')
          .map((item) => omitBy(item, (value) => value === '' || isNil(value)))
          .filter((item) => size(item))
      : undefined,
    quantization_config: size(src.quantization_config as object | undefined)
      ? transformFormListToObj(src.quantization_config as ValueType[])
      : undefined,
    ...(size(src.extend_config as object | undefined)
      ? transformExtendFormListToObj(src.extend_config as ValueType[])
      : {}),
  };
}
