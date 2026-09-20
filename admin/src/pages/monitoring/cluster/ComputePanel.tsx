import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useRequest } from 'ahooks';
import { Alert, Segmented, Switch } from 'antd';
import { ProCard } from '@ant-design/pro-components';
import { size } from 'lodash';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import {
  EventStreamController,
  getEventStreamFetcher,
} from '@/utils/eventStream';
import { usePageVisible } from '@/hooks/usePageVisible';
import {
  displayAccelKind,
  mapClusterGpuApiRows,
  type GpuRow,
  type GpuViewMode,
} from '@/utils/monitorMetrics';
import type { NodeInfoSource } from '@/types/Public/data';
import GpuBoard, { getDefaultNodeKeys } from '../components/GpuBoard';
import { MonitorIcons } from '../icons';

type FilterKey = 'all' | 'ok' | 'warn' | 'idle' | 'offline';

type ClusterSummary = {
  nodes_online?: number;
  nodes_total?: number;
  gpus_total?: number;
  gpus_online?: number;
  alloc_ratio?: number;
  accelerator_breakdown?: Record<string, number>;
};

type LiveSnapshot = {
  summary?: ClusterSummary;
  gpus?: Array<Record<string, unknown>>;
  nodes?: NodeInfoSource[];
};

/** 集群监控 · 节点：SSE 推送摘要 + Supervisor + 显卡矩阵 */
const ComputePanel = () => {
  const Crown = MonitorIcons.haPrimary;
  const Shield = MonitorIcons.shield;
  const pageVisible = usePageVisible();
  const [live, setLive] = useState(true);
  const [filter, setFilter] = useState<FilterKey>('all');
  const [gpuView, setGpuView] = useState<GpuViewMode>('kind');
  const [activeNodeKeys, setActiveNodeKeys] = useState<string[] | undefined>();
  const [liveSnapshot, setLiveSnapshot] = useState<LiveSnapshot | null>(null);
  const streamCtrlRef = useRef<EventStreamController | null>(null);
  const reconnectTimerRef = useRef<number | undefined>();

  /** 关闭实时时兜底：一次性拉取 */
  const { data: gpusHttp, loading: gpusLoading } = useRequest(
    async () => {
      const all: Array<Record<string, unknown>> = [];
      let offset = 0;
      const limit = 200;
      for (let i = 0; i < 3; i += 1) {
        const res = await request<{
          data: {
            data: {
              next_offset: number | null;
              results: Array<Record<string, unknown>>;
            };
          };
        }>('/monitor/cluster/gpus', { params: { offset, limit } });
        const page = res?.data?.data;
        const results = page?.results || [];
        all.push(...results);
        if (page?.next_offset == null) break;
        offset = page.next_offset;
      }
      return all;
    },
    { ready: !live || !pageVisible },
  );

  const { data: summaryHttp } = useRequest(
    () => request<{ data: { data: ClusterSummary } }>('/monitor/cluster/summary'),
    { ready: !live || !pageVisible, onError: () => undefined },
  );

  const { data: clusterHttp } = useRequest(
    () =>
      request<{ data: { results: NodeInfoSource[] } }>('/cluster/info', {
        params: ALL_LIST_PAGES_PARAMS,
      }),
    { ready: !live || !pageVisible },
  );

  const { data: transferStatusRes } = useRequest(() => request('/query_transfer_status'), {
    onError: () => undefined,
  });
  const { has_transfered = false, transfer_time = '' } =
    transferStatusRes?.data?.data || {};

  useEffect(() => {
    let cancelled = false;
    let generation = 0;

    const clearReconnect = () => {
      if (reconnectTimerRef.current != null) {
        window.clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = undefined;
      }
    };

    const stopStream = () => {
      clearReconnect();
      streamCtrlRef.current?.terminate();
      streamCtrlRef.current = null;
    };

    if (!live || !pageVisible) {
      stopStream();
      return stopStream;
    }

    const connect = () => {
      if (cancelled) return;
      clearReconnect();
      const myGen = ++generation;
      streamCtrlRef.current?.terminate();
      const ctrl = new EventStreamController();
      streamCtrlRef.current = ctrl;
      void getEventStreamFetcher<LiveSnapshot & { error?: string }>(
        {
          url: '/monitor/cluster/live/stream',
          params: { interval: 5 },
          options: {
            onData: (data) => {
              if (cancelled || myGen !== generation || !data) return;
              if (!data.summary && !data.gpus && !data.nodes) return;
              setLiveSnapshot((prev) => ({
                summary: data.summary ?? prev?.summary,
                gpus: data.gpus ?? prev?.gpus,
                nodes: data.nodes ?? prev?.nodes,
              }));
            },
            onError: () => undefined,
            onEnd: () => {
              if (cancelled || myGen !== generation) return;
              reconnectTimerRef.current = window.setTimeout(connect, 2_000);
            },
          },
        },
        ctrl,
      );
    };

    connect();
    return () => {
      cancelled = true;
      generation += 1;
      stopStream();
    };
  }, [live, pageVisible]);

  const summary = liveSnapshot?.summary || summaryHttp?.data?.data;
  const gpuRaw = live
    ? liveSnapshot?.gpus || []
    : gpusHttp || [];
  const nodeResults = live
    ? liveSnapshot?.nodes || []
    : clusterHttp?.data?.results || [];

  const supervisors = useMemo(() => {
    if (!size(nodeResults)) return [];
    return nodeResults
      .filter((item) => item.node_type === 'Supervisor')
      .slice()
      .sort((a, b) => Number(b.is_primary === true) - Number(a.is_primary === true));
  }, [nodeResults]);

  const gpuRows = useMemo(() => mapClusterGpuApiRows(gpuRaw), [gpuRaw]);
  const loading = live ? !liveSnapshot?.gpus : gpusLoading;

  useEffect(() => {
    if (activeNodeKeys !== undefined || !gpuRows.length) return;
    const byNode = new Map<string, GpuRow[]>();
    for (const row of gpuRows) {
      const key = row.workerAddress || row.node;
      if (!byNode.has(key)) byNode.set(key, []);
      byNode.get(key)!.push(row);
    }
    const nodes = [...byNode.entries()].map(([key, rows]) => ({
      key,
      hasAlert: rows.some(
        (r) => r.status === 'warn' || r.status === 'crit' || r.status === 'offline',
      ),
      rows,
    }));
    setActiveNodeKeys(getDefaultNodeKeys(nodes));
  }, [gpuRows, activeNodeKeys]);

  const strip = useMemo(() => {
    const temps = gpuRows.map((r) => r.temperatureC).filter((t): t is number => t != null);
    const powers = gpuRows.map((r) => r.powerW).filter((p): p is number => p != null);
    const utils = gpuRows.map((r) => r.util).filter((u): u is number => u != null);
    const memUsed = gpuRows.reduce((s, r) => s + r.memUsedGb, 0);
    const memTotal = gpuRows.reduce((s, r) => s + r.memTotalGb, 0);
    const busy = gpuRows.filter(
      (r) => r.status !== 'idle' && r.status !== 'offline',
    ).length;
    return {
      idle: gpuRows.filter((r) => r.status === 'idle').length,
      ok: gpuRows.filter((r) => r.status === 'ok').length,
      warn: gpuRows.filter((r) => r.status === 'warn' || r.status === 'crit').length,
      offline: gpuRows.filter((r) => r.status === 'offline').length,
      busy,
      avgUtil: utils.length
        ? Math.round(utils.reduce((s, u) => s + u, 0) / utils.length)
        : null,
      avgMem: gpuRows.length
        ? Math.round(gpuRows.reduce((s, r) => s + r.memUsage, 0) / gpuRows.length)
        : 0,
      avgTemp: temps.length
        ? Math.round(temps.reduce((s, t) => s + t, 0) / temps.length)
        : null,
      powerSum: powers.length ? Math.round(powers.reduce((s, p) => s + p, 0)) : null,
      memUsed: Math.round(memUsed * 100) / 100,
      memTotal: Math.round(memTotal * 100) / 100,
      occupied: gpuRows.filter((r) => r.occupant?.model_uid).length,
    };
  }, [gpuRows]);

  const metricChip = (label: string, value: ReactNode, title?: string) => (
    <div
      key={label}
      title={title}
      className="rounded-md border border-border/40 bg-card px-2.5 py-1.5"
    >
      <div className="text-[10px] text-muted">{label}</div>
      <div className="font-mono text-sm font-semibold tabular-nums text-default">{value}</div>
    </div>
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex justify-end">
        <span className="inline-flex items-center gap-1.5 text-xs text-muted">
          <Switch size="small" checked={live} onChange={setLive} />
          {l('monitor.cluster.live')}
        </span>
      </div>

      {has_transfered ? (
        <Alert
          banner
          message={l('global.message.supervisorChangeMsg', undefined, { time: transfer_time })}
          type="warning"
          showIcon
        />
      ) : null}

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5">
        {metricChip(
          l('monitor.deviceInfo.nodes'),
          `${summary?.nodes_online ?? '—'}/${summary?.nodes_total ?? '—'}`,
          l('monitor.cluster.tip.nodes',
          ),
        )}
        {metricChip(
          l('monitor.cluster.overview.gpusOnline'),
          `${summary?.gpus_online ?? gpuRows.length}/${summary?.gpus_total ?? gpuRows.length}`,
          l('monitor.cluster.tip.gpusOnline',
          ),
        )}
        {metricChip(
          l('monitor.cluster.overview.allocRatio'),
          summary?.alloc_ratio != null ? `${summary.alloc_ratio}%` : '—',
          l('monitor.cluster.tip.allocRatio',
          ),
        )}
        {metricChip(
          l('monitor.cluster.overview.gpuIdleBusy'),
          `${strip.busy} / ${gpuRows.length}`,
        )}
        {metricChip(
          l('monitor.cluster.compute.strip.status'),
          `${strip.idle}/${strip.ok}/${strip.warn}/${strip.offline}`,
          'idle / ok / warn / offline',
        )}
        {metricChip(
          'Util',
          strip.avgUtil != null ? `${strip.avgUtil}%` : '—',
          l('monitor.cluster.tip.util',
          ),
        )}
        {metricChip(
          'Mem',
          `${strip.memUsed}/${strip.memTotal}G (${strip.avgMem}%)`,
        )}
        {metricChip(
          l('monitor.cluster.overview.avgTemp'),
          strip.avgTemp != null ? `${strip.avgTemp}°C` : '—',
        )}
        {metricChip(
          l('monitor.cluster.overview.powerSum'),
          strip.powerSum != null ? `${strip.powerSum}W` : '—',
        )}
        {metricChip(
          l('monitor.cluster.compute.strip.occupied'),
          strip.occupied,
          l('monitor.cluster.tip.occupied',
          ),
        )}
      </div>

      {!!size(supervisors) && (
        <div>
          <h3 className="mb-2 text-xs font-medium text-muted">Supervisor</h3>
          <div className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">
            {supervisors.map((sv) => (
              <ProCard
                key={sv.ip_address}
                size="small"
                bodyStyle={{ padding: '10px 12px' }}
                className={sv.is_primary ? 'ring-1 ring-primary/40' : undefined}
              >
                <div className="flex items-center gap-2">
                  {sv.is_primary === false ? (
                    <div className="flex h-7 w-7 items-center justify-center rounded-md bg-background">
                      <Shield className="h-3.5 w-3.5 text-muted" />
                    </div>
                  ) : (
                    <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary/10">
                      <Crown className="h-3.5 w-3.5 text-primary" />
                    </div>
                  )}
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5 text-sm font-medium">
                      <span className="truncate">{sv.ip_address}</span>
                      {sv.is_primary !== false && (
                        <span className="shrink-0 rounded bg-primary/10 px-1 py-px text-[10px] text-primary">
                          {l('monitor.deviceInfo.supervisorPrimaryBadge')}
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-muted">
                      {sv.is_primary === false
                        ? l('monitor.deviceInfo.supervisorSecondary')
                        : l('monitor.deviceInfo.supervisorPrimary')}
                    </div>
                  </div>
                </div>
              </ProCard>
            ))}
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Segmented
          size="small"
          value={gpuView}
          onChange={(v) => setGpuView(v as GpuViewMode)}
          options={[
            { label: l('monitor.cluster.gpuView.kind'), value: 'kind' },
            { label: l('monitor.cluster.gpuView.node'), value: 'node' },
            { label: l('monitor.cluster.gpuView.model'), value: 'model' },
          ]}
        />
        {summary?.accelerator_breakdown && gpuView === 'kind' ? (
          <span className="text-xs text-muted">
            {Object.entries(summary.accelerator_breakdown)
              .map(([k, n]) => `${displayAccelKind(k)} ${n}`)
              .join(' · ')}
          </span>
        ) : null}
      </div>
      <GpuBoard
        gpuRows={gpuRows}
        loading={loading}
        filter={filter}
        onFilterChange={setFilter}
        activeNodeKeys={activeNodeKeys}
        onActiveNodeKeysChange={setActiveNodeKeys}
        groupMode={gpuView}
      />
    </div>
  );
};

export default ComputePanel;
