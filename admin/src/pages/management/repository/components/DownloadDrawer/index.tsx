import { App, Checkbox, Drawer, Select, Space, Spin } from 'antd';
import { Download } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { history } from '@umijs/max';
import { useRequest } from 'ahooks';

import {
  listModelDownloadNodes,
  ModelDownloadNode,
  pullModelDownload,
} from '@/services/modelDownloads';
import { ModelData, ModelRepositoryListItem } from '@/types/Public/data';
import { readRequestFailure, stringifyDetail } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';

const SUPERVISOR = 'supervisor';

const HUB_OPTIONS = [
  { value: 'modelscope', label: 'ModelScope' },
  { value: 'huggingface', label: 'Hugging Face' },
  { value: 'csghub', label: 'CSG Hub' },
];

export type DownloadVersionOption = {
  key: string;
  label: string;
  model_format?: string;
  model_size_in_billions?: number | string;
  quantization?: string;
  model_version?: string;
  model_hub?: string;
};

export type DownloadDraft = {
  modelName: string;
  modelType?: string;
  versionHint?: string;
  /** Pre-selected version (e.g. from detail table). */
  version?: DownloadVersionOption;
  modelData?: ModelData | ModelRepositoryListItem;
  downloadHubs?: string[];
};

type Props = {
  open: boolean;
  draft: DownloadDraft | null;
  onClose: () => void;
};

function parseSize(size: unknown): number | null {
  if (size == null || size === '') return null;
  const n = Number(size);
  return Number.isFinite(n) ? n : null;
}

type SpecRow = {
  model_format?: string;
  model_size_in_billions?: number | string;
  quantizations?: string[];
  quantization?: string;
  model_version?: string;
  model_hub?: string;
};

type VersionRow = {
  model_version?: string;
  model_format?: string;
  model_size_in_billions?: number | string;
  quantization?: string;
  model_hub?: string;
};

function buildVersionOptionsFromSpecs(specs: SpecRow[] | undefined | null): DownloadVersionOption[] {
  if (!specs?.length) return [];
  const options: DownloadVersionOption[] = [];
  specs.forEach((spec, idx) => {
    const format = spec.model_format;
    const size = spec.model_size_in_billions;
    const quants: string[] =
      Array.isArray(spec.quantizations) && spec.quantizations.length
        ? spec.quantizations
        : spec.quantization
          ? [spec.quantization]
          : ['none'];
    quants.forEach((q) => {
      const label =
        spec.model_version ||
        [format, size != null ? `${size}B` : null, q].filter(Boolean).join('-') ||
        `spec-${idx}`;
      options.push({
        key: `${format || ''}|${size ?? ''}|${q}|${spec.model_hub || ''}`,
        label,
        model_format: format,
        model_size_in_billions: size,
        quantization: q === 'none' ? undefined : q,
        model_version: spec.model_version,
        model_hub: spec.model_hub,
      });
    });
  });
  const seen = new Set<string>();
  return options.filter((o) => {
    if (seen.has(o.key)) return false;
    seen.add(o.key);
    return true;
  });
}

function buildVersionOptionsFromVersions(versions: VersionRow[] | undefined | null): DownloadVersionOption[] {
  if (!versions?.length) return [];
  return versions.map((v) => ({
    key: v.model_version || `${v.model_format}|${v.model_size_in_billions}|${v.quantization}`,
    label: v.model_version || `${v.model_format}-${v.model_size_in_billions}B-${v.quantization}`,
    model_format: v.model_format,
    model_size_in_billions: v.model_size_in_billions,
    quantization: v.quantization,
    model_version: v.model_version,
  }));
}

/** 下载权重：选版本规格 + Hub + 目标节点，创建独立下载任务。 */
const DownloadDrawer: React.FC<Props> = ({ open, draft, onClose }) => {
  const { message } = App.useApp();
  const [hub, setHub] = useState('modelscope');
  const [targets, setTargets] = useState<string[]>([SUPERVISOR]);
  const [versionKey, setVersionKey] = useState<string>('');
  const [submitting, setSubmitting] = useState(false);

  const needsFetch =
    open &&
    !!draft?.modelName &&
    !draft.version &&
    !(draft.modelData?.model_specs && draft.modelData.model_specs.length);

  /** 列表无 specs：一次拉取 versions（优先）或 detail，避免双 useRequest loading 卡住 */
  const { data: fetchedMeta, loading: metaLoading } = useRequest(
    async () => {
      const modelType = draft?.modelType || 'LLM';
      const name = draft!.modelName;

      const vRes = await request(`/models/${modelType}/${name}/versions`);
      if (vRes?.success) {
        const results = (vRes.data?.results || []) as VersionRow[];
        const options = buildVersionOptionsFromVersions(results);
        if (options.length) {
          return { options, downloadHubs: undefined as string[] | undefined };
        }
      }

      const dRes = await request(`/model_registrations/${modelType}/${name}`);
      if (!dRes?.success) {
        throw new Error(stringifyDetail(dRes?.data?.detail) || 'load model detail failed');
      }
      // 兼容 {code,data:{model_data}} 与拦截器再包一层
      const body = dRes.data || {};
      const detail = body.data?.model_data ? body.data : body.model_data ? body : body.data || body;
      const specs = detail?.model_data?.model_specs || detail?.model_specs || [];
      return {
        options: buildVersionOptionsFromSpecs(specs),
        downloadHubs: (detail?.download_hubs || body?.data?.download_hubs || body?.download_hubs) as
          | string[]
          | undefined,
      };
    },
    {
      ready: needsFetch,
      refreshDeps: [open, draft?.modelName, draft?.modelType],
    },
  );

  const versionOptions = useMemo(() => {
    if (!draft) return [];
    if (draft.version) return [draft.version];
    const fromDraft = buildVersionOptionsFromSpecs(draft.modelData?.model_specs);
    if (fromDraft.length) return fromDraft;
    return fetchedMeta?.options || [];
  }, [draft, fetchedMeta]);

  const hubOptions = useMemo(() => {
    const hubs = draft?.downloadHubs || fetchedMeta?.downloadHubs;
    if (hubs?.length) {
      return hubs.map((h) => ({
        value: h,
        label: HUB_OPTIONS.find((o) => o.value === h)?.label || h,
      }));
    }
    return HUB_OPTIONS;
  }, [draft?.downloadHubs, fetchedMeta?.downloadHubs]);

  const { data: nodesRes, loading: nodesLoading } = useRequest(
    async () => {
      const res = await listModelDownloadNodes();
      if (!res?.success) {
        throw new Error(stringifyDetail(res?.data?.detail) || 'load nodes failed');
      }
      return (res.data?.nodes || []) as ModelDownloadNode[];
    },
    { ready: open, refreshDeps: [open] },
  );

  const nodes = useMemo(() => {
    const list = nodesRes || [];
    if (!list.length) {
      return [{ node_id: SUPERVISOR, role: 'supervisor', label: 'supervisor' }];
    }
    return list;
  }, [nodesRes]);

  useEffect(() => {
    if (!open) return;
    const defaultHub =
      versionOptions.find((v) => v.key === versionKey)?.model_hub ||
      hubOptions[0]?.value ||
      'modelscope';
    setHub(defaultHub);
    setTargets([SUPERVISOR]);
    // Prefer a small spec (<=2B) when available for safer default.
    const small = versionOptions.find((v) => {
      const sz = parseSize(v.model_size_in_billions);
      return sz != null && sz > 0 && sz <= 2;
    });
    setVersionKey(small?.key || versionOptions[0]?.key || '');
  }, [open, draft?.modelName, versionOptions, hubOptions]);

  const selectedVersion = versionOptions.find((v) => v.key === versionKey);
  const optionsLoading = needsFetch && metaLoading;

  const handleStart = async () => {
    if (!draft) return;
    if (!selectedVersion) {
      message.warning(lGet('models.repository.download.needVersion'));
      return;
    }
    const dests = targets.filter(Boolean);
    if (!dests.length) {
      message.warning(lGet('models.repository.download.needTarget'));
      return;
    }
    setSubmitting(true);
    try {
      for (const target of dests) {
        const res = await pullModelDownload({
          model_type: draft.modelType || 'LLM',
          model_name: draft.modelName,
          model_format: selectedVersion.model_format,
          model_size_in_billions: selectedVersion.model_size_in_billions,
          quantization: selectedVersion.quantization,
          model_version: selectedVersion.model_version || selectedVersion.label,
          download_hub: selectedVersion.model_hub || hub,
          target,
          wait: false,
        });
        if (!res?.success && res?.data?.detail) {
          throw new Error(stringifyDetail(res.data.detail) || 'pull failed');
        }
      }
      message.success(
        dests.length === 1
          ? lGet('models.repository.download.queuedToast', undefined, {
              model: draft.modelName,
              target: dests[0],
            }) || `已创建下载任务：${draft.modelName} → ${dests[0]}`
          : lGet('models.repository.download.queuedToastMulti', undefined, {
              model: draft.modelName,
              count: dests.length,
            }) || `已创建 ${dests.length} 条下载任务：${draft.modelName}`,
      );
      onClose();
      history.push('/models/repository/downloads');
    } catch (e: unknown) {
      message.error(readRequestFailure(e) || l('models.repository.download.failed'));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Drawer
      title={l('models.repository.download.title')}
      open={open}
      onClose={onClose}
      width={420}
      destroyOnClose
      footer={
        <div className="flex justify-end gap-2">
          <button
            type="button"
            disabled={submitting || optionsLoading || !selectedVersion}
            className="h-9 px-4 rounded-md bg-[var(--c-navy)] text-white text-sm font-semibold cursor-pointer hover:bg-[var(--c-navy-hover)] inline-flex items-center gap-1.5 disabled:opacity-60"
            onClick={handleStart}
          >
            <Download size={14} />
            {l('models.repository.download.start')}
          </button>
          <button
            type="button"
            className="h-9 px-4 rounded-md border border-[color:var(--c-border)] bg-[var(--c-surface)] text-sm font-semibold cursor-pointer hover:bg-[var(--c-surface-2)]"
            onClick={onClose}
          >
            {l('global.actions.cancel')}
          </button>
        </div>
      }
    >
      <div className="rounded-lg bg-[var(--c-surface-2)] p-3 mb-4">
        <div className="text-xs text-muted">{l('models.repository.download.model')}</div>
        <div className="text-sm font-semibold text-default mt-0.5">{draft?.modelName || '—'}</div>
      </div>

      <div className="mb-4">
        <div className="text-xs font-semibold text-secondary mb-1.5">
          {l('models.repository.download.version')}
        </div>
        {optionsLoading ? (
          <div className="py-4 flex justify-center">
            <Spin size="small" />
          </div>
        ) : versionOptions.length > 0 ? (
          <Select
            className="w-full"
            value={versionKey || undefined}
            options={versionOptions.map((v) => ({ value: v.key, label: v.label }))}
            onChange={(key) => {
              setVersionKey(key);
              const opt = versionOptions.find((v) => v.key === key);
              if (opt?.model_hub) setHub(opt.model_hub);
            }}
            placeholder={l('models.repository.download.versionPlaceholder')}
            showSearch
            optionFilterProp="label"
          />
        ) : (
          <div className="text-[13px] text-danger">
            {l('models.repository.download.noVersion')}
          </div>
        )}
      </div>

      <div className="mb-4">
        <div className="text-xs font-semibold text-secondary mb-1.5">
          {l('models.repository.download.hub')}
        </div>
        <Select className="w-full" value={hub} options={hubOptions} onChange={setHub} />
      </div>

      <div className="mb-2">
        <div className="text-xs font-semibold text-secondary mb-1.5">
          {l('models.repository.download.target')}
        </div>
        <Checkbox.Group
          className="w-full"
          value={targets}
          onChange={(vals) => setTargets(vals.map(String))}
          disabled={nodesLoading}
        >
          <Space direction="vertical" className="w-full">
            {nodes.map((n) => {
              const id = n.node_id || SUPERVISOR;
              const label =
                id === SUPERVISOR
                  ? l('models.repository.download.supervisor')
                  : n.label || n.ip_address || id;
              return (
                <label
                  key={id}
                  className={`flex items-start gap-2.5 p-3 rounded-md border cursor-pointer transition-colors ${
                    targets.includes(id)
                      ? 'border-primary bg-[var(--c-primary-light)]'
                      : 'border-[color:var(--c-border)] hover:border-primary'
                  }`}
                >
                  <Checkbox value={id} className="mt-0.5" />
                  <span className="min-w-0">
                    <span className="block text-sm font-semibold text-default">{label}</span>
                    <span className="block text-[11px] text-muted font-mono mt-0.5">{id}</span>
                  </span>
                </label>
              );
            })}
          </Space>
        </Checkbox.Group>
        <p className="text-xs text-muted mt-2 mb-0">
          {l('models.repository.download.hint',
          )}
        </p>
      </div>
    </Drawer>
  );
};

export default DownloadDrawer;
