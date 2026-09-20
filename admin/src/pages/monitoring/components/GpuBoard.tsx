import { useMemo, useState, type ReactNode } from 'react';
import { Button, Collapse, Pagination } from 'antd';
import { history } from '@umijs/max';
import cn from 'classnames';
import { ProCard } from '@ant-design/pro-components';
import { l } from '@/utils/intl';
import type { GpuRow, GpuViewMode } from '@/utils/monitorMetrics';
import {
  displayAccelKind,
  exportGpuRowsCsv,
  GPU_CSV_HEADERS,
  groupGpuRowsByKind,
  groupGpuRowsByModel,
  groupGpuRowsByName,
} from '@/utils/monitorMetrics';
import { downloadCsv } from '@/utils/downloadCsv';
import dayjs from 'dayjs';
import GpuMatrixCard from './GpuMatrixCard';
import { MonitorIcons } from '../icons';

export const getDefaultNodeKeys = (byNode: { key: string; hasAlert: boolean; rows: GpuRow[] }[]) => {
  const alertKeys = byNode.filter((n) => n.hasAlert).map((n) => n.key);
  if (alertKeys.length) return alertKeys.slice(0, 3);
  return byNode.slice(0, 1).map((n) => n.key);
};

type FilterKey = 'all' | 'ok' | 'warn' | 'idle' | 'offline';

const FILTERS: { key: FilterKey; labelKey: string; fallback: string }[] = [
  { key: 'all', labelKey: 'monitor.platform.filter.all', fallback: '全部' },
  { key: 'ok', labelKey: 'monitor.platform.filter.ok', fallback: '运行中' },
  { key: 'warn', labelKey: 'monitor.platform.filter.warn', fallback: '告警' },
  { key: 'idle', labelKey: 'monitor.platform.filter.idle', fallback: '空闲' },
  { key: 'offline', labelKey: 'monitor.platform.filter.offline', fallback: '离线' },
];

function matchFilter(row: GpuRow, key: FilterKey): boolean {
  if (key === 'all') return true;
  // 「运行中」：在线且有负载（含告警中的卡，与告警筛选可重叠）
  if (key === 'ok') {
    return row.status !== 'idle' && row.status !== 'offline';
  }
  if (key === 'warn') return row.status === 'warn' || row.status === 'crit';
  return row.status === key;
}

const PAGE_SIZE_NODES = 24;

type Props = {
  gpuRows: GpuRow[];
  loading?: boolean;
  filter: FilterKey;
  onFilterChange: (f: FilterKey) => void;
  activeNodeKeys?: string[];
  onActiveNodeKeysChange?: (keys: string[]) => void;
  toolbarExtra?: ReactNode;
  groupMode?: GpuViewMode;
};

const GpuBoard = ({
  gpuRows,
  loading,
  filter,
  onFilterChange,
  activeNodeKeys,
  onActiveNodeKeysChange,
  toolbarExtra,
  groupMode = 'kind',
}: Props) => {
  const [nodePage, setNodePage] = useState(1);
  const [kindPath, setKindPath] = useState<string[]>([]);
  const [internalActive, setInternalActive] = useState<string[]>([]);
  const Download = MonitorIcons.download;

  const filtered = useMemo(
    () => gpuRows.filter((r) => matchFilter(r, filter)),
    [gpuRows, filter],
  );

  const counts = useMemo(() => {
    const ok = gpuRows.filter(
      (r) => r.status !== 'idle' && r.status !== 'offline',
    ).length;
    const warn = gpuRows.filter((r) => r.status === 'warn' || r.status === 'crit').length;
    const idle = gpuRows.filter((r) => r.status === 'idle').length;
    const offline = gpuRows.filter((r) => r.status === 'offline').length;
    return { ok, warn, idle, offline, total: gpuRows.length };
  }, [gpuRows]);

  const byNode = useMemo(() => {
    const map = new Map<string, GpuRow[]>();
    for (const row of filtered) {
      const key = row.workerAddress || row.node;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(row);
    }
    return [...map.entries()].map(([key, rows]) => {
      const utils = rows.map((r) => r.util).filter((u): u is number => u !== null);
      const avgUtil = utils.length
        ? Math.round(utils.reduce((s, u) => s + u, 0) / utils.length)
        : null;
      const avgMem = rows.length
        ? Math.round(rows.reduce((s, r) => s + r.memUsage, 0) / rows.length)
        : 0;
      const hasAlert = rows.some(
        (r) => r.status === 'warn' || r.status === 'crit' || r.status === 'offline',
      );
      return {
        key,
        label: rows[0]?.node || key,
        detail: key,
        rows,
        avgUtil,
        avgMem,
        hasAlert,
      };
    });
  }, [filtered]);

  const pagedNodes = useMemo(() => {
    if (byNode.length <= PAGE_SIZE_NODES) return byNode;
    const start = (nodePage - 1) * PAGE_SIZE_NODES;
    return byNode.slice(start, start + PAGE_SIZE_NODES);
  }, [byNode, nodePage]);

  const defaultOpenKeys = useMemo(() => getDefaultNodeKeys(byNode), [byNode]);
  const activeKeys = activeNodeKeys ?? (internalActive.length ? internalActive : defaultOpenKeys);
  const setActiveKeys = (keys: string[]) => {
    if (onActiveNodeKeysChange) onActiveNodeKeysChange(keys);
    else setInternalActive(keys);
  };

  const byKind = useMemo(() => groupGpuRowsByKind(filtered), [filtered]);
  const byModel = useMemo(() => groupGpuRowsByModel(filtered), [filtered]);
  const kindLevelRows = useMemo(() => {
    if (kindPath.length === 0) return [];
    const kindRows = byKind.find((g) => g.key === kindPath[0])?.rows || [];
    if (kindPath.length === 1) return kindRows;
    return groupGpuRowsByName(kindRows).find((g) => g.key === kindPath[1])?.rows || [];
  }, [byKind, kindPath]);

  const handleExport = () => {
    downloadCsv(
      `cluster-gpus-${dayjs().format('YYYYMMDD-HHmmss')}.csv`,
      GPU_CSV_HEADERS,
      exportGpuRowsCsv(filtered),
    );
  };

  const renderCardGrid = (rows: GpuRow[]) => (
    <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
      {rows.map((row) => (
        <GpuMatrixCard key={row.id} row={row} />
      ))}
    </div>
  );

  const renderPickGrid = (
    items: { key: string; label?: string; count: number }[],
    onPick: (key: string) => void,
  ) => (
    <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
      {items.map((item) => (
        <button
          key={item.key}
          type="button"
          onClick={() => onPick(item.key)}
          className="rounded-md border border-border/40 bg-card px-3 py-2 text-left hover:bg-background-muted"
        >
          <div className="text-sm font-medium">{item.label || item.key}</div>
          <div className="text-xs text-muted">
            {l('monitor.cluster.gpuView.cards', { n: item.count })}
          </div>
        </button>
      ))}
    </div>
  );

  const renderKindView = () => {
    if (kindPath.length === 0) {
      return renderPickGrid(
        byKind.map((g) => ({
          key: g.key,
          label: displayAccelKind(g.key),
          count: g.rows.length,
        })),
        (key) => setKindPath([key]),
      );
    }
    if (kindPath.length === 1) {
      const names = groupGpuRowsByName(kindLevelRows);
      return (
        <div className="space-y-2">
          <Button type="link" size="small" className="px-0" onClick={() => setKindPath([])}>
            {l('monitor.cluster.gpuView.backKinds')}
          </Button>
          {renderPickGrid(
            names.map((g) => ({ key: g.key, label: g.key, count: g.rows.length })),
            (key) => setKindPath([kindPath[0], key]),
          )}
        </div>
      );
    }
    return (
      <div className="space-y-2">
        <Button type="link" size="small" className="px-0" onClick={() => setKindPath([kindPath[0]])}>
          {displayAccelKind(kindPath[0])} / {kindPath[1]}
        </Button>
        {renderCardGrid(kindLevelRows)}
      </div>
    );
  };

  const renderModelView = () => {
    if (!byModel.length) {
      return (
        <div className="py-10 text-center text-sm text-muted">
          {l('monitor.cluster.gpuView.emptyModel')}
        </div>
      );
    }
    return (
      <Collapse
        destroyOnHidden
        items={byModel.map((group) => ({
          key: group.key,
          label: (
            <span className="text-sm">
              <button
                type="button"
                className="font-medium text-primary hover:underline"
                onClick={(e) => {
                  e.stopPropagation();
                  history.push(`/monitor/instances?id=${encodeURIComponent(group.key)}`);
                }}
              >
                {group.label}
              </button>
              <span className="ml-2 text-xs text-muted">
                {group.rows.length} {l('monitor.platform.gpuStatus', 'GPU')}
              </span>
            </span>
          ),
          children: renderCardGrid(group.rows),
        }))}
      />
    );
  };

  const renderNodeView = () => (
    <>
      <Collapse
        destroyOnHidden
        activeKey={activeKeys}
        onChange={(keys) => setActiveKeys(keys as string[])}
        items={pagedNodes.map((node) => ({
          key: node.key,
          label: (
            <span className="text-sm">
              <span className="font-medium">{node.label}</span>
              <span className="ml-2 text-xs text-muted">
                {node.rows.length} {l('monitor.platform.gpuStatus', 'GPU')}
                {node.avgUtil !== null ? ` · util ${node.avgUtil}%` : ''} · mem {node.avgMem}%
              </span>
              {node.hasAlert && (
                <span className="ml-2 text-xs text-warning">
                  {l('monitor.platform.filter.warn')}
                </span>
              )}
            </span>
          ),
          children: activeKeys.includes(node.key) ? renderCardGrid(node.rows) : null,
        }))}
      />
      {byNode.length > PAGE_SIZE_NODES && (
        <div className="mt-4 flex justify-end">
          <Pagination
            current={nodePage}
            pageSize={PAGE_SIZE_NODES}
            total={byNode.length}
            onChange={setNodePage}
            showSizeChanger={false}
            size="small"
          />
        </div>
      )}
    </>
  );

  return (
    <div className="space-y-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex flex-wrap gap-1" role="group" aria-label="GPU status filter">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              onClick={() => {
                onFilterChange(f.key);
                setNodePage(1);
              }}
              className={cn(
                'rounded border px-2 py-0.5 text-[11px] font-medium transition-colors',
                filter === f.key
                  ? 'border-primary bg-primary text-white'
                  : f.key === 'warn'
                    ? 'border-warning/30 bg-warning/10 text-warning'
                    : 'border-[color:var(--c-border-light)] bg-[var(--c-surface)] text-muted hover:bg-background-muted',
              )}
            >
              {l(f.labelKey, f.fallback)}
              {f.key === 'all'
                ? ` ${counts.total}`
                : f.key === 'ok'
                  ? ` ${counts.ok}`
                  : f.key === 'warn'
                    ? ` ${counts.warn}`
                    : f.key === 'idle'
                      ? ` ${counts.idle}`
                      : ` ${counts.offline}`}
            </button>
          ))}
        </div>
        <span className="text-[11px] text-disabled">
          {filtered.length}/{counts.total} · {byNode.length}{' '}
          {l('monitor.deviceInfo.nodes')}
        </span>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <Button size="small" icon={<Download size={14} />} onClick={handleExport}>
            {l('monitor.platform.downloadCsv')}
          </Button>
          {toolbarExtra}
        </div>
      </div>

      <ProCard
        size="small"
        title={l('monitor.platform.gpuMatrix')}
        loading={loading}
        bodyStyle={{ padding: 12 }}
        headStyle={{ paddingBlock: 8 }}
        extra={
          <span className="text-[11px] font-normal text-muted">
            {l('monitor.cluster.gpu.legend')}
          </span>
        }
      >
        {filtered.length === 0 ? (
          <div className="py-10 text-center text-sm text-muted">
            {filter === 'offline'
              ? l('monitor.cluster.gpuView.emptyOffline',
                )
              : l('global.data.empty')}
          </div>
        ) : groupMode === 'kind' ? (
          renderKindView()
        ) : groupMode === 'model' ? (
          renderModelView()
        ) : (
          renderNodeView()
        )}
      </ProCard>
    </div>
  );
};

export default GpuBoard;
