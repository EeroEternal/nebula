import classNames from 'classnames';
import {
  Trash2,
  MoreHorizontal,
  Play,
  ArrowUp,
  Download,
  Copy,
  ArrowRightLeft,
} from 'lucide-react';
import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { ProCard, ProTable } from '@ant-design/pro-components';
import { App, Button, Checkbox, Dropdown, Modal, Select, Tooltip } from 'antd';
import type { MenuProps } from 'antd';
import { FC, useEffect, useMemo, useRef, useState } from 'react';
import { history, useParams } from '@umijs/max';
import { useRequest } from 'ahooks';

import { formPageParams } from '@/utils/fomatData';
import { l, lGet } from '@/utils/intl';
import { copyToClipboard } from '@/utils';
import { readRequestFailure, stringifyDetail } from '@/utils/formatApiError';
import { ModelData, ModelVersionTableListItem } from '@/types/Public/data';
import request from '@/utils/request';
import { swrGet, swrInvalidate, swrPeek } from '@/utils/swrCache';
import { StatusTag } from '@/components';
import {
  listModelDownloadNodes,
  transferModelDownload,
  type ModelDownloadNode,
} from '@/services/modelDownloads';
import DownloadDrawer, { DownloadDraft } from '../DownloadDrawer';
import TransferDrawer from '../../../components/TransferDrawer';

const VERSIONS_STALE_MS = 60_000;

interface ModelVersionsProps {
  modelData: ModelData;
  onDeleteCallBack: () => void;
  downloadHubs?: string[];
  /** 嵌在详情内容面内时去掉外层 ProCard，避免双层白卡 */
  embedded?: boolean;
}

type DeleteTarget = {
  modelVersion: string;
  nodes: string[];
};

type TransferTarget = {
  mode: 'copy' | 'migrate';
  record: ModelVersionTableListItem;
  source: string;
  path?: string;
};

const SUPERVISOR = 'supervisor';

/** Dropdown 菜单挂 body 时，关闭后 click 会穿透到表格行；用短窗屏蔽 onRow */
const ROW_CLICK_SUPPRESS_MS = 400;

const locationEntries = (loc?: Record<string, string> | null) =>
  loc && typeof loc === 'object' ? Object.entries(loc) : [];

const enginesOf = (record: ModelVersionTableListItem): string[] => {
  const raw = record.model_engine as string | string[] | undefined;
  if (Array.isArray(raw)) return raw.map(String).filter(Boolean);
  if (typeof raw === 'string' && raw.trim()) return [raw.trim()];
  return [];
};

const engineMatches = (record: ModelVersionTableListItem, filter: string) => {
  if (!filter || filter === 'all') return true;
  const want = filter.toLowerCase();
  return enginesOf(record).some((e) => e.toLowerCase() === want);
};

/**
 * 筛选值用规范化字符串，展示用 `${n}B`。
 * 模型仓常用 `0_8` 表示 0.8B；也兼容 `0.8B` / `0.8b`。
 */
const sizeKeyOf = (raw: unknown): string | null => {
  if (raw == null || raw === '') return null;
  if (typeof raw === 'number' && Number.isFinite(raw)) return String(raw);
  let s = String(raw).trim();
  if (!s) return null;
  if (/b$/i.test(s)) s = s.slice(0, -1).trim();
  const normalized = s.includes('_') ? s.replace(/_/g, '.') : s;
  const n = Number(normalized);
  if (Number.isFinite(n)) return String(n);
  return s;
};

/** 从 model_version（如 qwen3.5--0_8B--pytorch--none）解析参数量键 */
const sizeKeyFromVersion = (version?: string | null): string | null => {
  if (!version) return null;
  const parts = String(version).split('--');
  if (parts.length < 2) return null;
  return sizeKeyOf(parts[1]);
};

/** 展示用 version 串（如 qwen3.5--397B--…）为准；字段偶发与串不一致时避免筛错 */
const rowSizeKey = (row: ModelVersionTableListItem): string | null =>
  sizeKeyFromVersion(row.model_version) ?? sizeKeyOf(row.model_size_in_billions);

const sizeLabelOf = (key: string): string => {
  const n = Number(key);
  return Number.isFinite(n) ? `${n}B` : key;
};

const quantKeyOf = (raw: unknown): string | null => {
  if (raw == null) return null;
  const s = String(raw).trim();
  return s || null;
};

const sortEngines = (engines: string[]): string[] =>
  [...engines].sort((a, b) => {
    const aVllm = a.toLowerCase() === 'vllm' ? 0 : 1;
    const bVllm = b.toLowerCase() === 'vllm' ? 0 : 1;
    if (aVllm !== bVllm) return aVllm - bVllm;
    return a.localeCompare(b);
  });

const ModelVersions: FC<ModelVersionsProps> = ({
  modelData,
  onDeleteCallBack,
  downloadHubs,
  embedded = false,
}) => {
  const params = useParams();
  const { modelType, modelName } = params || {};
  const { message } = App.useApp();
  const actionRef = useRef<ActionType>();
  const suppressRowClickRef = useRef(false);
  const suppressRowClickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const suppressRowClick = () => {
    suppressRowClickRef.current = true;
    if (suppressRowClickTimerRef.current) {
      clearTimeout(suppressRowClickTimerRef.current);
    }
    suppressRowClickTimerRef.current = setTimeout(() => {
      suppressRowClickRef.current = false;
      suppressRowClickTimerRef.current = null;
    }, ROW_CLICK_SUPPRESS_MS);
  };
  useEffect(
    () => () => {
      if (suppressRowClickTimerRef.current) {
        clearTimeout(suppressRowClickTimerRef.current);
      }
    },
    [],
  );
  const [isCacheStatusSort, setCacheStatusSort] = useState(false);
  const [engineFilter, setEngineFilter] = useState<string>('all');
  const [sizeFilter, setSizeFilter] = useState<string>('all');
  const [quantFilter, setQuantFilter] = useState<string>('all');
  const [allRows, setAllRows] = useState<ModelVersionTableListItem[]>([]);
  const [downloadOpen, setDownloadOpen] = useState(false);
  const [downloadDraft, setDownloadDraft] = useState<DownloadDraft | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const [selectedNodes, setSelectedNodes] = useState<string[]>([]);
  const [deleting, setDeleting] = useState(false);
  const [transferTarget, setTransferTarget] = useState<TransferTarget | null>(null);
  const [transferring, setTransferring] = useState(false);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [page, setPage] = useState({ current: 1, pageSize: 20 });

  const versionsCacheKey = `model-versions|${modelType || ''}|${modelName || ''}|${
    isCacheStatusSort ? '1' : '0'
  }`;

  const { data: nodesRes } = useRequest(async () => {
    const res = await listModelDownloadNodes();
    if (!res?.success) return [] as ModelDownloadNode[];
    return (res.data?.nodes || []) as ModelDownloadNode[];
  }, { cacheKey: 'model-download-nodes', staleTime: 60_000 });
  const nodes = nodesRes || [];

  // 一次拉全量；筛选/翻页纯内存。缓存命中不盖 spin，避免挡「更多操作」。
  useEffect(() => {
    let cancelled = false;
    const fetcher = async () => {
      const res = await request(`/models/${modelType}/${modelName}/versions`, {
        params: {
          ...formPageParams({ current: 1, pageSize: 20 }),
          isCacheStatusSort,
          curPageNum: -1,
          numPerPage: -1,
        },
      });
      return (res.data?.results || []) as ModelVersionTableListItem[];
    };
    const peek = swrPeek<ModelVersionTableListItem[]>(versionsCacheKey);
    if (peek?.length) {
      setAllRows(peek);
      setVersionsLoading(false);
      void swrGet(versionsCacheKey, fetcher, {
        staleTime: VERSIONS_STALE_MS,
        onUpdate: (fresh) => {
          if (!cancelled) setAllRows(fresh);
        },
      });
      return () => {
        cancelled = true;
      };
    }
    setVersionsLoading(true);
    void swrGet(versionsCacheKey, fetcher, { staleTime: VERSIONS_STALE_MS }).then(
      ({ data }) => {
        if (!cancelled) {
          setAllRows(data || []);
          setVersionsLoading(false);
        }
      },
      () => {
        if (!cancelled) setVersionsLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [versionsCacheKey, modelType, modelName, isCacheStatusSort]);

  const engineOptions = useMemo(() => {
    // 大小写不敏感去重，保留首次出现的展示名
    const byLower = new Map<string, string>();
    for (const row of allRows) {
      for (const e of enginesOf(row)) {
        const lower = e.toLowerCase();
        if (!byLower.has(lower)) byLower.set(lower, e);
      }
    }
    return sortEngines(Array.from(byLower.values()));
  }, [allRows]);

  /** 选中引擎后，参数量/量化只列出该引擎下真实存在的版本 */
  const rowsForEngine = useMemo(() => {
    if (engineFilter === 'all') return allRows;
    return allRows.filter((r) => engineMatches(r, engineFilter));
  }, [allRows, engineFilter]);

  const sizeOptions = useMemo(() => {
    const set = new Set<string>();
    for (const row of rowsForEngine) {
      const key = rowSizeKey(row);
      if (key) set.add(key);
    }
    return Array.from(set).sort((a, b) => {
      const na = Number(a);
      const nb = Number(b);
      if (Number.isFinite(na) && Number.isFinite(nb)) return na - nb;
      return a.localeCompare(b);
    });
  }, [rowsForEngine]);

  const quantOptions = useMemo(() => {
    const set = new Set<string>();
    for (const row of rowsForEngine) {
      if (sizeFilter !== 'all' && rowSizeKey(row) !== sizeFilter) {
        continue;
      }
      const key = quantKeyOf(row.quantization);
      if (key) set.add(key);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
  }, [rowsForEngine, sizeFilter]);

  const filteredRows = useMemo(() => {
    return rowsForEngine.filter((r) => {
      if (sizeFilter !== 'all' && rowSizeKey(r) !== sizeFilter) return false;
      if (quantFilter !== 'all' && quantKeyOf(r.quantization) !== quantFilter) {
        return false;
      }
      return true;
    });
  }, [rowsForEngine, sizeFilter, quantFilter]);

  useEffect(() => {
    setPage((p) => ({ ...p, current: 1 }));
  }, [engineFilter, sizeFilter, quantFilter, isCacheStatusSort]);

  const pageSlice = useMemo(() => {
    const start = (page.current - 1) * page.pageSize;
    // model_version 在多引擎/多节点行上会重复；非唯一 rowKey 会导致 Ant Table 复用旧 DOM，筛选后仍显示脏行
    return filteredRows.slice(start, start + page.pageSize).map((r, i) => ({
      ...r,
      _rowKey: [
        r.model_version,
        enginesOf(r).join(','),
        r.model_format,
        r.quantization,
        String(start + i),
      ].join('|'),
    }));
  }, [filteredRows, page.current, page.pageSize]);

  useEffect(() => {
    if (engineFilter === 'all') return;
    const hit = engineOptions.some(
      (e) => e.toLowerCase() === engineFilter.toLowerCase(),
    );
    if (!hit) setEngineFilter('all');
  }, [engineFilter, engineOptions]);

  // 仅在选项已稳定且当前值确实不存在时重置（避免加载中短暂空 options 清掉用户选择）
  useEffect(() => {
    if (sizeFilter === 'all' || !allRows.length || !sizeOptions.length) return;
    if (!sizeOptions.includes(sizeFilter)) setSizeFilter('all');
  }, [sizeFilter, sizeOptions, allRows.length]);

  useEffect(() => {
    if (quantFilter === 'all' || !allRows.length || !quantOptions.length) return;
    if (!quantOptions.includes(quantFilter)) setQuantFilter('all');
  }, [quantFilter, quantOptions, allRows.length]);

  const handleCacheStatus = () => {
    setCacheStatusSort(!isCacheStatusSort);
    actionRef.current?.reload(true);
  };

  const handleCopy = (text: string) => {
    if (!text) {
      message.warning(String(lGet('models.repository.detail.versions.copyEmpty')));
      return;
    }
    copyToClipboard(text);
  };

  const openDeleteModal = (record: ModelVersionTableListItem) => {
    const nodeList = locationEntries(record.model_file_location).map(([node]) => node);
    if (!nodeList.length) {
      message.warning(
        String(lGet('models.repository.detail.versions.deleteNoNodes')),
      );
      return;
    }
    setDeleteTarget({ modelVersion: record.model_version, nodes: nodeList });
    setSelectedNodes(nodeList);
  };

  const openTransfer = (record: ModelVersionTableListItem, mode: 'copy' | 'migrate') => {
    const entries = locationEntries(record.model_file_location);
    if (!entries.length) {
      message.warning(
        String(lGet('models.repository.detail.versions.transferNoSource')),
      );
      return;
    }
    const [source, path] = entries[0];
    setTransferTarget({ mode, record, source, path });
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    if (!selectedNodes.length) {
      message.warning(
        String(lGet('models.repository.detail.versions.deleteNeedNode')),
      );
      return;
    }
    setDeleting(true);
    try {
      for (const workerIp of selectedNodes) {
        const q = new URLSearchParams({
          model_version: deleteTarget.modelVersion,
          worker_ip: workerIp,
        });
        await request(`/cache/models?${q.toString()}`, {
          method: 'delete',
        });
      }
      message.success(String(lGet('models.repository.detail.versions.deleteOk')));
      setDeleteTarget(null);
      setSelectedNodes([]);
      swrInvalidate(`model-versions|${modelType || ''}|${modelName || ''}`);
      actionRef.current?.reload();
      onDeleteCallBack();
    } catch (e: unknown) {
      message.error(
        readRequestFailure(e) ||
          String(lGet('models.repository.detail.versions.deleteFailed')),
      );
    } finally {
      setDeleting(false);
    }
  };

  const transferNodes = useMemo(() => {
    const cached = new Set(
      locationEntries(transferTarget?.record.model_file_location).map(([id]) => id),
    );
    const fromCache = locationEntries(transferTarget?.record.model_file_location).map(
      ([id, p]) => ({
        id,
        label: (
          <span>
            <span className="font-mono text-[12px]">
              {id === SUPERVISOR
                ? String(lGet('models.downloads.nodeSupervisor'))
                : id}
            </span>
            {p ? (
              <span className="block text-[11px] text-muted font-mono truncate">{p}</span>
            ) : null}
          </span>
        ),
        sourceDisabled: false,
      }),
    );
    const extras = nodes
      .filter((n) => n.node_id && !cached.has(n.node_id))
      .map((n) => ({
        id: n.node_id,
        label:
          n.node_id === SUPERVISOR
            ? String(lGet('models.downloads.nodeSupervisor'))
            : n.node_id,
        sourceDisabled: true,
      }));
    return [...fromCache, ...extras];
  }, [nodes, transferTarget]);

  const goDeploy = (record: ModelVersionTableListItem) => {
    const engines = enginesOf(record);
    const q = new URLSearchParams({
      activeTab: 'deploy',
      ...(record.model_version ? { model_version: record.model_version } : {}),
      ...(engines[0] ? { model_engine: engines[0] } : {}),
    });
    history.push(`/models/repository/${modelType}/${modelName}?${q.toString()}`);
  };

  const columns: ProColumns<ModelVersionTableListItem>[] = [
    {
      title: l('models.repository.detail.versions.version'),
      dataIndex: 'model_version',
      width: 240,
      render: (_, record) => {
        const meta = [record.model_format, record.model_size_in_billions, record.quantization]
          .filter((x) => x != null && x !== '')
          .join(' · ');
        const engines = enginesOf(record);
        return (
          <Tooltip title={record.model_version}>
            <div className="min-w-0">
              <div className="text-sm font-semibold text-default truncate">{record.model_version}</div>
              {meta ? (
                <div className="text-[11px] text-muted font-mono truncate mt-0.5">{meta}</div>
              ) : null}
              {engines.length ? (
                <div className="text-[11px] text-muted truncate mt-0.5">
                  {engines.join(', ')}
                </div>
              ) : null}
            </div>
          </Tooltip>
        );
      },
    },
    {
      title: l('models.repository.detail.versions.nodePath'),
      dataIndex: 'model_file_location',
      ellipsis: true,
      render: (_, record) => {
        const entries = locationEntries(record.model_file_location);
        if (!entries.length) {
          return <span className="text-muted text-[12px]">—</span>;
        }
        return (
          <div className="flex flex-col gap-1.5 min-w-0 w-full">
            {entries.map(([node, path]) => {
              const pathText = path || '';
              return (
                <div key={node} className="flex items-center gap-3 min-w-0 w-full">
                  <Tooltip title={node}>
                    <span className="shrink-0 w-[132px] max-w-[40%] font-mono text-[12px] text-default truncate">
                      {node}
                    </span>
                  </Tooltip>
                  {pathText ? (
                    <Tooltip
                      title={
                        <div className="max-w-[420px]">
                          <div className="font-mono text-[12px] break-all whitespace-pre-wrap">
                            {pathText}
                          </div>
                          <div className="mt-1 text-[11px] opacity-80">
                            {l('models.repository.detail.versions.copyHint')}
                          </div>
                        </div>
                      }
                    >
                      <button
                        type="button"
                        className="group min-w-0 flex-1 inline-flex items-center gap-1 bg-transparent border-0 p-0 cursor-pointer text-left"
                        onClick={(e) => {
                          e.stopPropagation();
                          suppressRowClick();
                          handleCopy(pathText);
                        }}
                      >
                        <span className="font-mono text-[11px] text-muted truncate min-w-0 flex-1 group-hover:text-primary">
                          {pathText}
                        </span>
                        <Copy size={12} className="shrink-0 text-muted group-hover:text-primary" />
                      </button>
                    </Tooltip>
                  ) : (
                    <span className="text-muted text-[11px]">—</span>
                  )}
                </div>
              );
            })}
          </div>
        );
      },
    },
    {
      title: (
        <div className="flex items-center gap-x-[8px] cursor-pointer" onClick={handleCacheStatus}>
          {l('models.repository.detail.versions.cacheStatus')}
          <ArrowUp
            size={16}
            className={classNames('duration-300', { 'rotate-180': isCacheStatusSort })}
          />
        </div>
      ),
      dataIndex: 'cache_status',
      width: 120,
      render: (_, record) => (
        <StatusTag tone={record.cache_status ? 'success' : 'neutral'}>
          {record.cache_status
            ? l('models.repository.cached')
            : l('models.repository.notCached')}
        </StatusTag>
      ),
    },
    {
      title: l('models.repository.detail.versions.actions'),
      dataIndex: 'option',
      valueType: 'option',
      fixed: 'right',
      width: 72,
      render: (_, record) => {
        const items: MenuProps['items'] = [
          {
            key: 'download',
            icon: <Download size={14} />,
            label: l('models.repository.detail.versions.actions.download'),
            onClick: () => {
              setDownloadDraft({
                modelName: modelName || modelData.model_name,
                modelType: modelType as string,
                modelData,
                downloadHubs,
                version: {
                  key: record.model_version,
                  label: record.model_version,
                  model_format: record.model_format,
                  model_size_in_billions: record.model_size_in_billions,
                  quantization: record.quantization,
                  model_version: record.model_version,
                },
              });
              setDownloadOpen(true);
            },
          },
          {
            key: 'deploy',
            icon: <Play size={14} />,
            label: l('models.repository.detail.versions.actions.deploy'),
            onClick: () => goDeploy(record),
          },
        ];
        if (record.cache_status) {
          items.push(
            {
              key: 'copy',
              icon: <Copy size={14} />,
              label: l('models.repository.detail.versions.actions.copy'),
              onClick: () => openTransfer(record, 'copy'),
            },
            {
              key: 'migrate',
              icon: <ArrowRightLeft size={14} />,
              label: l('models.repository.detail.versions.actions.migrate'),
              onClick: () => openTransfer(record, 'migrate'),
            },
            {
              key: 'delete',
              danger: true,
              icon: <Trash2 size={14} />,
              label: l('models.repository.detail.versions.actions.delete'),
              onClick: () => openDeleteModal(record),
            },
          );
        }
        return (
          <Dropdown
            menu={{
              items,
              onClick: () => {
                // 菜单项点击后 dropdown 关闭，原生 click 会穿透到行 → 误触发部署
                suppressRowClick();
              },
            }}
            trigger={['click']}
            placement="bottomRight"
            // 详情页 section 有 overflow-hidden；挂到 body 避免菜单被裁切后点不到
            getPopupContainer={() => document.body}
            onOpenChange={(open) => {
              if (open) suppressRowClick();
            }}
          >
            <Button
              type="text"
              size="small"
              className="!px-1.5"
              aria-label={l('models.repository.moreActions')}
              icon={<MoreHorizontal size={16} />}
              onClick={(e) => {
                e.stopPropagation();
                suppressRowClick();
              }}
            />
          </Dropdown>
        );
      },
    },
  ];

  const table = (
    <>
      <DownloadDrawer
        open={downloadOpen}
        draft={downloadDraft}
        onClose={() => {
          setDownloadOpen(false);
          setDownloadDraft(null);
        }}
      />
      <Modal
        title={l('models.repository.detail.versions.actions.delete')}
        open={!!deleteTarget}
        onCancel={() => {
          if (deleting) return;
          setDeleteTarget(null);
          setSelectedNodes([]);
        }}
        onOk={handleDeleteConfirm}
        confirmLoading={deleting}
        okButtonProps={{ danger: true, disabled: !selectedNodes.length }}
        okText={l('models.repository.detail.versions.deleteConfirmOk')}
        destroyOnClose
      >
        <p className="text-sm text-default mb-3">
          {l('models.repository.detail.versions.actions.delete.tips',
          )}
        </p>
        {deleteTarget ? (
          <div className="mb-3 text-[12px] text-muted font-mono break-all">
            {deleteTarget.modelVersion}
          </div>
        ) : null}
        <div className="flex items-center justify-between mb-2">
          <span className="text-sm text-default">
            {l('models.repository.detail.versions.deleteSelectNodes')}
          </span>
          {deleteTarget && deleteTarget.nodes.length > 1 ? (
            <Button
              type="link"
              size="small"
              className="!px-0"
              onClick={() => {
                const all = deleteTarget.nodes;
                setSelectedNodes(selectedNodes.length === all.length ? [] : all);
              }}
            >
              {selectedNodes.length === deleteTarget.nodes.length
                ? l('models.repository.detail.versions.unselectAll')
                : l('models.repository.detail.versions.selectAll')}
            </Button>
          ) : null}
        </div>
        <Checkbox.Group
          className="flex flex-col gap-2 !w-full"
          value={selectedNodes}
          onChange={(vals) => setSelectedNodes(vals as string[])}
          options={(deleteTarget?.nodes || []).map((node) => ({
            label: <span className="font-mono text-[12px]">{node}</span>,
            value: node,
          }))}
        />
      </Modal>
      <TransferDrawer
        open={!!transferTarget}
        title={
          transferTarget?.mode === 'migrate'
            ? l('models.repository.detail.versions.migrateTitle')
            : l('models.repository.detail.versions.copyTitle')
        }
        initialSource={transferTarget?.source}
        initialMode={transferTarget?.mode || 'migrate'}
        submitting={transferring}
        nodes={transferNodes}
        subject={
          transferTarget ? (
            <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card shadow-card p-5">
              <div className="font-semibold text-default">
                {transferTarget.record.model_version}
              </div>
            </div>
          ) : null
        }
        modeHint={{
          migrate: l('models.repository.detail.versions.migrateWarn',
          ),
          copy: l('models.repository.detail.versions.copyWarn',
          ),
        }}
        onClose={() => setTransferTarget(null)}
        onSubmit={async ({ source, dests, mode }) => {
          if (!transferTarget) return;
          setTransferring(true);
          try {
            const { record } = transferTarget;
            const path = locationEntries(record.model_file_location).find(
              ([id]) => id === source,
            )?.[1];
            for (const dest of dests) {
              const res = await transferModelDownload({
                model_type: (modelType as string) || 'LLM',
                model_name: modelName || modelData.model_name,
                source,
                dest,
                src_path: path || undefined,
                model_version: record.model_version,
                model_format: record.model_format,
                model_size_in_billions: record.model_size_in_billions,
                quantization: record.quantization,
                wait: false,
                mode,
              });
              if (!res?.success && res?.data?.detail) {
                throw new Error(stringifyDetail(res.data.detail));
              }
            }
            message.success(
              String(
                mode === 'migrate'
                  ? lGet('models.repository.detail.versions.migrateStarted',
                    )
                  : lGet('models.repository.detail.versions.copyStarted'),
              ),
            );
            setTransferTarget(null);
            swrInvalidate(`model-versions|${modelType || ''}|${modelName || ''}`);
            actionRef.current?.reload();
            onDeleteCallBack();
          } catch (e: unknown) {
            message.error(
              readRequestFailure(e) ||
                String(lGet('models.repository.detail.versions.transferFailed')),
            );
          } finally {
            setTransferring(false);
          }
        }}
      />
      <div className="flex flex-wrap items-center gap-2 mb-3 px-1">
        <Select
          className="w-[108px]"
          popupMatchSelectWidth={false}
          getPopupContainer={() => document.body}
          value={engineFilter}
          options={[
            {
              value: 'all',
              label: l('models.repository.detail.versions.engineAll'),
            },
            ...engineOptions.map((e) => ({ value: e, label: e })),
          ]}
          onChange={(v) => {
            setEngineFilter(v);
            // 重选引擎：参数量/量化一律重置；params 变化会触发 ProTable 重请求，勿再 reload 盖 loading
            setSizeFilter('all');
            setQuantFilter('all');
          }}
        />
        <Select
          className="w-[120px]"
          popupMatchSelectWidth={false}
          getPopupContainer={() => document.body}
          value={sizeFilter}
          options={[
            {
              value: 'all',
              label: l('models.repository.detail.versions.sizeAll'),
            },
            ...sizeOptions.map((s) => ({ value: s, label: sizeLabelOf(s) })),
          ]}
          onChange={(v) => {
            setSizeFilter(v);
            setQuantFilter('all');
          }}
        />
        <Select
          className="w-[108px]"
          popupMatchSelectWidth={false}
          getPopupContainer={() => document.body}
          value={quantFilter}
          options={[
            {
              value: 'all',
              label: l('models.repository.detail.versions.quantAll'),
            },
            ...quantOptions.map((q) => ({ value: q, label: q })),
          ]}
          onChange={(v) => {
            setQuantFilter(v);
          }}
        />
      </div>
      <div
        className={
          embedded
            ? undefined
            : 'rounded-lg bg-card border border-[color:var(--c-border-light)] shadow-card overflow-hidden'
        }
      >
        <ProTable
          toolBarRender={false}
          rowKey="_rowKey"
          actionRef={actionRef}
          columns={columns}
          search={false}
          scroll={{ x: 900 }}
          size="middle"
          cardProps={{ bodyStyle: { padding: 0 }, bordered: false }}
          loading={versionsLoading && !allRows.length}
          dataSource={pageSlice}
          pagination={{
            current: page.current,
            pageSize: page.pageSize,
            total: filteredRows.length,
            showSizeChanger: true,
            onChange: (current, pageSize) =>
              setPage({ current, pageSize: pageSize || 20 }),
          }}
          onRow={(record) => ({
            onClick: () => {
              if (suppressRowClickRef.current) return;
              goDeploy(record);
            },
            className: 'cursor-pointer',
          })}
        />
      </div>
    </>
  );

  if (embedded) return <div className="-mx-1">{table}</div>;
  return <ProCard bodyStyle={{ padding: 0 }}>{table}</ProCard>;
};

export default ModelVersions;
