import {
  transformFormListToObj,
  transformFormListToStringObj,
  transformExtendFormListToObj,
  transformValueType,
} from '@/utils/formList';
import { buildManifestLaunchKwargs } from '@/components/DeployModelInstance/buildManifestKwargs';

describe('transformValueType', () => {
  it('正向：布尔与数字字符串', () => {
    expect(transformValueType('true')).toBe(true);
    expect(transformValueType('False')).toBe(false);
    expect(transformValueType('256')).toBe(256);
    expect(transformValueType('none')).toBeNull();
  });

  it('反向：未填 / null 不抛、不写成 undefined 字符串', () => {
    expect(transformValueType(undefined)).toBe('');
    expect(transformValueType(null)).toBe('');
    expect(transformValueType('')).toBe('');
    expect(transformValueType('  ')).toBe('');
  });
});

describe('transformFormListToObj', () => {
  it('正向：完整 key/value', () => {
    expect(
      transformFormListToObj([
        { key: 'max_model_len', value: '8192' },
        { key: 'enable_prefix_caching', value: 'true' },
      ]),
    ).toEqual({ max_model_len: 8192, enable_prefix_caching: true });
  });

  it('反向：新增空行（antd Form.List 未填）不抛 trim', () => {
    expect(() =>
      transformFormListToObj([
        { key: undefined as unknown as string, value: undefined as unknown as string },
        undefined as unknown as { key: string; value: string },
        { key: null as unknown as string, value: '' },
        { key: '', value: 'x' },
      ]),
    ).not.toThrow();
    expect(
      transformFormListToObj([
        { key: undefined as unknown as string, value: undefined as unknown as string },
        undefined as unknown as { key: string; value: string },
      ]),
    ).toEqual({});
  });

  it('边界：只填 key、value 空；key 仅空白', () => {
    expect(transformFormListToObj([{ key: 'only_key', value: undefined as unknown as string }])).toEqual({
      only_key: '',
    });
    expect(transformFormListToObj([{ key: '   ', value: '1' }])).toEqual({});
  });

  it('组合：空行夹在有效行之间，有效 key 保留', () => {
    expect(
      transformFormListToObj([
        { key: 'a', value: '1' },
        { key: undefined as unknown as string, value: undefined as unknown as string },
        { key: 'b', value: '2' },
      ]),
    ).toEqual({ a: 1, b: 2 });
  });
});

describe('transformFormListToStringObj（环境变量）', () => {
  it('正向：数字/布尔保持字符串', () => {
    expect(
      transformFormListToStringObj([
        { key: 'NCCL_IB_DISABLE', value: '0' },
        { key: 'FLAG', value: 'true' },
      ]),
    ).toEqual({ NCCL_IB_DISABLE: '0', FLAG: 'true' });
  });

  it('反向：空行不抛', () => {
    expect(
      transformFormListToStringObj([
        undefined as unknown as { key: string; value: string },
        { key: undefined as unknown as string, value: undefined as unknown as string },
      ]),
    ).toEqual({});
  });

  it('边界：value 为空仍保留 key', () => {
    expect(transformFormListToStringObj([{ key: 'HTTP_PROXY', value: '' }])).toEqual({
      HTTP_PROXY: '',
    });
  });
});

describe('transformExtendFormListToObj', () => {
  it('反向：空行 + 内部控制字段都丢掉', () => {
    expect(
      transformExtendFormListToObj([
        { key: undefined as unknown as string, value: '1' },
        { key: 'engine_version', value: 'v1' },
        { key: 'gpu_cores', value: '100' },
        { key: 'gpu_type', value: 'NVIDIA RTX PRO 4000 Blackwell' },
        { key: 'max_model_len', value: '4096' },
      ]),
    ).toEqual({ max_model_len: 4096 });
  });
});

describe('buildManifestLaunchKwargs（切 YAML 预览）', () => {
  it('正向：附加参数进入扁平 kwargs', () => {
    const out = buildManifestLaunchKwargs({
      enable_ib: true,
      extend_config: [{ key: 'max_num_seqs', value: '64' }],
    });
    expect(out.enable_ib).toBe(true);
    expect(out.max_num_seqs).toBe(64);
  });

  it('反向：附加参数 / 量化 / MindIE 空行不抛、不污染', () => {
    expect(() =>
      buildManifestLaunchKwargs({
        extend_config: [{ key: undefined, value: undefined }],
        quantization_config: [undefined, { key: '', value: '1' }],
        mindIE_config: [undefined, {}, { npuDeviceIds: '' }],
      }),
    ).not.toThrow();
    const out = buildManifestLaunchKwargs({
      extend_config: [{ key: undefined, value: undefined }],
      quantization_config: [{ key: undefined, value: undefined }],
      mindIE_config: [undefined, {}],
    });
    expect(out.max_model_len).toBeUndefined();
    expect(out.quantization_config).toEqual({});
    expect(out.mindIE_config).toEqual([]);
  });

  it('组合：有效附加参数 + 空环境变量行形状的 extend 夹杂', () => {
    const out = buildManifestLaunchKwargs({
      extend_config: [
        { key: 'gpu_memory_utilization', value: '0.9' },
        { key: undefined, value: undefined },
        { key: 'max_model_len', value: '8192' },
      ],
      quantization_config: [{ key: 'activation', value: 'int8' }, { key: undefined }],
    });
    expect(out.gpu_memory_utilization).toBe(0.9);
    expect(out.max_model_len).toBe(8192);
    expect(out.quantization_config).toEqual({ activation: 'int8' });
  });

  it('反向：HAMi 配额字段不进 YAML 预览 kwargs（与提交路径一致）', () => {
    const out = buildManifestLaunchKwargs({
      extend_config: [
        { key: 'gpu_cores', value: '100' },
        { key: 'gpu_type', value: 'NVIDIA RTX PRO 4000 Blackwell' },
        { key: 'max_model_len', value: '4096' },
      ],
    });
    expect(out.gpu_cores).toBeUndefined();
    expect(out.gpu_type).toBeUndefined();
    expect(out.max_model_len).toBe(4096);
  });
});
