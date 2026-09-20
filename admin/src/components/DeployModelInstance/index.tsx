import {
  ProForm,
  ProFormDigit,
  ProFormSelect,
  ProFormSwitch,
  ProFormText,
} from '@ant-design/pro-components';
import { history } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Form, message, App, Col, Row, Spin, Tag, Tooltip, Drawer, Button } from 'antd';
import type { FormProps } from 'antd';
import { FileCode2, SlidersHorizontal } from 'lucide-react';
import { isBoolean, isEmpty, omit, pick, size, omitBy, isNil } from 'lodash';
import { FC, useEffect, useMemo, useRef, useState } from 'react';

import ActionWithTips from '@/components/ActionWithTips';
import IconButton from '@/components/IconButton';

import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import {
  MODEL_DOWNLOAD_HUB,
  MODEL_DOWNLOAD_HUB_LABELS,
  ModelType,
  VIRTUAL_ENV_UNSET,
} from '@/constants/modelData';
import type {
  DeviceInfo,
  ModelData,
  ModelEngines,
  ModelVersionListItem,
  ModelVersionTableListItem,
  ValueType,
} from '@/types/Public/data';
import {
  convertObjToFormList,
  INTERNAL_EXTEND_KWARGS_KEYS,
  transformExtendFormListToObj,
  transformFormListToObj,
  transformFormListToStringObj,
} from '@/utils';
import {
  mapHistoryReplicaConfig,
  omitLegacyFlatLaunchFields,
  shouldBackfillModelPath,
} from '@/utils/deployHistoryPrefill';
import {
  ensureGgufVllmDtype,
  extendConfigEquals,
  needsGgufVllmDtype,
} from '@/utils/ggufVllmDtype';
import {
  ensureVllmDefaultExtend,
  isVllmFamilyEngine,
} from '@/utils/vllmExtendDefaults';
import { readResponseDetail } from '@/utils/formatApiError';
import { evaluateHamiQuota } from '@/utils/hamiQuotaFit';
import type { HamiDeviceReq, HamiStockRow } from '@/utils/hamiQuotaFit';
import { useK8sRuntime } from '@/hooks/useK8sRuntime';
import { l, lGet } from '@/utils/intl';
import { markPendingDeployReady } from '@/utils/appNotifications';
import request from '@/utils/request';
import { swrInvalidate } from '@/utils/swrCache';
import {
  listEngineImages,
  listLocalEngineImages,
  getEngineRegistry,
  getEngineCompat,
  type EngineImageSpec,
  type EngineCompatResult,
  type LocalEngineImagesResult,
} from '@/services/engineImages';
import {
  compareEngines,
  engineTagLabel,
  PREFERRED_CATALOG_ENGINES_BY_TYPE,
} from '@/pages/management/engines/utils';
import {
  buildEngineVersionOptions,
  localImageBelongsToEngine,
} from './engineVersionOptions';

/** 引擎名大小写不敏感查找（API 多为 vLLM，表单/历史可能是 vllm） */
const findEngineConfig = (config: ModelEngines | undefined, engine?: string) => {
  if (!config || !engine) return undefined;
  if (Object.prototype.hasOwnProperty.call(config, engine)) {
    return { key: engine, value: config[engine] };
  }
  const lower = engine.toLowerCase();
  const key = Object.keys(config).find((k) => k.toLowerCase() === lower);
  return key !== undefined ? { key, value: config[key] } : undefined;
};

const engineDisplayLabel = (engine: string) => engineTagLabel(engine);

const buildLlmVersionOptions = (specs: ModelEngines[string] | undefined) =>
  (specs || []).flatMap((item) =>
    (item?.quantizations || []).map((quant) => ({
      label: `${item.model_name}--${item.model_size_in_billions}B--${item.model_format}--${quant}`,
      value: `${item.model_name}--${item.model_size_in_billions}B--${item.model_format}--${quant}`,
      // 可用标准：权重已在 cache tracker（含下载完成后登记）
      cache_status: (item?.cache_list || []).includes(quant),
    })),
  );

import CommandLineCopy from './CommandLineCopy';
import CommandLineModal from './CommandLineModal';
import ManifestYamlPreview, { type EngineManifestDraft } from './ManifestYamlPreview';
import ReplicaInfo, { initReplicaConfigValue } from './ReplicaInfo';
import MindIEParams from './MindIEParams';
import type { OtherParamsMethod } from './OtherParams';
import OtherParams from './OtherParams';
import { envsSuggestIbEnabled } from './ibDeployEnv';

// 放在kwargs中的特定字段
const baseKeyForKwargs = [
  'enable_thinking',
  'reasoning_content',
  'cpu_offload',
  'enable_ib',
];
// 放在kwargs中的高级配置字段
const specificKeyForKwargs = ['quantization_config', 'mindIE_config', 'advanced_config'];
export interface FormValues {
  model_uid: string;
  model_name: string;
  model_engine?: string;
  /** Engine image version from /v1/engine-images catalog (scheme B). */
  engine_version?: string;
  model_version: string;
  download_hub?: string;
  request_limits?: number;
  replica_concurrency?: number;
  replica?: number;
  replica_config: {
    replica_uid?: string;
    devices: {
      worker_ip: string;
      n_gpu: string;
      gpu_idx?: string[];
      model_path?: string;
      role?: string;
    }[];
  }[];
  peft_model_config?: {
    lora_list?: {
      lora_name: string;
      local_path: string;
    }[];
    image_lora_load_kwargs?: ValueType[];
    image_lora_fuse_kwargs?: ValueType[];
  };
  kwargs?: {
    enable_thinking?: boolean;
    reasoning_content?: boolean;
    cpu_offload?: boolean;
    enable_ib?: boolean;
    quantization_config?: ValueType[];
    extend_config?: ValueType[];
    mindIE_config?: [
      {
        ip_address: string;
        port?: string;
        model_total_length?: string;
        npuMemSize?: string;
        model_input_length?: string;
        model_output_length?: string;
      },
    ];
  };
  virtual_env_config?: {
    enable_virtual_env?: string;
    virtual_env_packages?: ValueType[];
    envs?: ValueType[];
  };
}
type DeployInitialValues = {
  model_name?: string;
  model_version?: string;
  model_engine?: string;
  download_hub?: string;
  model_uid?: string;
  [key: string]: unknown;
};

export interface InstanceModalProps {
  type?: 'add' | 'edit';
  /** 模型类型 */
  modelType: ModelType;
  initialValues: DeployInitialValues;
  /** 模型基本信息 */
  modelData?: Partial<ModelData>;
  children?: React.ReactNode;
  open?: boolean;
  /** drawer=右侧滑出；embedded=嵌在模型详情 Tab */
  variant?: 'drawer' | 'embedded';
  deployDoneGotoInstance?: boolean;
  onCancel?: () => void;
  submitCallBack?: (
    result: { success: boolean; data: { model_uid: string; detail?: string } },
    // values: SubmitValues,
  ) => void;
}
const DeployModelInstance: FC<InstanceModalProps> = (props) => {
  const { modal } = App.useApp();
  const {
    initialValues = {},
    children,
    type = 'add',
    modelType,
    modelData,
    open,
    variant = 'drawer',
    deployDoneGotoInstance = true,
    onCancel,
    submitCallBack,
  } = props;
  const isEdit = type === 'edit';
  const isEmbedded = variant === 'embedded';
  /** 大语言模型/自定义的大语言模型；仓库 custom Tab 的 URL 可能是 custom */
  const deployModelType =
    (modelType as string) === 'custom' ? ModelType.LLM : modelType;
  const initValues = {
    model_uid: initialValues.model_name,
    model_type: deployModelType,
    replica: 1,
    replica_concurrency: 1,
    replica_config: initReplicaConfigValue,
    kwargs: {
      enable_thinking: true,
    },
    ...initialValues,
  };
  const [innerVisible, setInnerVisible] = useState(false);
  const [activePanel, setActivePanel] = useState<'form' | 'yaml'>('form');
  const visible = isEmbedded ? true : open !== undefined ? open : innerVisible;
  // history 回填不进 Spin，只允许打开后、未 dirty 时回填一次
  const historyFilledRef = useRef(false);
  useEffect(() => {
    if (!visible) {
      setActivePanel('form');
      historyFilledRef.current = false;
    }
  }, [visible]);
  const isLLM = deployModelType === ModelType.LLM;
  const isImage = deployModelType === ModelType.image;
  const isEmbedding = deployModelType === ModelType.embedding;
  const isVideo = deployModelType === ModelType.video;
  // Container-backed modalities require model_engine (align CONTAINER_READY_MODEL_TYPES).
  const showEngine = [
    ModelType.LLM,
    ModelType.embedding,
    ModelType.rerank,
    ModelType.audio,
    ModelType.image,
    ModelType.video,
    ModelType.flexible,
  ].includes(deployModelType);
  const [form] = Form.useForm();
  const { hamiEnabled } = useK8sRuntime();
  const replicaConfigWatch = Form.useWatch('replica_config', form);
  const { data: hamiStock } = useRequest(
    async () => {
      const all: Array<Record<string, unknown>> = [];
      let offset = 0;
      for (let i = 0; i < 3; i += 1) {
        const res = await request<{
          data: { data: { next_offset: number | null; results: Array<Record<string, unknown>> } };
        }>('/monitor/cluster/gpus', {
          params: { offset, limit: 200 },
          skipNotification: true,
        });
        const page = res?.data?.data;
        all.push(...(page?.results || []));
        if (page?.next_offset == null) break;
        offset = page.next_offset;
      }
      return all;
    },
    { ready: visible && hamiEnabled },
  );
  const hamiQuota = useMemo(() => {
    const devices = (replicaConfigWatch || []).flatMap(
      (r: { devices?: Array<Record<string, unknown>> }) => r?.devices || [],
    );
    return evaluateHamiQuota(devices as HamiDeviceReq[], hamiStock as HamiStockRow[] | undefined);
  }, [replicaConfigWatch, hamiStock]);
  const hamiBlock = Boolean(hamiEnabled && !hamiQuota.ok);

  const otherParamsRef = useRef<OtherParamsMethod>(null);
  const modalBodyRef = useRef<HTMLDivElement>(null);
  const engineManifestsRef = useRef<EngineManifestDraft[]>([]);
  const [seedManifests, setSeedManifests] = useState<EngineManifestDraft[]>([]);
  const modelEngineValue = Form.useWatch('model_engine', form);
  const modelVersionValue = Form.useWatch('model_version', form);
  const modelNameValue = Form.useWatch('model_name', form);
  const enableThinkingValue = Form.useWatch(['kwargs', 'enable_thinking'], form);
  const setVisible = (v: boolean) => {
    if (open === undefined) setInnerVisible(v);
    else if (!v) onCancel?.();
  };
  // 详情 != form 格式，需进行转换
  const asRecord = (v: unknown): Record<string, unknown> =>
    v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
  const transformDetailToForm = (detail: Record<string, unknown>) => {
    const src = detail || {};
    const kwargs = asRecord(src.kwargs);
    const peft_model_config = asRecord(src.peft_model_config);
    const virtual_env_config = asRecord(src.virtual_env_config);
    const replica_config = src.replica_config;
    const reset = omit(src, [
      'kwargs',
      'peft_model_config',
      'replica_config',
      'virtual_env_config',
    ]);

    let values: Record<string, unknown> & {
      kwargs?: Record<string, unknown>;
      peft_model_config?: {
        lora_list?: unknown;
        image_lora_load_kwargs?: ValueType[];
        image_lora_fuse_kwargs?: ValueType[];
      };
      virtual_env_config?: {
        enable_virtual_env?: unknown;
        virtual_env_packages?: unknown;
        envs?: ValueType[];
      };
    } = reset;
    // engine_version is stored in instance kwargs after launch; echo to top-level field.
    values.engine_version =
      detail?.engine_version || kwargs?.engine_version || values.engine_version;
    values.replica_config = size(replica_config as object | undefined)
      ? replica_config
      : detail?.replica === 0
      ? []
      : initReplicaConfigValue;
    // 需要展开的折叠面板
    const panelsToOpen = new Set<string>();
    if (!isEmpty(kwargs)) {
      // 传递给推理引擎的附加参数（需要从后端kwargs中过滤出来baseKeyForKwargs, specificKeyForKwargs 剩下的key, 属于附加参数）
      const extend_config = convertObjToFormList(
        omit(kwargs, [
          ...baseKeyForKwargs,
          ...specificKeyForKwargs,
          ...INTERNAL_EXTEND_KWARGS_KEYS,
        ]),
        { scalarsOnly: true },
      );
      if (size(extend_config)) {
        panelsToOpen.add('extend_config');
      }
      const quantization_config = convertObjToFormList(asRecord(kwargs.quantization_config));
      if (size(quantization_config)) {
        panelsToOpen.add('quantization_config');
      }
      values.kwargs = {
        ...pick(kwargs, [...baseKeyForKwargs, ...specificKeyForKwargs]),
        quantization_config,
        extend_config,
      };
    }

    if (!isEmpty(peft_model_config)) {
      const {
        image_lora_load_kwargs = {},
        image_lora_fuse_kwargs = {},
        lora_list = [],
      } = peft_model_config;
      values.peft_model_config = {
        lora_list,
        image_lora_load_kwargs: convertObjToFormList(asRecord(image_lora_load_kwargs)),
        image_lora_fuse_kwargs: convertObjToFormList(asRecord(image_lora_fuse_kwargs)),
      };

      if (size(lora_list as object | undefined)) panelsToOpen.add('lora_list');
      if (size(values.peft_model_config.image_lora_load_kwargs))
        panelsToOpen.add('image_lora_load_kwargs');
      if (size(values.peft_model_config.image_lora_fuse_kwargs))
        panelsToOpen.add('image_lora_fuse_kwargs');
    }
    if (!isEmpty(virtual_env_config)) {
      const { enable_virtual_env, virtual_env_packages = [], envs = {} } = virtual_env_config;
      const packages = Array.isArray(virtual_env_packages) ? virtual_env_packages : [];
      values.virtual_env_config = {
        enable_virtual_env: isBoolean(enable_virtual_env) ? enable_virtual_env : VIRTUAL_ENV_UNSET,
        virtual_env_packages: packages.map((item) => ({ value: String(item) })),
        envs: convertObjToFormList(asRecord(envs)),
      };
      if (size(values.virtual_env_config.envs)) panelsToOpen.add('envs_config');
      if (
        values.kwargs?.enable_ib ||
        envsSuggestIbEnabled(values.virtual_env_config.envs)
      ) {
        values.kwargs = { ...(values.kwargs || {}), enable_ib: true };
      }
    }
    otherParamsRef.current?.updateActivePanels(Array.from(panelsToOpen));
    form.setFieldsValue(values);
    // 回填上次部署 YAML（配置历史），避免预览接口覆盖用户/失败现场
    const manifests = detail?.engine_manifests;
    if (Array.isArray(manifests) && manifests.length) {
      const seeded = manifests
        .filter((m: EngineManifestDraft) => m?.key && m?.yaml)
        .map(
          (m: EngineManifestDraft): EngineManifestDraft => ({
            key: String(m.key),
            yaml: String(m.yaml),
            replica_uid: m.replica_uid,
            shard: m.shard,
            role: m.role,
          }),
        );
      engineManifestsRef.current = seeded;
      setSeedManifests(seeded);
    } else {
      engineManifestsRef.current = [];
      setSeedManifests([]);
    }
  };
  useRequest(
    () =>
      request('/instance/history', {
        params: {
          model_name: initialValues?.model_name,
          model_type: modelType,
        },
      }),
    {
      ready: visible && !isEdit,
      onSuccess: (res) => {
        const list = res?.data?.data || [];
        if (!list.length) return;
        // history 不进 Spin：用户先改表时禁止覆盖。无 jest，靠 isFieldsTouched + 只填一次。
        if (historyFilledRef.current || form.isFieldsTouched()) return;
        historyFilledRef.current = true;
        // 入口预填（如下载任务的 model_version）优先于历史记录
        const backfillPath = shouldBackfillModelPath(
          initialValues?.model_version,
          list[0]?.model_version,
        );
        transformDetailToForm({
          ...list[0],
          ...initialValues,
          // Do not seed another instance's YAML on silent history fill.
          engine_manifests: undefined,
          replica_config: mapHistoryReplicaConfig(
            list[0]?.replica_config as FormValues['replica_config'],
            backfillPath,
          ) as FormValues['replica_config'],
          model_path: backfillPath ? list[0]?.model_path : undefined,
          model_version: initialValues?.model_version || list[0]?.model_version,
          model_engine: initialValues?.model_engine || list[0]?.model_engine,
          download_hub: initialValues?.download_hub || list[0]?.download_hub,
        });
      },
    },
  );
  const { data: deviceResult, loading: deviceLoading } = useRequest(
    () =>
      request<{ data: { results: DeviceInfo[] } }>(
        `/device/info/${modelType}/${initialValues.model_name}`,
        {
          params: ALL_LIST_PAGES_PARAMS,
        },
      ),
    {
      ready: visible,
      cacheKey: `device-info-${modelType}-${initialValues.model_name}`,
      staleTime: 30_000,
    },
  );
  const devices = deviceResult?.data?.results || [];
  // 仓库列表轻量 card 带 exist_cache 且无 model_specs；此时补拉详情供 gguf/lightning 等字段
  const listCardModelData =
    !!modelData &&
    'exist_cache' in modelData &&
    !Array.isArray((modelData as { model_specs?: unknown }).model_specs);
  const needFetchModelDetail = visible && (!('modelData' in props) || listCardModelData);
  const { data: modelDetailForFetch, loading: modelDetailLoading } = useRequest(
    () => request(`/model_registrations/${modelType}/${initialValues.model_name}`),
    { ready: needFetchModelDetail },
  );
  const { model_ability, gguf_quantizations, lightning_versions } = {
    ...(modelData || {}),
    ...(modelDetailForFetch?.data?.data?.model_data || {}),
  };
  const hasHybrid = (model_ability || []).includes('hybrid');
  const hasReasoning = (model_ability || []).includes('reasoning');
  const { loading: instanceDetailLoading } = useRequest(
    () =>
      request(
        `/models/instances/${encodeURIComponent(String(initialValues?.model_uid || ''))}`,
      ),
    {
      ready: isEdit && visible && !!initialValues?.model_uid,
      onSuccess: (res) => {
        if (!res?.success) return;
        const detail = res?.data?.data || {};
        if (!detail?.model_uid) return;
        // Same dirty guard as history (#5): late ERROR snapshot must not clobber edits.
        if (historyFilledRef.current || form.isFieldsTouched()) return;
        historyFilledRef.current = true;
        transformDetailToForm({ ...initialValues, ...detail });
      },
    },
  );

  const { data: enginesResult } = useRequest(
    () => request(`/engines/${deployModelType}/${modelNameValue}`),
    {
      ready: showEngine && !!modelNameValue && visible,
      cacheKey: `engines-${deployModelType}-${modelNameValue}`,
      staleTime: 60_000,
    },
  );
  const enginesConfig = (enginesResult?.data || {}) as ModelEngines;

  const { data: engineCatalogResult } = useRequest(() => listEngineImages(), {
    ready: showEngine && visible,
    cacheKey: 'engine-images-catalog',
    staleTime: 60_000,
  });
  const engineCatalogList = (engineCatalogResult?.data?.list || []) as EngineImageSpec[];

  const { data: engineLocalResult } = useRequest(() => listLocalEngineImages(), {
    ready: showEngine && visible,
    cacheKey: 'engine-images-local-deploy',
    staleTime: 30_000,
  });
  const localEngineImages = useMemo(() => {
    const data = engineLocalResult?.data as LocalEngineImagesResult | undefined;
    return (data?.images || []) as string[];
  }, [engineLocalResult]);

  /** 与「模型引擎 → 引擎仓库映射」对齐：下拉只展示映射里启用的引擎 */
  const { data: engineCompatResult } = useRequest(
    async () => {
      const res = await getEngineCompat();
      if (!res?.success) return null;
      return res.data as EngineCompatResult;
    },
    {
      ready: showEngine && visible,
      cacheKey: 'engine-images-compat',
      staleTime: 30_000,
    },
  );

  const { data: engineRegistry } = useRequest(
    async () => {
      const res = await getEngineRegistry();
      if (!res?.success) return null;
      return res.data as { registry?: string; repos?: Record<string, string> };
    },
    {
      ready: showEngine && visible,
      cacheKey: 'engine-images-registry',
      staleTime: 60_000,
    },
  );
  const registryEngineKeys = useMemo(() => {
    const repos = engineRegistry?.repos || {};
    return Object.keys(repos)
      .map((k) => k.trim())
      .filter(Boolean)
      .sort(compareEngines);
  }, [engineRegistry]);

  const allowedEnginesByWorkers = useMemo(() => {
    const workers = engineCompatResult?.workers || [];
    if (!workers.length) return null;
    const selected: string[] = [];
    for (const rc of replicaConfigWatch || []) {
      for (const d of rc?.devices || []) {
        const ip = String(d?.worker_ip || '').trim();
        if (ip && ip !== 'auto') selected.push(ip);
      }
    }
    const matchWorker = (ip: string) =>
      workers.find(
        (w) =>
          w.worker_address === ip ||
          String(w.worker_address || '').startsWith(`${ip}:`),
      );
    const intersect = (pools: string[][]) => {
      if (!pools.length) return new Set<string>();
      const set = new Set(pools[0].map((e) => e.toLowerCase()));
      for (const pool of pools.slice(1)) {
        const next = new Set(pool.map((e) => e.toLowerCase()));
        for (const key of Array.from(set)) {
          if (!next.has(key)) set.delete(key);
        }
      }
      return set;
    };
    // auto / 未指定：并集；指定 Worker：该 Worker（或多 Worker 交集）
    if (!selected.length) {
      const pools = workers
        .map((w) => w.allowed_engines || [])
        .filter((p) => p.length > 0);
      if (!pools.length) return null;
      const union = new Set<string>();
      for (const pool of pools) {
        pool.forEach((e) => union.add(e.toLowerCase()));
      }
      return union;
    }
    const matched = selected.map(matchWorker);
    if (matched.every((w) => !w)) return null;
    return intersect(
      matched.filter(Boolean).map((w) => w?.allowed_engines || []),
    );
  }, [engineCompatResult, replicaConfigWatch]);

  const catalogEngineKeys = useMemo(() => {
    const keys = Array.from(
      new Set(
        (engineCatalogList || [])
          .map((s) => (s.engine || '').trim())
          .filter(Boolean),
      ),
    );
    return keys.sort(compareEngines);
  }, [engineCatalogList]);

  const enginesList = useMemo(() => {
    const preferredVersion = modelVersionValue || initialValues?.model_version || '';
    const rows: {
      label: string;
      value: string;
      disabled?: boolean;
      supportsVersion?: boolean;
    }[] = [];

    // /engines 有内置规格（LLM 等）：走规格/仓库映射；
    // audio/image/video 等常返回空 → 改用引擎目录（whisper/diffusers/tei…），勿再用
    // LLM 仓库映射把全部标成「无规格」。
    const hasBuiltinSpecs = Object.values(enginesConfig || {}).some(
      (v) => Array.isArray(v) && v.length > 0,
    );

    let sourceKeys: string[];
    if (hasBuiltinSpecs) {
      sourceKeys =
        registryEngineKeys.length > 0
          ? registryEngineKeys
          : Object.keys(enginesConfig || {}).sort(compareEngines);
    } else if (catalogEngineKeys.length > 0) {
      const preferred = (PREFERRED_CATALOG_ENGINES_BY_TYPE[modelType] || []).map(
        (e) => e.toLowerCase(),
      );
      if (preferred.length) {
        const preferredSet = new Set(preferred);
        const matched = catalogEngineKeys.filter((k) =>
          preferredSet.has(k.toLowerCase()),
        );
        sourceKeys = matched.length > 0 ? matched : catalogEngineKeys;
      } else {
        sourceKeys = catalogEngineKeys;
      }
    } else {
      sourceKeys = registryEngineKeys;
    }

    if (allowedEnginesByWorkers) {
      sourceKeys = sourceKeys.filter((k) =>
        allowedEnginesByWorkers.has(String(k).toLowerCase()),
      );
    }

    sourceKeys.forEach((regKey) => {
      const entry = findEngineConfig(enginesConfig, regKey);
      const apiKey = entry?.key || regKey;
      let value = entry?.value;
      if (
        (!Array.isArray(value) || !value.length) &&
        String(regKey).toLowerCase() === 'vllm-ascend'
      ) {
        value = findEngineConfig(enginesConfig, 'vllm')?.value;
      }
      const hasSpecs = Array.isArray(value) && value.length > 0;
      const inCatalog = catalogEngineKeys.some(
        (k) => k.toLowerCase() === String(regKey).toLowerCase(),
      );
      const hasLocalImage = localEngineImages.some((img) =>
        localImageBelongsToEngine(
          img,
          regKey,
          engineCatalogList,
          engineRegistry?.repos,
        ),
      );
      const versions = hasSpecs ? buildLlmVersionOptions(value) : [];
      const supportsVersion = preferredVersion
        ? versions.some((v) => v.value === preferredVersion)
        : false;
      // 内置规格：无 list 时若本机/目录仍有该引擎镜像则可选（如镜像源自定义 vllm-lmcache）
      // 无内置规格：Worker allowed ∩ registry 仍可选引擎（版本可空/latest），不要一律 disabled
      const disabled = hasBuiltinSpecs
        ? !hasSpecs && !hasLocalImage && !inCatalog
        : false;
      rows.push({
        label: `${engineDisplayLabel(apiKey)}${
          supportsVersion
            ? ` · ${lGet('models.deploy.engineSupportsVersion')}`
            : disabled
              ? ` (${lGet('models.deploy.engineNoSpecs')})`
              : ''
        }`,
        value: apiKey,
        disabled,
        supportsVersion,
      });
    });

    // 固定顺序：vllm → transformers → mindie → …；同序时匹配版本优先
    return rows.sort((a, b) => {
      const byEngine = compareEngines(a.value, b.value);
      if (byEngine !== 0) return byEngine;
      return Number(b.supportsVersion) - Number(a.supportsVersion);
    });
  }, [
    enginesConfig,
    registryEngineKeys,
    catalogEngineKeys,
    engineCatalogList,
    engineRegistry?.repos,
    localEngineImages,
    modelType,
    modelVersionValue,
    initialValues?.model_version,
    allowedEnginesByWorkers,
  ]);

  const engineVersionOptions = useMemo(() => {
    if (!modelEngineValue) return [];
    return buildEngineVersionOptions({
      engine: String(modelEngineValue),
      catalog: engineCatalogList,
      localImages: localEngineImages,
      registryRepos: engineRegistry?.repos || undefined,
    });
  }, [
    engineCatalogList,
    engineRegistry?.repos,
    localEngineImages,
    modelEngineValue,
  ]);

  // 无内置规格且仅一个可选引擎时（如 audio→whisper、video→diffusers）自动选中
  useEffect(() => {
    if (isEdit || !visible || !showEngine) return;
    if (form.getFieldValue('model_engine')) return;
    const enabled = enginesList.filter((e) => !e.disabled);
    if (enabled.length === 1) {
      form.setFieldValue('model_engine', enabled[0].value);
    }
  }, [enginesList, form, isEdit, showEngine, visible]);

  // 指定 Worker 后当前引擎不在允许列表：清空，避免提交被后端拒绝
  useEffect(() => {
    if (!visible || !showEngine || !modelEngineValue) return;
    if (!allowedEnginesByWorkers) return;
    if (allowedEnginesByWorkers.has(String(modelEngineValue).toLowerCase())) {
      return;
    }
    form.setFieldValue('model_engine', undefined);
    form.setFieldValue('engine_version', undefined);
  }, [
    allowedEnginesByWorkers,
    form,
    modelEngineValue,
    showEngine,
    visible,
  ]);

  // Prefer a locally ready version when user has not picked one yet.
  useEffect(() => {
    if (isEdit || !visible || !showEngine || !modelEngineValue) return;
    const current = form.getFieldValue('engine_version');
    if (current) return;
    const ready = engineVersionOptions.filter((o) => o.ready);
    const pick =
      ready.find(
        (o) =>
          o.value === 'latest' ||
          String(o.value).endsWith(':latest') ||
          String(o.label).startsWith('latest '),
      ) || (ready.length === 1 ? ready[0] : undefined);
    if (pick) {
      form.setFieldValue('engine_version', pick.value);
    }
  }, [
    engineVersionOptions,
    form,
    isEdit,
    modelEngineValue,
    showEngine,
    visible,
  ]);

  // LLM / 非 LLM 均拉 versions，用于版本下拉的 cache_status / model_file_location
  const { data: versionsResult } = useRequest(
    () =>
      request(`/models/${deployModelType}/${modelNameValue}/versions`, {
        params: ALL_LIST_PAGES_PARAMS,
      }),
    {
      ready: !!modelNameValue && visible,
      cacheKey: `model-versions-${deployModelType}-${modelNameValue}`,
      staleTime: 30_000,
    },
  );
  const versionRows = (versionsResult?.data?.results || []) as Array<
    ModelVersionListItem & ModelVersionTableListItem
  >;
  const versionLocationMap = useMemo(() => {
    const map: Record<string, Record<string, string>> = {};
    versionRows.forEach((item) => {
      const ver = item?.model_version;
      const loc = item?.model_file_location;
      if (!ver || !loc || typeof loc !== 'object') return;
      map[ver] = loc as Record<string, string>;
    });
    return map;
  }, [versionRows]);
  const versionList = useMemo(() => {
    if (isLLM) {
      // 以 /engines 返回的规格为准（name--sizeB--format--quant）；引擎键大小写不敏感
      let entry = findEngineConfig(enginesConfig, modelEngineValue);
      if (
        (!entry || typeof entry.value === 'string' || !Array.isArray(entry.value) || !entry.value.length) &&
        String(modelEngineValue || '').toLowerCase() === 'vllm-ascend'
      ) {
        entry = findEngineConfig(enginesConfig, 'vllm');
      }
      if (entry && Array.isArray(entry.value) && entry.value.length) {
        return buildLlmVersionOptions(entry.value).map((opt) => ({
          ...opt,
          model_file_location: versionLocationMap[opt.value],
          cache_status: opt.cache_status || !!versionLocationMap[opt.value],
        }));
      }
    }
    return versionRows.map((item) => ({
      label: item.model_version,
      value: item.model_version,
      cache_status: item?.cache_status,
      model_file_location: item?.model_file_location as Record<string, string> | undefined,
    }));
  }, [isLLM, modelEngineValue, versionRows, enginesConfig, versionLocationMap]);

  /** 客户未改模型地址则保持为空；换版本时清掉上次部署/回填残留，手填则保留 */
  const clearDefaultModelPathsIfUnedited = () => {
    if (isEdit) return;
    const replicaConfig = (form.getFieldValue('replica_config') ||
      []) as FormValues['replica_config'];
    const touched = replicaConfig.some((rc, ri) =>
      (rc.devices || []).some((_, di) =>
        form.isFieldTouched(['replica_config', ri, 'devices', di, 'model_path']),
      ),
    );
    if (touched) return;
    let changed = false;
    const next = replicaConfig.map((rc) => ({
      ...rc,
      devices: (rc.devices || []).map((d) => {
        if (!d.model_path) return d;
        changed = true;
        return { ...d, model_path: undefined };
      }),
    }));
    if (changed) form.setFieldValue('replica_config', next);
  };

  useEffect(() => {
    if (!visible || isEdit) return;
    clearDefaultModelPathsIfUnedited();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, isEdit, modelVersionValue]);

  const downloadHubOptions = useMemo(() => {
    const fromDetail = (
      (modelDetailForFetch?.data?.data?.model_data as { download_hubs?: string[] } | undefined)
        ?.download_hubs ||
      (modelData as { download_hubs?: string[] } | undefined)?.download_hubs ||
      []
    ).filter(Boolean);
    const hubs = fromDetail.length ? fromDetail : MODEL_DOWNLOAD_HUB;
    return hubs.map((hub) => ({
      value: hub,
      label: MODEL_DOWNLOAD_HUB_LABELS[hub] || hub,
    }));
  }, [modelDetailForFetch, modelData]);

  // 预填版本存在时，在「仓库映射」引擎中自动选中支持该版本的一项
  useEffect(() => {
    if (!visible || isEdit || !isLLM) return;
    const preferred =
      form.getFieldValue('model_version') || initialValues?.model_version || '';
    if (!preferred || !Object.keys(enginesConfig || {}).length) return;

    const allowed =
      registryEngineKeys.length > 0
        ? new Set(registryEngineKeys.map((k) => k.toLowerCase()))
        : null;

    const currentEngine = form.getFieldValue('model_engine');
    const currentEntry = findEngineConfig(enginesConfig, currentEngine);
    const currentAllowed =
      !allowed ||
      (currentEntry && allowed.has(currentEntry.key.toLowerCase())) ||
      (currentEngine && allowed.has(String(currentEngine).toLowerCase()));
    const currentOk =
      currentAllowed &&
      currentEntry &&
      Array.isArray(currentEntry.value) &&
      buildLlmVersionOptions(currentEntry.value).some((v) => v.value === preferred);
    if (currentOk) {
      if (currentEntry && currentEntry.key !== currentEngine) {
        form.setFieldValue('model_engine', currentEntry.key);
      }
      return;
    }
    for (const [key, value] of Object.entries(enginesConfig)) {
      if (allowed && !allowed.has(key.toLowerCase())) continue;
      if (!Array.isArray(value)) continue;
      if (buildLlmVersionOptions(value).some((v) => v.value === preferred)) {
        form.setFieldValue('model_engine', key);
        form.setFieldValue('model_version', preferred);
        break;
      }
    }
  }, [
    visible,
    isEdit,
    isLLM,
    enginesConfig,
    registryEngineKeys,
    initialValues?.model_version,
  ]);

  // 处理模版引擎改变：仅当当前版本不在新引擎规格内时才清空
  const handleEngineChange = () => {
    if (isEmbedding) return;
    form.setFieldValue('engine_version', undefined);
    const preferred = form.getFieldValue('model_version');
    if (!preferred) return;
    const entry = findEngineConfig(enginesConfig, form.getFieldValue('model_engine'));
    const ok =
      entry &&
      Array.isArray(entry.value) &&
      buildLlmVersionOptions(entry.value).some((v) => v.value === preferred);
    if (!ok) form.setFieldValue('model_version', undefined);
  };

  // vLLM / vLLM-ascend：前三项固定 max_model_len / gpu_memory_utilization / max_num_seqs
  // GGUF + vLLM：再补 dtype=float16（置顶 + 星标）
  useEffect(() => {
    if (!visible || !isLLM) return;
    const current: ValueType[] = form.getFieldValue(['kwargs', 'extend_config']) || [];
    let next = current;
    if (isVllmFamilyEngine(modelEngineValue)) {
      next = ensureVllmDefaultExtend(next);
    }
    if (needsGgufVllmDtype(modelEngineValue, modelVersionValue)) {
      next = ensureGgufVllmDtype(next);
    }
    if (extendConfigEquals(next, current)) return;
    form.setFieldValue(['kwargs', 'extend_config'], next);
    otherParamsRef.current?.updateActivePanels(['extend_config']);
  }, [visible, isLLM, modelVersionValue, modelEngineValue, form]);

  const handleOpen = () => {
    setInnerVisible(true);
  };
  const handleCancel = () => {
    // 关闭弹窗保留表单与 YAML 草稿，便于部署失败后继续排查；成功提交后再清空
    setVisible(false);
  };

  const { runAsync, loading } = useRequest(
    (data) => request('/models/instance', { data, method: isEdit ? 'put' : 'post' }),
    { manual: true },
  );

  const handleSubmit = async (values: FormValues, fromYaml = false) => {
    if (showEngine && !isEdit && !values.engine_version) {
      message.warning(
        String(
          lGet('models.deploy.engineVersion.recommend',
          ),
        ),
      );
    }
    // YAML 部署不校验表单；表单部署仍检查同一副本 Worker 不重复
    for (const rc of fromYaml ? [] : values.replica_config || []) {
      const ips = (rc?.devices || [])
        .map((d: { worker_ip?: string }) => d?.worker_ip)
        .filter((ip: string) => ip && ip !== 'auto');
      if (ips.length > 1 && new Set(ips).size !== ips.length) {
        message.error(
          String(
            lGet('models.deploy.replicaConfig.duplicateWorker',
            ),
          ),
        );
        return;
      }
    }
    let { kwargs, peft_model_config, replica_config, virtual_env_config, ...resetValues } = values;
    if (isLLM && isVllmFamilyEngine(resetValues.model_engine)) {
      kwargs = {
        ...kwargs,
        extend_config: ensureVllmDefaultExtend(kwargs?.extend_config),
      };
    }
    if (isLLM && needsGgufVllmDtype(resetValues.model_engine, resetValues.model_version)) {
      kwargs = {
        ...kwargs,
        extend_config: ensureGgufVllmDtype(kwargs?.extend_config),
      };
    }
    // lora_list还是按照FormList的格式传给后代，image_lora_load_kwargs，image_lora_fuse_kwargs需转化为对象形式：[{key: 'min', value: 1}] => { min: 1}
    const entries = Object.entries(peft_model_config || {}).map(([key, value]) => {
      if (!size(value)) return [key, undefined];
      if (['image_lora_load_kwargs', 'image_lora_fuse_kwargs'].includes(key)) {
        return [key, transformFormListToObj(value as ValueType[])];
      }
      return [key, value];
    });
    const transformed = Object.fromEntries(entries);
    // 如果所有相关字段都为空，则清空 peft_model_config
    peft_model_config = Object.values(transformed).some((val) =>
      size(val as object | string | null | undefined),
    )
      ? transformed
      : undefined;

    // 表单部署：不传 overlay，后端按表单重生并覆盖卡片 YAML。
    // YAML 部署：只传当前清单，不校验、不用表单覆盖。
    const drafts = engineManifestsRef.current || [];
    let engine_manifests: { key: string; yaml: string }[] = [];
    if (fromYaml) {
      const yaml =
        drafts.find((m) => m.yaml)?.yaml || drafts[0]?.yaml || '';
      if (!String(yaml).trim()) {
        message.warning(
          String(lGet('models.deploy.yaml.empty')),
        );
        return;
      }
      engine_manifests = [
        { key: String(resetValues.model_uid || values.model_uid || ''), yaml },
      ];
    }

    const data = omitLegacyFlatLaunchFields({
      ...resetValues,
      model_version: resetValues.model_version || undefined,
      engine_manifests,
      peft_model_config,
      // 传递给推理引擎的量化参数, 传递给推理引擎的附加参数, mindIE配置 都要放在kwargs 中
      kwargs: {
        ...pick(kwargs, baseKeyForKwargs),
        // 过滤掉 空，null, undefined
        mindIE_config: size(kwargs?.mindIE_config)
          ? (kwargs?.mindIE_config || []).map((item) =>
              omitBy(item, (value) => value === '' || isNil(value)),
            )
          : undefined,
        quantization_config: size(kwargs?.quantization_config)
          ? transformFormListToObj(kwargs?.quantization_config)
          : undefined,
        ...(size(kwargs?.extend_config)
          ? transformExtendFormListToObj(kwargs?.extend_config)
          : {}),
      },
      replica_config: (replica_config || []).map((item) => ({
        replica_uid: item.replica_uid || null,
        devices: (item.devices || [])
          .filter((device) => !isEmpty(device))
          .map((item) => {
            const device: Record<string, unknown> = {
              ...item,
              role: item?.role || undefined,
              n_gpu: item.n_gpu !== 'auto' ? Number(item.n_gpu) : item.n_gpu,
              model_path: (item.model_path || '').trim() || undefined,
            };
            // Path A (Docker / no-hami / NPU): quota is K8s-only, never submit.
            if (!hamiEnabled) {
              delete device.gpu_cores;
              delete device.gpu_type;
              delete device.gpu_mem_gb;
              delete device.gpu_mem_mib;
            }
            return device;
          }),
      })),
      // LLM 容器路径：虚环开关/包不提交；仅 env 注入引擎容器
      virtual_env_config: isLLM
        ? {
            envs: size(virtual_env_config?.envs)
              ? transformFormListToStringObj(virtual_env_config?.envs)
              : undefined,
          }
        : {
            enable_virtual_env:
              virtual_env_config?.enable_virtual_env === VIRTUAL_ENV_UNSET
                ? undefined
                : virtual_env_config?.enable_virtual_env,
            virtual_env_packages: (virtual_env_config?.virtual_env_packages || [])
              .map((item) => item.value)
              .filter(Boolean),
            envs: size(virtual_env_config?.envs)
              ? transformFormListToStringObj(virtual_env_config?.envs)
              : undefined,
          },
    });
    // console.log(data);
    // return;
    const res = await runAsync(data);
    if (res.success) {
      swrInvalidate('repo-list');
      setVisible(false);
      form.resetFields();
      engineManifestsRef.current = [];
      setSeedManifests([]);
      if (isEdit) {
        message.success(lGet('models.instances.edit.success'));
      } else {
        // Mark so READY → warmup prompt still fires after navigating away mid-launch
        if (data.model_uid) markPendingDeployReady(String(data.model_uid));
        modal.success({
          title: lGet('models.instances.create.accepted',
          ),
          content: `Model UID:${data.model_uid} ${lGet('models.instances.create.accepted.tips',
          )}`,
          ...(deployDoneGotoInstance
            ? {
                okText: lGet('models.instances.create.success.okbtn'),
                onOk: () => history.push(`/models/instances?id=${data?.model_uid}`),
              }
            : {}),
        });
      }
      submitCallBack?.(res);
    } else {
      // 失败不清理表单 / YAML / 配置历史，便于对照修改后重试
      const detail =
        readResponseDetail(res) || lGet('models.instances.create.failed');
      message.error(detail);
    }
  };
  const handleDeployClick = () => {
    if (activePanel === 'yaml') {
      const values = form.getFieldsValue(true) as FormValues;
      if (!values.model_uid) {
        message.warning(String(l('models.deploy.instanceName.tips')));
        return;
      }
      void handleSubmit(values, true);
      return;
    }
    if (hamiBlock) return;
    form.submit();
  };
  const onFinishFailed: FormProps['onFinishFailed'] = (errorInfo) => {
    const errorFields = errorInfo.errorFields;
    const panelsToOpen = new Set<string>();
    const peftKeys = {
      lora_list: 'lora_list',
      image_lora_load_kwargs: 'image_lora_load_kwargs',
      image_lora_fuse_kwargs: 'image_lora_fuse_kwargs',
    };
    const virtualKeys = {
      envs: 'envs_config',
    };
    const kwargsKeys = {
      extend_config: 'extend_config',
      quantization_config: 'quantization_config',
    };
    errorFields.forEach(({ name }: { name: (string | number)[] }) => {
      // 匹配 peft_model_config 下的子项
      if (name.length >= 2 && name[0] === 'peft_model_config' && name[1] in peftKeys) {
        panelsToOpen.add(peftKeys[name[1] as keyof typeof peftKeys]);
      }
      if (name.length >= 2 && name[0] === 'kwargs' && name[1] in kwargsKeys) {
        panelsToOpen.add(kwargsKeys[name[1] as keyof typeof kwargsKeys]);
      }
      if (name.length >= 2 && name[0] === 'virtual_env_config' && name[1] in virtualKeys) {
        panelsToOpen.add(virtualKeys[name[1] as keyof typeof virtualKeys]);
      }
    });
    // 把含有错误表单的折叠面板打开
    otherParamsRef.current?.updateActivePanels(Array.from(panelsToOpen));
  };
  const onCommandSubmitBack = (formValues: Record<string, unknown>) => {
    transformDetailToForm(formValues);
  };

  const formFields = (
    <>
          <div className="mx-4 w-full font-medium text-base mb-2">
            {l('models.deploy.baseinfo')}
          </div>
          <ProFormText
            label={l('models.deploy.instanceName')}
            name="model_uid"
            placeholder={l('models.deploy.instanceName.tips')}
            disabled={isEdit}
            colProps={{ span: 12 }}
            rules={[
              {
                required: true,
                message: l('models.deploy.instanceName.tips'),
              },
            ]}
          />
          <ProFormText hidden name="model_type" colProps={{ span: 0 }} />
          <ProFormText
            label={l('models.deploy.modelName')}
            name="model_name"
            colProps={{ span: 12 }}
            disabled
          />
          {showEngine && (
            <ProFormSelect
              label={l('models.deploy.modelEngine')}
              name="model_engine"
              placeholder={l('models.deploy.modelEngine.tips')}
              options={enginesList}
              rules={[
                {
                  required: showEngine,
                  message: l('models.deploy.modelEngine.tips'),
                },
              ]}
              colProps={{ span: 12 }}
              disabled={isEdit}
              fieldProps={{
                onChange: handleEngineChange,
              }}
            />
          )}
          {showEngine && (
            <ProFormSelect
              label={l('models.deploy.engineVersion')}
              name="engine_version"
              placeholder={l('models.deploy.engineVersion.tips')}
              options={engineVersionOptions}
              colProps={{ span: 12 }}
              disabled={isEdit || !modelEngineValue}
              tooltip={l('models.deploy.engineVersion.autoPullHint')}
              fieldProps={{
                allowClear: true,
                optionRender: (option) => (
                  <div className="flex flex-col" title="">
                    <span>{option.label}</span>
                    {option.data?.image && (
                      <span className="text-xs text-gray-400 truncate">{option.data.image}</span>
                    )}
                  </div>
                ),
              }}
            />
          )}
          <ProFormSelect
            label={l('models.deploy.modelVersion')}
            name="model_version"
            placeholder={l('models.deploy.modelVersion.tips')}
            disabled={isEdit}
            options={versionList}
            colProps={{ span: 12 }}
            rules={[
              {
                required: activePanel === 'form',
                message: l('models.deploy.modelVersion.tips'),
              },
            ]}
            fieldProps={{
              onChange: () => {
                clearDefaultModelPathsIfUnedited();
              },
              optionRender: (option) => {
                return (
                  <div
                    className="flex justify-between items-center gap-x-[8px]"
                    key={option.value}
                    title=""
                  >
                    <Tooltip
                      overlayStyle={{ maxWidth: 'none' }}
                      mouseEnterDelay={0.5}
                      title={<div className="whitespace-nowrap">{option.label}</div>}
                    >
                      <span className="flex-1 truncate">{option.label}</span>
                    </Tooltip>
                    <Tag
                      className="shrink-0 !mr-0 !font-normal"
                      color={option.data?.cache_status ? 'green' : 'default'}
                    >
                      {option.data?.cache_status
                        ? l('models.repository.cached')
                        : l('models.repository.notCached')}
                    </Tag>
                  </div>
                );
              },
            }}
          />
          <ProFormSelect
            label={l('models.deploy.downloadHub')}
            name="download_hub"
            placeholder={l('models.deploy.downloadHub.placeholder')}
            disabled={isEdit}
            colProps={{ span: 12 }}
            options={downloadHubOptions}
            fieldProps={{ allowClear: true }}
          />
          <ProFormDigit
            label={l('models.deploy.requestLimits')}
            name="request_limits"
            placeholder={l('models.deploy.requestLimits.placeholder')}
            colProps={{ span: 12 }}
          />
          {/* LLM / embedding / rerank / 图音视频共用。默认 1；显式 N 才并行。不设 allow_batch。 */}
          <ProFormDigit
            label={l('models.deploy.replicaConcurrency')}
            name="replica_concurrency"
            placeholder={l('models.deploy.replicaConcurrency.placeholder')}
            tooltip={l('models.deploy.replicaConcurrency.placeholder')}
            colProps={{ span: 12 }}
            min={1}
            fieldProps={{ precision: 0 }}
          />
          {hasHybrid && (
            <ProFormSwitch
              name={['kwargs', 'enable_thinking']}
              label={l('models.deploy.enableThinking')}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
              colProps={{ span: 12 }}
            />
          )}
          {(hasHybrid ? hasReasoning && enableThinkingValue : hasReasoning) && (
            <ProFormSwitch
              name={['kwargs', 'reasoning_content']}
              label={l('models.deploy.reasoningContent')}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
              colProps={{ span: 12 }}
            />
          )}
          {(isImage || isVideo) && (
            <ProFormSwitch
              name={['kwargs', 'cpu_offload']}
              label={l('models.deploy.cpuOffload')}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
              tooltip={l('models.deploy.cpuOffload.tips')}
              colProps={{ span: 12 }}
            />
          )}
          {!!size(gguf_quantizations) && (
            <>
              <ProFormSelect
                label={l('models.deploy.ggufQuantizations')}
                name="gguf_quantization"
                colProps={{ span: 12 }}
                options={gguf_quantizations}
              />
              <ProFormText
                label={l('models.deploy.ggufModelPath')}
                name="gguf_model_path"
                colProps={{ span: 12 }}
              />
            </>
          )}
          {!!size(lightning_versions) && (
            <>
              <ProFormSelect
                label={l('models.deploy.lightningVersion')}
                name="lightning_version"
                colProps={{ span: 12 }}
                options={lightning_versions}
              />
              <ProFormText
                label={l('models.deploy.lightningModelPath')}
                name="lightning_model_path"
                colProps={{ span: 12 }}
              />
            </>
          )}
          <ReplicaInfo
            form={form}
            devices={devices}
            modelName={modelNameValue}
            modalBodyRef={modalBodyRef.current}
          />
          {hamiEnabled ? (
            <div className="col-span-full mx-2 mb-2 text-xs">
              {hamiBlock ? (
                <p className="text-error">
                  {hamiQuota.reason === 'capacity'
                    ? l('models.deploy.hami.capacityNone',
                      )
                    : l('models.deploy.hami.overQuota',
                      )}
                  {' · '}
                  <a
                    className="text-primary"
                    onClick={() => history.push('/monitor/hami')}
                  >
                    {l('models.deploy.hami.viewPool')}
                  </a>
                </p>
              ) : hamiQuota.fits != null && hamiQuota.fits > 0 ? (
                <p className="text-muted">
                  {l('models.deploy.hami.fits').replace(
                    '{n}',
                    String(hamiQuota.fits),
                  )}
                </p>
              ) : null}
            </div>
          ) : null}
          <div className="w-full mx-2 flex flex-col gap-2">
            {(modelEngineValue || '').toLocaleLowerCase() === 'mindie' && (
              <MindIEParams devices={devices} form={form} />
            )}
            <OtherParams
              form={form}
              modelType={modelType}
              ref={otherParamsRef}
              modalBodyRef={modalBodyRef.current}
            />
          </div>
    </>
  );

  const yamlPanel = (
    <div className="w-full">
      <ManifestYamlPreview
        form={form}
        // 弹窗可见即预览（不依赖 YAML 面板），表单改参可触发 shouldPersist
        open={visible}
        modelType={deployModelType}
        modelAbility={modelData?.model_ability}
        seedManifests={seedManifests}
        onManifestsChange={(items) => {
          engineManifestsRef.current = items;
        }}
      />
    </div>
  );

  const formContent = (
    <ProForm
      form={form}
      layout="vertical"
      grid
      submitter={false}
      initialValues={initValues}
      onFinish={(values) => handleSubmit(values, false)}
      onFinishFailed={onFinishFailed}
      preserve
    >
      {/* ProForm grid 会外包 ant-row；Spin 必须占满 24 列，否则空态按内容宽收缩靠左 */}
      <Col span={24} className="!w-full max-w-full">
        <Spin
          className="w-full [&_.ant-spin-container]:w-full"
          /* history 静默回填，不挡首屏；设备信息仅影响副本区 */
          spinning={instanceDetailLoading || modelDetailLoading || (deviceLoading && !deviceResult)}
        >
          {/* 双面板常挂载：避免切 YAML 时字段卸载导致 model_engine 丢失、预览变空 */}
          <div
            ref={modalBodyRef}
            className={
              isEmbedded
                ? 'w-full overflow-x-hidden'
                : 'w-full max-h-[calc(100vh-220px)] overflow-y-auto overflow-x-hidden'
            }
          >
            <Row
              gutter={[24, 4]}
              style={{ display: activePanel === 'form' ? undefined : 'none' }}
            >
              {formFields}
            </Row>
            <div
              className="w-full"
              style={{ display: activePanel === 'yaml' ? undefined : 'none' }}
            >
              {yamlPanel}
            </div>
          </div>
        </Spin>
      </Col>
    </ProForm>
  );

  const yamlToggleTitle =
    activePanel === 'yaml'
      ? lGet('models.repository.detail.formToggle')
      : lGet('models.repository.detail.yamlToggle');
  const yamlToggle = (
    <ActionWithTips title={yamlToggleTitle} mouseEnterDelay={0.1} placement="top">
      <IconButton
        aria-label={String(yamlToggleTitle)}
        className={
          activePanel === 'yaml'
            ? '!w-8 !h-8 !rounded-[6px] !bg-[var(--c-primary-light)] !text-[var(--c-primary)]'
            : '!w-8 !h-8 text-muted !rounded-[6px] hover:!bg-black-06 hover:!text-black'
        }
        onClick={() => setActivePanel((p) => (p === 'form' ? 'yaml' : 'form'))}
      >
        {activePanel === 'yaml' ? <SlidersHorizontal size={18} /> : <FileCode2 size={18} />}
      </IconButton>
    </ActionWithTips>
  );

  if (isEmbedded) {
    return (
      <div className="w-full flex flex-col gap-4">
        <div className="flex items-center justify-between gap-2">
          <div className="flex flex-wrap gap-1.5">
            {!isEdit && activePanel === 'form' ? (
              <>
                <CommandLineModal onSubmitBack={onCommandSubmitBack} />
                <CommandLineCopy form={form} />
              </>
            ) : null}
          </div>
          {yamlToggle}
        </div>
        {formContent}
        <div className="flex justify-end gap-2">
          <Button
            type="primary"
            loading={loading}
            disabled={hamiBlock && activePanel === 'form'}
            onClick={handleDeployClick}
            className="!bg-[var(--c-primary)] !border-[var(--c-primary)] hover:!bg-[var(--c-primary-hover)] hover:!border-[var(--c-primary-hover)]"
          >
            {l('global.actions.deploy')}
          </Button>
        </div>
      </div>
    );
  }

  return (
    <>
      {children ? <span onClick={handleOpen}>{children}</span> : null}
      <Drawer
        open={visible}
        onClose={handleCancel}
        width={720}
        destroyOnClose
        placement="right"
        title={
          <div className="flex flex-col gap-2 pr-2">
            <div className="flex items-center justify-between gap-2">
              <div className="text-lg">{`${isEdit ? l('global.update') : l('global.create')}${l(
                'menu.models.instances',
              )}`}</div>
              {yamlToggle}
            </div>
            {activePanel === 'form' ? (
              <div className="font-normal flex flex-wrap gap-1.5">
                <CommandLineModal onSubmitBack={onCommandSubmitBack} />
                <CommandLineCopy form={form} />
              </div>
            ) : null}
          </div>
        }
        footer={
          <div className="flex justify-end gap-2">
            <Button
              type="primary"
              loading={loading}
              disabled={hamiBlock && activePanel === 'form'}
              onClick={handleDeployClick}
              className="!bg-[var(--c-primary)] !border-[var(--c-primary)] hover:!bg-[var(--c-primary-hover)] hover:!border-[var(--c-primary-hover)]"
            >
              {l('global.actions.deploy')}
            </Button>
            <Button onClick={handleCancel}>{l('global.actions.cancel')}</Button>
          </div>
        }
      >
        {formContent}
      </Drawer>
    </>
  );
};
export default DeployModelInstance;
