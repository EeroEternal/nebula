import { useRequest } from 'ahooks';
import {
  Alert,
  App,
  Button,
  Dropdown,
  Input,
  Modal,
  Progress,
  Select,
  Table,
  Tag,
  Tooltip,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { MenuProps } from 'antd';
import classNames from 'classnames';
import {
  ArrowRightLeft,
  ArrowUp,
  CloudDownload,
  Container,
  Download,
  Loader2,
  MoreVertical,
  Plus,
  PackagePlus,
  RefreshCw,
  RotateCcw,
  Trash2,
} from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { history, useParams } from '@umijs/max';

import {
  EmptyState,
  FilterBar,
  IconButton,
  PageContainer,
  PillTabs,
  SectionLoading,
} from '@/components';
import {
  imageMatches,
  preferCanonicalLocalImage,
} from '@/components/DeployModelInstance/engineVersionOptions';
import {
  EngineImageSpec,
  EngineNodeImages,
  EngineNodeInfo,
  EnginePullProgress,
  EngineRegistryConfig,
  EngineRuntimeKind,
  getEnginePullProgress,
  getEngineRegistry,
  getRemoteSyncProgress,
  listEngineImages,
  listLocalEngineImages,
  listRemoteEngineTags,
  LocalEngineImagesResult,
  RemoteEngineTag,
  RemoteSyncProgress,
  resetEngineRegistry,
  syncRemoteEngineTags,
} from '@/services/engineImages';
import {
  humanizeEngineImageError,
  stringifyDetail,
} from '@/utils/formatApiError';
import { formatDisplayTime } from '@/utils';
import { l, lGet } from '@/utils/intl';

import ActionDrawer, {
  ActionDraft,
} from './components/ActionDrawer';
import ManualRegisterModal from './components/ManualRegisterModal';
import RegistryBar from './components/RegistryBar';
import StatusTag from './components/StatusTag';
import TaskPanel from './components/TaskPanel';
import { useEngineTasks } from './hooks/useEngineTasks';
import {
  anyNodeHasImage,
  catalogRowKey,
  compareEngines,
  coverageForImage,
  engineTagLabel,
  engineTagTone,
  inferEngineForLocalImage,
  localImageVersion,
  SUPERVISOR_NODE_ID,
  updatedAtMs,
} from './utils';
import { asServerPageRows } from '@/utils/tablePagination';

const LOCAL_FILTER_ALL = 'all';
const LOCAL_FILTER_READY = 'ready';
const LOCAL_FILTER_MISSING = 'missing';
const ENGINE_FILTER_ALL = 'all';

const TAB_IMAGES = 'images';
const TAB_REGISTRY = 'registry';
const TAB_TASKS = 'tasks';
const ENGINE_TABS = [TAB_IMAGES, TAB_REGISTRY, TAB_TASKS] as const;

const panelShell =
  'rounded-lg bg-card border border-[color:var(--c-border-light)] shadow-card overflow-hidden';

type VersionRow = EngineImageSpec & {
  recommended: boolean;
  key: string;
  last_updated?: string;
  full_size?: number | null;
  /** 服务端 remote-tags.local_present；远程分页时用于置顶与状态 */
  local_present?: boolean;
};

/** Hub full_size（字节）→ 固定 GB 展示 */
const formatSizeGB = (n?: number | null): string => {
  const v = Number(n);
  if (!Number.isFinite(v) || v <= 0) return '—';
  const gb = v / (1024 * 1024 * 1024);
  if (gb < 0.01) return '<0.01 GB';
  return `${gb < 10 ? gb.toFixed(2) : gb.toFixed(1)} GB`;
};

const EnginesPage: React.FC = () => {
  const { message } = App.useApp();
  const params = useParams<{ tab?: string }>();
  const tab = ENGINE_TABS.includes(params.tab as (typeof ENGINE_TABS)[number])
    ? (params.tab as string)
    : TAB_IMAGES;

  useEffect(() => {
    if (!params.tab || !ENGINE_TABS.includes(params.tab as (typeof ENGINE_TABS)[number])) {
      history.replace(`/models/engines/${TAB_IMAGES}`);
    }
  }, [params.tab]);

  /** 未解析前不传 runtime，由后端按启动时 POWERLLM_ENGINE_RUNTIME 决定 */
  const [runtime, setRuntime] = useState<EngineRuntimeKind | undefined>(
    undefined,
  );
  /**
   * 默认「全部」：远程列表由服务端按 local_present 置顶已就绪。
   * 勿默认 ready，否则与进页探测改 all 并发，易出现 total=3 却渲染错页。
   */
  const [localFilter, setLocalFilter] = useState(LOCAL_FILTER_ALL);
  /** false=更新时间新→旧；true=旧→新（与模型仓库「缓存状态」列交互一致） */
  const [isUpdatedAtSort, setUpdatedAtSort] = useState(false);
  const [engineFilter, setEngineFilter] = useState(ENGINE_FILTER_ALL);
  const [versionQuery, setVersionQuery] = useState('');
  const [keywordTags, setKeywordTags] = useState<string[]>([]);
  const [registryTick, setRegistryTick] = useState(0);
  const [syncingRemote, setSyncingRemote] = useState(false);
  const [manualOpen, setManualOpen] = useState(false);
  const [manualProgress, setManualProgress] =
    useState<EnginePullProgress | null>(null);
  const manualPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [registryBarTick, setRegistryBarTick] = useState(0);
  const [resettingRegistry, setResettingRegistry] = useState(false);
  /**
   * 远程 tag 对比：服务端已有 sync 缓存时进页自动启用；
   * 无缓存时仍只显示本机，需手动「更新仓库」。
   */
  const [includeRemoteCompare, setIncludeRemoteCompare] = useState(false);
  /** 远程 tag 服务端分页（与表格页对齐，避免一次拉全量） */
  const [remotePage, setRemotePage] = useState(1);
  const [remotePageSize, setRemotePageSize] = useState(10);
  const [remoteTotal, setRemoteTotal] = useState(0);

  const [actionOpen, setActionOpen] = useState(false);
  const [actionDraft, setActionDraft] = useState<ActionDraft | null>(null);
  const [syncProgress, setSyncProgress] = useState<RemoteSyncProgress | null>(
    null,
  );
  /** 与同步状态分离：关闭弹窗不中断后台轮询 */
  const [showSyncModal, setShowSyncModal] = useState(false);
  const forceLocalRef = useRef(false);
  const syncPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // 服务端 remote-tags 库表：新浏览器/刷新后仍可恢复「更新仓库」结果
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await listRemoteEngineTags({
          page: 1,
          page_size: 1,
        });
        if (cancelled || !res?.success) return;
        const data = res.data || {};
        const hasCache =
          Boolean(data.synced) ||
          data.updated_at != null ||
          (typeof data.count === 'number' && data.count > 0) ||
          (Array.isArray(data.tags) && data.tags.length > 0);
        if (hasCache) {
          setIncludeRemoteCompare(true);
        }
      } catch {
        /* 无缓存或接口失败时保持本机优先 */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const {
    data: registryCfg,
    refresh: refreshRegistry,
    mutate: mutateRegistry,
  } = useRequest(
    async () => {
      const res = await getEngineRegistry();
      if (!res?.success) return null;
      return res.data as EngineRegistryConfig;
    },
    { cacheKey: 'engine-images-registry', staleTime: 60_000 },
  );

  const {
    data: catalog,
    loading: catalogLoading,
    error: catalogError,
    refresh: refreshCatalog,
  } = useRequest(
    async () => {
      const res = await listEngineImages();
      if (!res?.success) {
        throw new Error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.loadFailed')),
        );
      }
      const list = res.data?.list ?? res.data?.data?.list ?? res.data;
      return (Array.isArray(list) ? list : []) as EngineImageSpec[];
    },
    { cacheKey: 'engine-images-catalog', staleTime: 60_000 },
  );

  const catalogList = Array.isArray(catalog) ? catalog : [];

  const parseLocalPayload = (res: {
    success?: boolean;
    data?: LocalEngineImagesResult & { detail?: unknown };
  }): LocalEngineImagesResult => {
    if (!res?.success) {
      throw new Error(
        stringifyDetail(res?.data?.detail) ||
          String(lGet('models.engines.localListFailed')),
      );
    }
    return {
      runtime: res.data?.runtime,
      nodes: (res.data?.nodes || []) as EngineNodeImages[],
      images: (res.data?.images || []) as string[],
      partial: Boolean(res.data?.partial),
    };
  };

  /** 首屏：仅扫本机 supervisor，尽快展示已有镜像 */
  const {
    data: quickLocal,
    loading: quickLocalLoading,
    error: quickLocalError,
    refresh: refreshQuickLocal,
  } = useRequest(
    async () => {
      const res = await listLocalEngineImages({
        ...(runtime ? { runtime } : {}),
        quick: true,
      });
      return parseLocalPayload(res);
    },
    {
      refreshDeps: [runtime],
      cacheKey: `engine-images-local-quick-${runtime ?? 'env'}`,
      staleTime: 20_000,
    },
  );

  /** 递补：全量节点（worker 并行）覆盖状态 */
  const {
    data: fullLocal,
    loading: fullLocalLoading,
    error: fullLocalError,
    refresh: refreshFullLocal,
  } = useRequest(
    async () => {
      const force = forceLocalRef.current;
      forceLocalRef.current = false;
      const res = await listLocalEngineImages({
        ...(runtime ? { runtime } : {}),
        force,
      });
      return parseLocalPayload(res);
    },
    {
      refreshDeps: [runtime],
      cacheKey: `engine-images-local-${runtime ?? 'env'}`,
      staleTime: 20_000,
    },
  );

  const localData = fullLocal ?? quickLocal;
  const localRequestError = fullLocalError || (!fullLocal && quickLocalError);
  /** 仅在全量节点结果返回后切到 worker 覆盖语义；失败时保持本机/检测中态 */
  const localScanComplete = Boolean(fullLocal);
  const localFirstLoading = !localData && (quickLocalLoading || fullLocalLoading);

  useEffect(() => {
    const r = localData?.runtime;
    if (r === 'docker' || r === 'k8s') setRuntime(r);
  }, [localData?.runtime]);

  const enabledEngines = useMemo(() => {
    const set = new Set<string>();
    for (const key of Object.keys(registryCfg?.repos || {})) {
      if (key) set.add(key.toLowerCase());
    }
    return set;
  }, [registryCfg]);

  const engineOptions = useMemo(
    () => Array.from(enabledEngines).sort(compareEngines),
    [enabledEngines],
  );

  useEffect(() => {
    if (
      engineFilter !== ENGINE_FILTER_ALL &&
      enabledEngines.size > 0 &&
      !enabledEngines.has(engineFilter)
    ) {
      setEngineFilter(ENGINE_FILTER_ALL);
    }
  }, [engineFilter, enabledEngines]);

  // 远程 API 的 q：多关键词空格 AND（在前序标签上叠加）。
  // 丢弃被更长标签覆盖的子串（minimax + mini → 只传 minimax）。
  const remoteSearchQ = useMemo(() => {
    const raw = [
      ...keywordTags.map((t) => t.trim()).filter(Boolean),
      versionQuery.trim(),
    ].filter(Boolean);
    const parts = raw.filter((term, i) => {
      const tl = term.toLowerCase();
      return !raw.some(
        (other, j) =>
          j !== i &&
          other.toLowerCase() !== tl &&
          other.toLowerCase().includes(tl),
      );
    });
    return parts.join(' ');
  }, [keywordTags, versionQuery]);

  const hasActiveImageFilter =
    Boolean(remoteSearchQ) ||
    keywordTags.length > 0 ||
    Boolean(versionQuery.trim()) ||
    localFilter !== LOCAL_FILTER_ALL ||
    engineFilter !== ENGINE_FILTER_ALL;

  /** 本机扫描落库后需重拉 remote-tags（local_present / __local__ 行） */
  const localInventoryKey = useMemo(() => {
    const imgs = (localData?.images || []) as string[];
    return imgs.slice().sort().join('\n');
  }, [localData?.images]);

  const remoteLocalParam =
    localFilter === LOCAL_FILTER_READY || localFilter === LOCAL_FILTER_MISSING
      ? localFilter
      : undefined;

  const {
    data: remoteCache,
    loading: remoteLoading,
    refresh: refreshRemote,
  } = useRequest(
    async () => {
      const res = await listRemoteEngineTags({
        engine:
          engineFilter === ENGINE_FILTER_ALL ? undefined : engineFilter,
        q: remoteSearchQ || undefined,
        page: remotePage,
        page_size: remotePageSize,
        local: remoteLocalParam,
      });
      if (!res?.success) {
        setRemoteTotal(0);
        return {
          synced: false,
          tags: [] as RemoteEngineTag[],
        };
      }
      const total = Number(res.data?.count || 0);
      setRemoteTotal(Number.isFinite(total) ? total : 0);
      return {
        synced: Boolean(res.data?.synced),
        updated_at: res.data?.updated_at as number | null | undefined,
        tags: (res.data?.tags || []) as RemoteEngineTag[],
        count: total,
      };
    },
    {
      // 进页若探测到服务端缓存会置 true；或手动 sync 成功后启用
      ready: includeRemoteCompare,
      refreshDeps: [
        engineFilter,
        remoteSearchQ,
        registryTick,
        includeRemoteCompare,
        remotePage,
        remotePageSize,
        remoteLocalParam,
        localInventoryKey,
      ],
      cacheKey: `engine-images-remote-${engineFilter}-${remotePage}-${remotePageSize}-${remoteSearchQ}-${remoteLocalParam || 'all'}-${localInventoryKey}-${registryTick}`,
      staleTime: 30_000,
    },
  );

  useEffect(() => {
    setRemotePage(1);
  }, [
    engineFilter,
    remoteSearchQ,
    registryTick,
    includeRemoteCompare,
    remoteLocalParam,
  ]);

  const refreshLocalForce = () => {
    forceLocalRef.current = true;
    refreshQuickLocal();
    refreshFullLocal();
  };

  const {
    tasks,
    activeTasks,
    enqueue,
    handleCancel,
    handleRetry,
    handleDelete,
    handleClear,
  } = useEngineTasks(() => {
    refreshLocalForce();
  });

  const entries = catalogList;
  const localNodes = localData?.nodes || [];
  const localImages = localData?.images || [];
  /**
   * quick（或本机探测失败）后即可递补内置 catalog，不等 full。
   * full 只后台更新 worker「已就绪」覆盖，避免进页干等第二次/多节点 Docker 扫描。
   */
  const catalogReadyToMerge =
    Boolean(localData) ||
    Boolean(localRequestError) ||
    (!quickLocalLoading && !fullLocalLoading && !localData);

  const localNodeErrors = useMemo(
    () =>
      (localNodes || [])
        .filter((n) => Boolean(n.error))
        .map((n) => ({
          node_id: n.node_id,
          role: n.role,
          error: String(n.error || ''),
        })),
    [localNodes],
  );

  const localListAlert = useMemo(() => {
    const nodeMsgs = localNodeErrors.map((n) => n.error).join('\n');
    const raw =
      (localRequestError as Error | undefined)?.message || nodeMsgs || '';
    if (!raw && !localRequestError && !localNodeErrors.length) return null;
    const missingSdk =
      /No module named ['"]?docker['"]?/i.test(raw) ||
      /No module named ['"]?kubernetes['"]?/i.test(raw) ||
      /ModuleNotFoundError:\s*docker/i.test(raw) ||
      /ModuleNotFoundError:\s*kubernetes/i.test(raw);
    if (missingSdk) {
      return {
        title: String(lGet('models.engines.localSdkMissingTitle')),
        description: String(lGet('models.engines.localSdkMissing')),
      };
    }
    if (localRequestError) {
      return {
        title: String(lGet('models.engines.localListFailed')),
        description: String(
          (localRequestError as Error)?.message ||
            lGet('models.engines.localListFailed'),
        ),
      };
    }
    if (localNodeErrors.length) {
      return {
        title: String(lGet('models.engines.nodeError')),
        description: localNodeErrors
          .map((n) => `${n.role || n.node_id}: ${n.error}`)
          .join('\n'),
      };
    }
    return null;
  }, [localRequestError, localNodeErrors]);

  const localAlertShownRef = useRef<string | null>(null);
  useEffect(() => {
    if (!localListAlert) {
      localAlertShownRef.current = null;
      return;
    }
    const key = `${localListAlert.title}\n${localListAlert.description}`;
    if (localAlertShownRef.current === key) return;
    localAlertShownRef.current = key;
    message.error(localListAlert.title);
  }, [localListAlert, message]);

  /** 列表态：任一目标节点（含 supervisor）有镜像即视为已缓存 */
  const isLocalReady = (image: string) =>
    Boolean(image) && coverageForImage(localNodes, image).ready > 0;

  /** 本机（含 supervisor）已有：首屏 quick 结果与删除/迁移可用性 */
  const isOnHost = (image: string) =>
    Boolean(image) &&
    (anyNodeHasImage(localNodes, image) ||
      localImages.some((img) => imageMatches(img, image)));

  const nodeOptions = useMemo(() => {
    const list = (localNodes || []).map((n) => ({
      node_id: n.node_id,
      role: n.role,
      label: n.label,
      ip_address: n.ip_address,
    })) as EngineNodeInfo[];
    if (list.some((n) => n.node_id === SUPERVISOR_NODE_ID)) return list;
    return [
      {
        node_id: SUPERVISOR_NODE_ID,
        role: 'supervisor',
        label: 'Supervisor',
      },
      ...list,
    ];
  }, [localNodes]);

  const versionRows: VersionRow[] = useMemo(() => {
    const localRows: VersionRow[] = [];
    if (catalogReadyToMerge || localImages.length) {
      const knownImages: string[] = [];
      for (const img of localImages) {
        if (!img || knownImages.some((k) => imageMatches(k, img))) continue;
        const eng = inferEngineForLocalImage(
          img,
          entries,
          registryCfg?.repos || undefined,
        );
        if (!eng) continue;
        if (enabledEngines.size > 0 && !enabledEngines.has(eng)) continue;
        const aliases = localImages.filter((x) => x && imageMatches(x, img));
        const display = preferCanonicalLocalImage(
          aliases,
          eng,
          entries,
          registryCfg?.repos || undefined,
        );
        const catalogHit = entries.find((e) => imageMatches(e.image, display));
        if (catalogHit) {
          knownImages.push(display);
          localRows.push({
            ...catalogHit,
            image: display,
            recommended: true,
            local_present: true,
            key: catalogRowKey(catalogHit),
          });
          continue;
        }
        knownImages.push(display);
        localRows.push({
          engine: eng,
          version: localImageVersion(display),
          image: display,
          description: undefined,
          recommended: false,
          local_present: true,
          key: `local:${display}`,
        });
      }
    }

    // 远程对比：本机行始终保留；再叠当前页 remote tags（命中则合并）。
    // 远端 0 条时退回纯本机，避免空态把已有镜像藏掉。
    if (!includeRemoteCompare) {
      return localRows;
    }
    const remoteTags = remoteCache?.tags || [];
    if (!remoteTags.length) {
      return localRows;
    }

    const coveredLocal = new Set<string>();
    const remoteRows: VersionRow[] = [];
    const seen = new Set<string>();
    for (const tag of remoteTags) {
      if (!tag?.name || !tag?.image) continue;
      const eng = (tag.engine || '').toLowerCase();
      if (!eng) continue;
      if (enabledEngines.size > 0 && !enabledEngines.has(eng)) continue;
      const key = `${eng}@${tag.name}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const localImg =
        preferCanonicalLocalImage(
          localImages.filter((img) => imageMatches(img, tag.image)),
          eng,
          entries,
          registryCfg?.repos || undefined,
        ) || localImages.find((img) => imageMatches(img, tag.image));
      if (localImg) coveredLocal.add(localImg);
      const catalogHit = entries.find(
        (e) =>
          e.engine.toLowerCase() === eng &&
          (e.version === tag.name || imageMatches(e.image, tag.image)),
      );
      if (catalogHit) {
        remoteRows.push({
          ...catalogHit,
          image: localImg || tag.image,
          recommended: true,
          last_updated: tag.last_updated,
          full_size: tag.full_size,
          local_present: Boolean(tag.local_present) || Boolean(localImg),
          key: catalogRowKey(catalogHit),
        });
      } else {
        remoteRows.push({
          engine: eng,
          version: tag.name,
          image: localImg || tag.image,
          description: undefined,
          recommended: false,
          last_updated: tag.last_updated,
          full_size: tag.full_size,
          local_present: Boolean(tag.local_present) || Boolean(localImg),
          key: `remote:${key}`,
        });
      }
    }
    const localOnly = localRows.filter(
      (row) =>
        !coveredLocal.has(row.image) &&
        !remoteRows.some((r) => imageMatches(r.image, row.image)),
    );
    return [...localOnly, ...remoteRows];
  }, [
    entries,
    remoteCache,
    localImages,
    registryCfg,
    catalogReadyToMerge,
    includeRemoteCompare,
    enabledEngines,
  ]);

  const filtered = useMemo(() => {
    const draft = versionQuery.trim().toLowerCase();
    const tags = keywordTags.map((t) => t.toLowerCase()).filter(Boolean);
    const rows = versionRows.filter((row) => {
      if (
        engineFilter !== ENGINE_FILTER_ALL &&
        row.engine.toLowerCase() !== engineFilter
      ) {
        return false;
      }
      // 远程对比：本机状态由服务端 local=ready|missing 筛选，勿再按当前页客户端滤空
      if (!includeRemoteCompare) {
        const cov = coverageForImage(localNodes, row.image);
        const present = localScanComplete
          ? cov.total > 0
            ? cov.ready > 0
            : isOnHost(row.image)
          : isOnHost(row.image);
        if (localFilter === LOCAL_FILTER_READY && !present) return false;
        if (localFilter === LOCAL_FILTER_MISSING && present) return false;
      }

      const hay = [
        row.engine,
        row.version,
        row.image,
        row.description,
        row.cuda,
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase();

      for (const tag of tags) {
        if (!hay.includes(tag)) return false;
      }
      if (draft && !hay.includes(draft)) return false;
      return true;
    });
    // 远程有 tag 时保持服务端页顺序；远端 0 条时按本机规则排序
    if (includeRemoteCompare && remoteTotal > 0) {
      return rows;
    }
    // 仅本机：本机已有置顶 → 引擎序 → 更新时间
    rows.sort((a, b) => {
      const rank = (img: string) => {
        if (!localScanComplete) return isOnHost(img) ? 0 : 1;
        const cov = coverageForImage(localNodes, img);
        const present =
          cov.total > 0 ? cov.ready > 0 : isOnHost(img);
        return present ? 0 : 1;
      };
      const aCached = rank(a.image);
      const bCached = rank(b.image);
      if (aCached !== bCached) return aCached - bCached;
      const byEngine = compareEngines(a.engine, b.engine);
      if (byEngine !== 0) return byEngine;
      const ta = updatedAtMs(a.last_updated);
      const tb = updatedAtMs(b.last_updated);
      if (!ta !== !tb) return ta ? -1 : 1;
      if (ta && tb) {
        const byTime = isUpdatedAtSort ? ta - tb : tb - ta;
        if (byTime !== 0) return byTime;
      }
      return (b.version || '').localeCompare(a.version || '', undefined, {
        numeric: true,
      });
    });
    return rows;
  }, [
    versionRows,
    localFilter,
    localNodes,
    localImages,
    localScanComplete,
    includeRemoteCompare,
    remoteTotal,
    engineFilter,
    versionQuery,
    keywordTags,
    isUpdatedAtSort,
  ]);

  const addKeywordTag = (raw?: string) => {
    const value = (raw ?? versionQuery).trim();
    if (!value) {
      message.warning(lGet('models.engines.keywordTagEmpty'));
      return;
    }
    const valueLc = value.toLowerCase();
    if (keywordTags.some((t) => t.toLowerCase() === valueLc)) {
      message.info(lGet('models.engines.keywordTagExists'));
      setVersionQuery('');
      return;
    }
    // 新标签覆盖已有子串标签；若新标签本身是已有标签的子串则忽略
    setKeywordTags((prev) => {
      if (prev.some((t) => t.toLowerCase().includes(valueLc) && t.toLowerCase() !== valueLc)) {
        return prev;
      }
      return [...prev.filter((t) => !valueLc.includes(t.toLowerCase())), value];
    });
    setVersionQuery('');
  };

  const removeKeywordTag = (value: string) => {
    setKeywordTags((prev) => prev.filter((t) => t !== value));
  };

  const stopSyncPoll = () => {
    if (syncPollRef.current) {
      clearInterval(syncPollRef.current);
      syncPollRef.current = null;
    }
  };

  const stopManualPoll = () => {
    if (manualPollRef.current) {
      clearInterval(manualPollRef.current);
      manualPollRef.current = null;
    }
  };

  const finishSyncProgress = (prog: RemoteSyncProgress) => {
    stopSyncPoll();
    setSyncingRemote(false);
    if (prog.status === 'succeeded') {
      message.success(
        String(
          lGet('models.engines.remoteSyncDone', undefined, {
            count: prog.tag_count ?? 0,
          }),
        ),
      );
      setIncludeRemoteCompare(true);
      setLocalFilter(LOCAL_FILTER_ALL);
      setRegistryTick((n) => n + 1);
    } else if (prog.status === 'failed') {
      message.error(
        humanizeEngineImageError(prog.message) ||
          prog.message ||
          String(lGet('models.engines.remoteSyncFailed')),
      );
    }
    setTimeout(() => {
      setSyncProgress(null);
      setShowSyncModal(false);
    }, 800);
  };

  const startSyncProgressPoll = () => {
    stopSyncPoll();
    syncPollRef.current = setInterval(async () => {
      const progRes = await getRemoteSyncProgress();
      if (!progRes?.success) return;
      const prog = progRes.data as RemoteSyncProgress;
      setSyncProgress(prog);
      if (prog.status === 'succeeded' || prog.status === 'failed') {
        finishSyncProgress(prog);
      }
    }, 600);
  };

  useEffect(
    () => () => {
      stopSyncPoll();
      stopManualPoll();
    },
    [],
  );

  // 刷新/重进页：若 Supervisor 仍在同步，恢复轮询并可再次打开进度
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const progRes = await getRemoteSyncProgress();
        if (cancelled || !progRes?.success) return;
        const prog = progRes.data as RemoteSyncProgress;
        if (prog?.status !== 'running') return;
        setSyncingRemote(true);
        setSyncProgress(prog);
        setShowSyncModal(true);
        startSyncProgressPoll();
      } catch {
        /* ignore */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount resume only
  }, []);

  const handleRegistryReset = () => {
    Modal.confirm({
      title: lGet('models.engines.registryReset'),
      content: lGet('models.engines.registryResetConfirm'),
      okText: lGet('models.engines.registryReset'),
      okButtonProps: { danger: true },
      cancelText: lGet('models.engines.cancel'),
      onOk: async () => {
        setResettingRegistry(true);
        try {
          const res = await resetEngineRegistry();
          if (!res?.success) {
            message.error(
              stringifyDetail(res?.data?.detail) ||
                String(lGet('models.engines.registryResetFailed')),
            );
            return;
          }
          message.success(lGet('models.engines.registryResetOk'));
          if (res.data) mutateRegistry(res.data as EngineRegistryConfig);
          else void refreshRegistry();
          setRegistryTick((n) => n + 1);
          setRegistryBarTick((n) => n + 1);
          if (includeRemoteCompare) void refreshRemote();
        } catch (e) {
          message.error(
            stringifyDetail(
              (e as { data?: { detail?: unknown } })?.data?.detail,
            ) || String(lGet('models.engines.registryResetFailed')),
          );
        } finally {
          setResettingRegistry(false);
        }
      },
    });
  };

  const dismissManualProgress = () => {
    stopManualPoll();
    setManualProgress(null);
  };

  const handleManualStarted = (progress: EnginePullProgress) => {
    // 与下载/迁移一致：写入任务进度（刷新后仍可从后端 hydrate）
    enqueue(progress);
    setManualProgress(progress);
    stopManualPoll();
    const taskId = progress.task_id;
    if (!taskId) return;
    // 弹窗仅作快捷预览；关闭后由 useEngineTasks 继续轮询任务进度
    manualPollRef.current = setInterval(async () => {
      const progRes = await getEnginePullProgress(taskId);
      if (!progRes?.success) return;
      const prog = progRes.data as EnginePullProgress;
      setManualProgress((cur) => (cur ? prog : cur));
      enqueue(prog);
      if (
        prog.status === 'succeeded' ||
        prog.status === 'failed' ||
        prog.status === 'cancelled'
      ) {
        stopManualPoll();
        if (prog.status === 'succeeded') {
          message.success(
            String(
              lGet('models.engines.manualRegisterDone', undefined, {
                engine: prog.engine || '—',
              }),
            ),
          );
          refreshLocalForce();
          setLocalFilter(LOCAL_FILTER_READY);
        } else if (prog.status === 'failed') {
          message.error(
            prog.error ||
              prog.message ||
              String(lGet('models.engines.manualRegisterFailed')),
          );
        }
        setTimeout(() => setManualProgress(null), 800);
      }
    }, 800);
  };

  const dismissSyncProgress = () => setShowSyncModal(false);

  const handleSyncRemote = async () => {
    // 关闭进度窗后同步仍在后台；再点按钮只重新打开进度，不重复发起
    if (syncingRemote) {
      setShowSyncModal(true);
      return;
    }
    setSyncingRemote(true);
    setShowSyncModal(true);
    setSyncProgress({
      status: 'running',
      message: 'starting',
      engines_done: 0,
      engines_total: 0,
      tag_count: 0,
    });
    try {
      const res = await syncRemoteEngineTags();
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.remoteSyncFailed')),
        );
        setSyncingRemote(false);
        setShowSyncModal(false);
        setSyncProgress(null);
        return;
      }
      setSyncProgress(res.data as RemoteSyncProgress);
      startSyncProgressPoll();
    } catch (e) {
      setSyncingRemote(false);
      setShowSyncModal(false);
      setSyncProgress(null);
      message.error(String(lGet('models.engines.remoteSyncFailed')));
    }
  };

  const openAction = (draft: ActionDraft) => {
    setActionDraft(draft);
    setActionOpen(true);
  };

  const activeBusy = (engine: string, version: string) =>
    activeTasks.some(
      (t) =>
        t.engine?.toLowerCase() === engine.toLowerCase() &&
        t.version === version,
    );

  const tabOptions = useMemo(
    () => [
      {
        label: l('models.engines.tabImages'),
        value: TAB_IMAGES,
      },
      {
        label: l('models.engines.tabRegistry'),
        value: TAB_REGISTRY,
      },
      {
        label: l('models.engines.tabTasks'),
        value: TAB_TASKS,
      },
    ],
    [tab],
  );

  const columns: ColumnsType<VersionRow> = [
    {
      title: l('models.engines.engine'),
      dataIndex: 'engine',
      width: 100,
      render: (engine: string) => (
        <StatusTag tone={engineTagTone(engine)} dot={false}>
          {engineTagLabel(engine)}
        </StatusTag>
      ),
    },
    {
      title: (
        <div
          className="flex items-center gap-x-[8px] cursor-pointer select-none"
          onClick={() => setUpdatedAtSort((v) => !v)}
        >
          {l('models.engines.updatedAt')}
          <ArrowUp
            size={16}
            className={classNames('duration-300', {
              // 默认新→旧（降序）显示 ↓；点击后旧→新显示 ↑
              'rotate-180': !isUpdatedAtSort,
            })}
          />
        </div>
      ),
      dataIndex: 'last_updated',
      width: 150,
      render: (raw?: string) => {
        if (!raw) {
          return <span className="text-muted text-[12px]">—</span>;
        }
        const ms = updatedAtMs(raw);
        if (!ms) {
          return (
            <span className="text-muted text-[12px] font-mono tabular-nums">
              {raw}
            </span>
          );
        }
        return (
          <span
            className="text-[12px] text-secondary font-mono tabular-nums"
            title={raw}
          >
            {formatDisplayTime(ms, 'YYYY-MM-DD HH:mm')}
          </span>
        );
      },
    },
    {
      title: l('models.engines.image'),
      dataIndex: 'image',
      ellipsis: true,
      render: (image: string) => (
        <Tooltip title={image}>
          <code className="block max-w-full truncate rounded bg-[var(--c-surface-2)] px-1.5 py-0.5 text-[11px] text-secondary font-mono leading-relaxed border border-[color:var(--c-border-light)]">
            {image}
          </code>
        </Tooltip>
      ),
    },
    {
      title: l('models.engines.imageSize'),
      dataIndex: 'full_size',
      width: 110,
      align: 'right',
      render: (size?: number | null) => (
        <span className="text-[12px] text-secondary font-mono tabular-nums">
          {formatSizeGB(size)}
        </span>
      ),
    },
    {
      title: (
        <Tooltip title={l('models.engines.localStatusHint')}>
          <span>{l('models.engines.localStatus')}</span>
        </Tooltip>
      ),
      key: 'local',
      width: 130,
      render: (_, row) => {
        if (!localScanComplete) {
          // full 扫描中：认服务端 local_present / 本机并集，避免镜像源前缀不一致时误标未下载
          const onHost = Boolean(row.local_present) || isOnHost(row.image);
          return (
            <div className="flex flex-col gap-1">
              <StatusTag tone={onHost ? 'success' : 'neutral'}>
                {onHost
                  ? l('models.engines.localOnHost')
                  : l('models.engines.localMissing')}
              </StatusTag>
              <span className="text-[11px] text-muted">—</span>
            </div>
          );
        }
        const cov = coverageForImage(localNodes, row.image);
        // 无 worker 回退本机；远程行另认服务端 local_present（镜像源前缀不一致时覆盖仍可能 0）
        const present =
          Boolean(row.local_present) ||
          isOnHost(row.image) ||
          (cov.total > 0 ? cov.ready > 0 : false);
        const readyLabel =
          cov.total > 0
            ? l('models.engines.localReady')
            : l('models.engines.localOnHost');
        const covReady =
          cov.total > 0
            ? Math.max(cov.ready, present ? 1 : 0)
            : cov.ready;
        return (
          <div className="flex flex-col gap-1">
            <StatusTag tone={present ? 'success' : 'neutral'}>
              {present ? readyLabel : l('models.engines.localMissing')}
            </StatusTag>
            {cov.total > 0 ? (
              <span className="text-[11px] text-muted font-mono tabular-nums">
                {lGet('models.engines.coverageHint', undefined, {
                  ready: Math.min(covReady, cov.total),
                  total: cov.total,
                })}
              </span>
            ) : (
              <span className="text-[11px] text-muted">—</span>
            )}
          </div>
        );
      },
    },
    {
      title: l('models.engines.actions'),
      key: 'actions',
      width: 64,
      fixed: 'right',
      align: 'center',
      render: (_, row) => {
        const busy = activeBusy(row.engine, row.version);
        const onAnyNode = anyNodeHasImage(localNodes, row.image);
        const base = {
          engine: row.engine,
          version: row.version,
          image: row.image,
          description: row.description,
          fromRemote: !row.recommended,
        };
        const items: MenuProps['items'] = [
          {
            key: 'download',
            icon: <Download size={14} />,
            label: l('models.engines.download'),
            disabled: busy,
            onClick: () => openAction({ ...base, mode: 'download' }),
          },
          {
            key: 'delete',
            icon: <Trash2 size={14} />,
            label: l('models.engines.delete'),
            disabled: busy || !onAnyNode,
            onClick: () => openAction({ ...base, mode: 'delete' }),
          },
          {
            key: 'migrate',
            icon: <ArrowRightLeft size={14} />,
            label: l('models.engines.migrate'),
            disabled: busy || !onAnyNode,
            onClick: () => openAction({ ...base, mode: 'migrate' }),
          },
        ];
        return (
          <Dropdown menu={{ items }} trigger={['click']} placement="bottomRight">
            <IconButton
              className="!w-8 !h-8"
              aria-label={String(lGet('models.engines.moreActions'))}
            >
              <MoreVertical size={16} />
            </IconButton>
          </Dropdown>
        );
      },
    },
  ];

  let imagesBody: React.ReactNode = null;
  let imagesEmpty: React.ReactNode = null;
  // 首屏只等本机；未返回前不铺 catalog。catalog/remote 递补。
  if (localFirstLoading) {
    imagesEmpty = <SectionLoading />;
  } else if (catalogError && !localImages.length && !versionRows.length) {
    imagesEmpty = (
      <EmptyState
        customIcon={<Container size={48} className="text-muted" />}
        title={l('models.engines.loadFailed')}
        description={String((catalogError as Error)?.message || '')}
        action={
          <Button type="primary" onClick={() => refreshCatalog()}>
            {l('models.engines.refresh')}
          </Button>
        }
      />
    );
  } else if (includeRemoteCompare && remoteLoading && !versionRows.length) {
    // 换筛选条件重新拉远程时不要闪「目录尚未就绪」
    imagesEmpty = <SectionLoading />;
  } else if (!versionRows.length && hasActiveImageFilter) {
    // 有筛选但结果为空：是筛空，不是目录未就绪
    imagesEmpty = (
      <EmptyState
        customIcon={<Container size={48} className="text-muted" />}
        title={l('models.engines.noMatch')}
        description={l('models.engines.noMatchHint')}
        action={
          <Button
            onClick={() => {
              setLocalFilter(LOCAL_FILTER_ALL);
              setEngineFilter(ENGINE_FILTER_ALL);
              setVersionQuery('');
              setKeywordTags([]);
            }}
          >
            {l('models.engines.clearFilters')}
          </Button>
        }
      />
    );
  } else if (!versionRows.length) {
    imagesEmpty = (
      <EmptyState
        customIcon={<Container size={48} className="text-muted" />}
        title={l('models.engines.empty')}
        description={
          localListAlert
            ? localListAlert.description
            : includeRemoteCompare && remoteTotal > 0
              ? l('models.engines.emptyHint')
              : l('models.engines.emptyLocalOnlyHint',
                )
        }
        action={
          <Button type="primary" onClick={refreshLocalForce}>
            {l('models.engines.refresh')}
          </Button>
        }
      />
    );
  } else if (
    !filtered.length &&
    !(includeRemoteCompare && remoteTotal > 0)
  ) {
    // 远程分页时：当前页被本地筛选筛空也要保留 Table+分页，否则无法翻到其它页
    imagesEmpty = (
      <EmptyState
        customIcon={<Container size={48} className="text-muted" />}
        title={l('models.engines.noMatch')}
        description={l('models.engines.noMatchHint')}
        action={
          <Button
            onClick={() => {
              setLocalFilter(LOCAL_FILTER_ALL);
              setEngineFilter(ENGINE_FILTER_ALL);
              setVersionQuery('');
              setKeywordTags([]);
            }}
          >
            {l('models.engines.clearFilters')}
          </Button>
        }
      />
    );
  } else {
    const pageSize = remotePageSize || 10;
    // 远端无 tag 时用本机客户端分页；有远端时保留本机行+当前页，勿截断本机
    const serverRemotePaging = includeRemoteCompare && remoteTotal > 0;
    const tableData = serverRemotePaging
      ? filtered
      : filtered;
    const localOnlyExtra = serverRemotePaging
      ? filtered.filter((r) => String(r.key || '').startsWith('local:')).length
      : 0;
    imagesBody = (
      <Table
        rowKey="key"
        /* 本机行已出则不再用全量扫描挡表格转圈（后台静默更新状态） */
        /* 本机行已出则不挡表格；catalog/remote 仅在「更新仓库」后合并 */
        columns={columns}
        dataSource={
          serverRemotePaging ? asServerPageRows(tableData, pageSize + localOnlyExtra) : tableData
        }
        loading={includeRemoteCompare ? remoteLoading && !localImages.length : false}
        locale={
          !filtered.length
            ? {
                emptyText: l('models.engines.noMatchHint',
                ),
              }
            : undefined
        }
        pagination={
          serverRemotePaging
            ? {
                current: remotePage,
                pageSize,
                total: remoteTotal + localOnlyExtra,
                showSizeChanger: false,
                hideOnSinglePage: false,
                showTotal: (total) =>
                  l('models.engines.taskPageTotal', { total }),
                onChange: (page) => {
                  setRemotePage(page);
                  if (remotePageSize !== 10) setRemotePageSize(10);
                },
              }
            : {
                pageSize: 10,
                showSizeChanger: false,
                hideOnSinglePage: false,
                showTotal: (total) =>
                  l('models.engines.taskPageTotal', { total }),
              }
        }
        scroll={{ x: 1020 }}
      />
    );
  }

  // 顺序：状态 → 引擎 → 关键词标签（左）… 检索 + 加号（右）；控件统一 h-8
  const filterBar = (
    <FilterBar
      className="!items-center !py-2"
      trailing={
        <div className="flex items-center gap-1.5 min-w-0">
          <Input
            allowClear
            size="middle"
            className="min-w-[200px] w-[220px] !h-8"
            value={versionQuery}
            placeholder={l('models.engines.versionFilterPlaceholder')}
            onChange={(e) => setVersionQuery(e.target.value || '')}
            onPressEnter={() => addKeywordTag(versionQuery)}
          />
          <Tooltip title={l('models.engines.keywordTagAddHint')}>
            <Button
              type="default"
              size="middle"
              className="!w-8 !h-8 !min-w-8 !p-0 !inline-flex !items-center !justify-center shrink-0"
              icon={<Plus size={15} strokeWidth={2.25} />}
              aria-label={String(lGet('models.engines.keywordTagAdd'))}
              onClick={() => addKeywordTag()}
            />
          </Tooltip>
          <Tooltip title={l('models.engines.refreshLocalHint')}>
            <Button
              type="default"
              size="middle"
              className="!w-8 !h-8 !min-w-8 !p-0 !inline-flex !items-center !justify-center shrink-0"
              icon={<RefreshCw size={15} strokeWidth={2.25} />}
              loading={quickLocalLoading || fullLocalLoading}
              aria-label={String(lGet('models.engines.refresh'))}
              onClick={refreshLocalForce}
            />
          </Tooltip>
        </div>
      }
    >
      <Select
        size="middle"
        className="min-w-[120px] w-[128px] !h-8"
        value={localFilter}
        onChange={setLocalFilter}
        options={[
          {
            value: LOCAL_FILTER_ALL,
            label: l('models.engines.filterAll'),
          },
          {
            value: LOCAL_FILTER_READY,
            label: l('models.engines.localReady'),
          },
          {
            value: LOCAL_FILTER_MISSING,
            label: l('models.engines.localMissing'),
          },
        ]}
      />
      <Select
        size="middle"
        className="min-w-[128px] w-[140px] !h-8"
        value={engineFilter}
        onChange={(v) => {
          setEngineFilter(v);
          setVersionQuery('');
        }}
        options={[
          {
            value: ENGINE_FILTER_ALL,
            label: l('models.engines.filterAllEngines'),
          },
          ...engineOptions.map((e) => ({
            value: e,
            label: engineTagLabel(e),
          })),
        ]}
      />
      {keywordTags.map((tag) => (
        <Tag
          key={tag}
          closable
          className="!m-0 !inline-flex !items-center !h-8 !px-2.5 !leading-none !text-[12px] !rounded-md"
          onClose={(e) => {
            e.preventDefault();
            removeKeywordTag(tag);
          }}
        >
          {tag}
        </Tag>
      ))}
    </FilterBar>
  );

  return (
    <PageContainer
      title={l('models.engines.title')}
      subTitle={
        tab === TAB_TASKS
          ? l('models.engines.tasksSubTitle')
          : tab === TAB_REGISTRY
            ? l('models.engines.registrySubTitle',
              )
            : l('models.engines.subTitle')
      }
      extraContent={
        activeTasks.length > 0 && tab === TAB_IMAGES ? (
          <button
            type="button"
            className="text-sm text-primary cursor-pointer bg-transparent border-0 p-0"
            onClick={() => history.push(`/models/engines/${TAB_TASKS}`)}
          >
            {lGet('models.engines.activeTasksHint', undefined, {
              count: activeTasks.length,
            })}
          </button>
        ) : null
      }
    >
      <div className="flex flex-col w-full gap-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="overflow-x-auto min-w-0 pb-0.5">
            <PillTabs
              value={tab}
              options={tabOptions}
              onChange={(key) => history.push(`/models/engines/${String(key)}`)}
            />
          </div>
          {tab === TAB_IMAGES ? (
            <div className="flex items-center gap-2 shrink-0">
              <span
                className="text-xs text-muted whitespace-nowrap"
                title={String(lGet('models.engines.runtimeSourceHint'))}
              >
                {lGet('models.engines.runtimeCurrent', undefined, {
                  runtime: localData?.runtime || runtime,
                })}
              </span>
              <Tooltip title={lGet('models.engines.manualRegisterHint')}>
                <Button
                  icon={<PackagePlus size={16} />}
                  disabled={syncingRemote || Boolean(manualProgress)}
                  onClick={() => setManualOpen(true)}
                >
                  {l('models.engines.manualRegister')}
                </Button>
              </Tooltip>
              <Tooltip
                title={
                  syncingRemote
                    ? lGet('models.engines.remoteSyncProgressReopenHint')
                    : lGet('models.engines.remoteSyncHint')
                }
              >
                {/* 不用 ant loading：会吞 onClick，同步中无法再次打开进度 */}
                <Button
                  icon={
                    syncingRemote ? (
                      <Loader2 size={16} className="animate-spin" />
                    ) : (
                      <CloudDownload size={16} />
                    )
                  }
                  disabled={Boolean(manualProgress)}
                  onClick={handleSyncRemote}
                >
                  {l('models.engines.remoteSync')}
                </Button>
              </Tooltip>
            </div>
          ) : null}
          {tab === TAB_REGISTRY ? (
            <div className="flex items-center gap-2 shrink-0">
              <Tooltip title={lGet('models.engines.registryResetHint')}>
                <Button
                  icon={<RotateCcw size={16} />}
                  loading={resettingRegistry}
                  disabled={syncingRemote}
                  onClick={handleRegistryReset}
                  className="!bg-white hover:!bg-white"
                  style={{ backgroundColor: '#fff' }}
                >
                  {l('models.engines.registryReset')}
                </Button>
              </Tooltip>
            </div>
          ) : null}
          {tab === TAB_TASKS ? (
            <div className="flex items-center gap-2 shrink-0">
              <Tooltip title={lGet('models.engines.clearTasksHint')}>
                <Button
                  icon={<Trash2 size={16} />}
                  danger
                  disabled={tasks.length === 0}
                  onClick={handleClear}
                >
                  {l('models.engines.clearTasks')}
                </Button>
              </Tooltip>
            </div>
          ) : null}
        </div>

        {tab === TAB_TASKS ? (
          <TaskPanel
            tasks={tasks}
            onCancel={handleCancel}
            onRetry={handleRetry}
            onDelete={handleDelete}
            onModify={(row) => {
              const mode = row.kind === 'migrate' ? 'migrate' : 'download';
              openAction({
                engine: row.engine || '',
                version: row.version || '',
                image: row.image || '',
                fromRemote: true,
                mode,
              });
            }}
          />
        ) : tab === TAB_REGISTRY ? (
          <RegistryBar
            onSaved={(cfg) => {
              if (cfg) mutateRegistry(cfg);
              else void refreshRegistry();
              setRegistryTick((n) => n + 1);
              if (includeRemoteCompare) void refreshRemote();
            }}
            onSyncRemote={handleSyncRemote}
            syncingRemote={syncingRemote}
            reloadToken={registryBarTick}
          />
        ) : (
          <div className="flex flex-col gap-4 w-full">
            {localListAlert ? (
              <Alert
                type="error"
                showIcon
                message={localListAlert.title}
                description={
                  <div className="whitespace-pre-wrap text-[12px] font-mono">
                    {localListAlert.description}
                  </div>
                }
                action={
                  <Button size="small" danger onClick={refreshLocalForce}>
                    {l('models.engines.refresh')}
                  </Button>
                }
              />
            ) : null}
            <div className={panelShell}>
              <div className="px-3 py-3">{filterBar}</div>
            </div>
            <div className={panelShell}>
              {imagesEmpty ? (
                <div className="px-4 py-6">{imagesEmpty}</div>
              ) : (
                imagesBody
              )}
            </div>
          </div>
        )}
      </div>

      <ActionDrawer
        open={actionOpen}
        draft={actionDraft}
        nodes={nodeOptions}
        localNodes={localNodes}
        localImages={localImages}
        onClose={() => setActionOpen(false)}
        onStarted={(p) => {
          enqueue(p);
        }}
      />
      <ManualRegisterModal
        open={manualOpen}
        onClose={() => setManualOpen(false)}
        onStarted={handleManualStarted}
      />
      <Modal
        open={Boolean(showSyncModal && syncProgress)}
        title={l('models.engines.remoteSync')}
        closable
        maskClosable
        keyboard
        destroyOnClose
        onCancel={dismissSyncProgress}
        centered
        width={420}
        footer={
          <div className="flex justify-end">
            <Button onClick={dismissSyncProgress}>
              {l('models.engines.remoteSyncProgressDismiss')}
            </Button>
          </div>
        }
      >
        <div className="flex flex-col gap-3 py-1">
          <p className="text-[12px] text-muted m-0 leading-relaxed">
            {l('models.engines.remoteSyncProgressLeaveHint')}
          </p>
          <p className="text-sm text-secondary m-0">
            {lGet('models.engines.remoteSyncProgressHint', undefined, {
              engine: syncProgress?.engine || '—',
              done: syncProgress?.engines_done ?? 0,
              total: syncProgress?.engines_total ?? 0,
              count: syncProgress?.tag_count ?? 0,
            })}
          </p>
          <Progress
            percent={
              syncProgress?.engines_total
                ? Math.round(
                    ((syncProgress.engines_done || 0) /
                      syncProgress.engines_total) *
                      100,
                  )
                : 0
            }
            status={
              syncProgress?.status === 'failed' ? 'exception' : 'active'
            }
          />
          <div className="text-[12px] text-muted font-mono tabular-nums">
            {lGet('models.engines.remoteSyncTagCount', undefined, {
              count: syncProgress?.tag_count ?? 0,
            })}
          </div>
        </div>
      </Modal>
      <Modal
        open={Boolean(manualProgress)}
        title={l('models.engines.manualRegisterProgress')}
        closable
        maskClosable
        keyboard
        onCancel={dismissManualProgress}
        centered
        width={480}
        footer={
          <div className="flex justify-end gap-2">
            <Button onClick={dismissManualProgress}>
              {l('models.engines.manualRegisterProgressDismiss')}
            </Button>
            <Button
              type="primary"
              onClick={() => {
                dismissManualProgress();
                history.push(`/models/engines/${TAB_TASKS}`);
              }}
            >
              {l('models.engines.manualRegisterViewTasks')}
            </Button>
          </div>
        }
      >
        <div className="flex flex-col gap-3 py-1">
          <p className="text-[12px] text-muted m-0 leading-relaxed">
            {l('models.engines.manualRegisterProgressLeaveHint')}
          </p>
          <p className="text-sm text-secondary m-0 break-all font-mono text-[12px]">
            {lGet('models.engines.manualRegisterProgressHint', undefined, {
              image: manualProgress?.image || '—',
              percent: Math.round((manualProgress?.progress || 0) * 100),
              message:
                humanizeEngineImageError(manualProgress?.message) ||
                manualProgress?.message ||
                '',
            })}
          </p>
          <Progress
            percent={Math.round((manualProgress?.progress || 0) * 100)}
            status={
              manualProgress?.status === 'failed'
                ? 'exception'
                : manualProgress?.status === 'succeeded'
                  ? 'success'
                  : 'active'
            }
          />
        </div>
      </Modal>
    </PageContainer>
  );
};

export default EnginesPage;
