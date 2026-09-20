import { useMemo, type ReactNode } from 'react';
import { useRequest } from 'ahooks';
import { usePageVisible } from '@/hooks/usePageVisible';
import { Alert, Button, Empty, Progress, Spin, Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip as RTooltip,
  XAxis,
  YAxis,
} from 'recharts';
import dayjs from 'dayjs';
import { Activity, Gauge, RefreshCw } from 'lucide-react';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { matchGpuUtil } from '@/utils/monitorMetrics';
import type { DeviceInfo } from '@/types/Public/data';
import MetricLabel from '../../components/MetricLabel';

type RuntimePayload = {
  pending_requests?: number;
  active_requests?: number;
  replicas?: Array<{
    replica_model_uid?: string;
    worker_address?: string;
    gpu_idx?: number[];
    pending_requests?: number;
    active_requests?: number;
  }>;
};

type PerfPayload = {
  instance?: Record<string, number | null | undefined>;
  replicas?: Array<{
    replica_model_uid: string;
    worker_address?: string;
    gpu_idx?: number[];
    replica_status?: string;
    pending_requests?: number;
    active_requests?: number;
    ttft_p95_ms?: number | null;
    e2e_p95_s?: number | null;
    qps?: number | null;
    inference_failure_rate?: number | null;
  }>;
  trends?: {
    ttft_p95_ms?: { ts: number; value: number }[];
    qps?: { ts: number; value: number }[];
    error_rate?: { ts: number; value: number }[];
  };
};

type ReplicaRow = {
  key: string;
  model_uid: string;
  replica_model_uid: string;
  replica_status?: string;
  pending_requests?: number;
  active_requests?: number;
  ttft_p95_ms?: number | null;
  e2e_p95_s?: number | null;
  qps?: number | null;
  worker_address?: string;
  gpu_idx?: number[];
};

const fmt = (n?: number | null, d = 2) =>
  n == null || Number.isNaN(Number(n)) ? '—' : Number(n).toFixed(d);

const avg = (nums: number[]) =>
  nums.length ? nums.reduce((s, n) => s + n, 0) / nums.length : null;

type Props = {
  modelUids: string[];
  devices?: DeviceInfo[];
  /** 父级 snapshot 各实例 runtime */
  runtimesOverride?: Record<string, RuntimePayload> | null;
  /** 父级 snapshot 各实例线上性能卡片 */
  performancesOverride?: Record<string, PerfPayload> | null;
  onRefreshSnapshot?: () => void;
  /** 标题栏「刷新」右侧额外操作（如压测入口） */
  headerExtra?: ReactNode;
};

/** 全部模型综合：卡片来自 snapshot，折线 15s 拉一次 cluster trends */
const AllModelsLiveBoard = ({
  modelUids,
  devices = [],
  runtimesOverride,
  performancesOverride,
  onRefreshSnapshot,
  headerExtra,
}: Props) => {
  const pageVisible = usePageVisible();
  const useRuntimeOverride = runtimesOverride != null;
  const usePerfOverride = performancesOverride != null;

  const { data: trendsRes, loading: perfLoading, refresh: refreshPerf } = useRequest(
    () => {
      const now = Math.floor(Date.now() / 1000);
      return request<{ data: { trends?: PerfPayload['trends'] } }>(
        '/monitor/models/live/trends',
        { params: { from: now - 3600, to: now } },
      );
    },
    {
      ready: modelUids.length > 0 && pageVisible,
      pollingInterval: pageVisible ? 15_000 : undefined,
      onError: () => undefined,
    },
  );

  const clusterTrends = (
    (trendsRes?.data?.data || trendsRes?.data || {}) as { trends?: PerfPayload['trends'] }
  ).trends;

  const perfList = useMemo(() => {
    return modelUids.map((uid) => {
      const live = usePerfOverride
        ? ((performancesOverride?.[uid] || {}) as PerfPayload)
        : ({} as PerfPayload);
      return { uid, perf: live };
    });
  }, [usePerfOverride, performancesOverride, modelUids]);

  const load = useMemo(() => {
    let pending = 0;
    let active = 0;
    const utils: number[] = [];
    const rows = modelUids.map((uid) => ({
      uid,
      runtime: ((runtimesOverride?.[uid] || {}) as RuntimePayload),
    }));
    for (const row of rows) {
      const rt = row.runtime;
      pending += Math.max(0, Number(rt.pending_requests ?? 0) || 0);
      active += Math.max(0, Number(rt.active_requests ?? 0) || 0);
      for (const r of rt.replicas || []) {
        const hint = matchGpuUtil(
          devices,
          r.worker_address,
          r.gpu_idx as number[] | undefined,
        );
        if (hint?.util != null) utils.push(hint.util);
      }
    }
    return {
      pending,
      active,
      gpuUtilAvg: utils.length ? Math.round(avg(utils)!) : null,
    };
  }, [devices, runtimesOverride, modelUids]);

  const cards = useMemo(() => {
    const insts = (perfList || []).map((p) => p.perf.instance || {});
    const num = (key: string) =>
      insts
        .map((i) => i[key])
        .filter((v): v is number => v != null && !Number.isNaN(Number(v)))
        .map(Number);
    const sum = (arr: number[]) => (arr.length ? arr.reduce((s, n) => s + n, 0) : null);
    return {
      ttft_p95_ms: avg(num('ttft_p95_ms')),
      ttft_p99_ms: avg(num('ttft_p99_ms')),
      e2e_p95_s: avg(num('e2e_p95_s')),
      e2e_p99_s: avg(num('e2e_p99_s')),
      qps: sum(num('qps')),
      rpm: sum(num('rpm')),
      tpm: sum(num('tpm')),
      inference_failure_rate: sum(num('inference_failure_rate')),
    };
  }, [perfList]);

  const chartData = useMemo(() => {
    const byTs = new Map<number, { ts: number; qps?: number; ttft?: number; errors?: number }>();
    const add = (key: 'qps' | 'ttft' | 'errors', points?: { ts: number; value: number }[]) => {
      for (const p of points || []) {
        const cur = byTs.get(p.ts) || { ts: p.ts };
        if (key === 'qps') cur.qps = p.value;
        else if (key === 'errors') cur.errors = p.value;
        else cur.ttft = p.value;
        byTs.set(p.ts, cur);
      }
    };
    add('qps', clusterTrends?.qps);
    add('ttft', clusterTrends?.ttft_p95_ms);
    add('errors', clusterTrends?.error_rate);
    return [...byTs.values()]
      .sort((a, b) => a.ts - b.ts)
      .map((r) => ({
        time: dayjs(r.ts * 1000).format('HH:mm'),
        qps: r.qps,
        ttft: r.ttft,
        errors: r.errors,
      }));
  }, [clusterTrends]);

  const replicaRows: ReplicaRow[] = useMemo(() => {
    const rows: ReplicaRow[] = [];
    for (const item of perfList || []) {
      for (const r of item.perf.replicas || []) {
        rows.push({
          key: `${item.uid}:${r.replica_model_uid}`,
          model_uid: item.uid,
          replica_model_uid: r.replica_model_uid,
          replica_status: r.replica_status,
          pending_requests: r.pending_requests,
          active_requests: r.active_requests,
          ttft_p95_ms: r.ttft_p95_ms,
          e2e_p95_s: r.e2e_p95_s,
          qps: r.qps,
          worker_address: r.worker_address,
          gpu_idx: r.gpu_idx,
        });
      }
    }
    return rows;
  }, [perfList]);

  const columns: ColumnsType<ReplicaRow> = [
    {
      title: l('models.instances.modelUid'),
      dataIndex: 'model_uid',
      ellipsis: true,
      width: 120,
    },
    {
      title: l('monitor.instances.replicaUid'),
      dataIndex: 'replica_model_uid',
      ellipsis: true,
    },
    {
      title: l('monitor.instances.replicaStatus'),
      dataIndex: 'replica_status',
      width: 90,
      render: (v) => v || '—',
    },
    {
      title: <MetricLabel zh="首Token时延 P95" en="TTFT P95" />,
      dataIndex: 'ttft_p95_ms',
      width: 110,
      render: (v) => fmt(v, 1),
    },
    {
      title: <MetricLabel zh="端到端时延 P95" en="E2E P95" />,
      dataIndex: 'e2e_p95_s',
      width: 110,
      render: (v) => fmt(v, 3),
    },
    {
      title: <MetricLabel zh="每秒查询数" en="QPS" />,
      dataIndex: 'qps',
      width: 90,
      render: (v) => fmt(v, 3),
    },
    {
      title: l('monitor.instances.pending'),
      dataIndex: 'pending_requests',
      width: 64,
      render: (v: number | undefined) => Math.max(0, Number(v) || 0),
    },
    {
      title: l('monitor.instances.active'),
      dataIndex: 'active_requests',
      width: 72,
      render: (v: number | undefined) => Math.max(0, Number(v) || 0),
    },
    {
      title: l('monitor.instances.gpuUtilRatio'),
      key: 'gpu_util',
      width: 100,
      render: (_, row) => {
        const hint = matchGpuUtil(
          devices,
          row.worker_address,
          row.gpu_idx as number[] | undefined,
        );
        return hint?.util == null ? '—' : `${hint.util}%`;
      },
    },
  ];

  const loading = !useRuntimeOverride && !usePerfOverride && perfLoading;

  if (!modelUids.length) {
    return (
      <div className="py-8 text-center text-sm text-muted">
        {l('monitor.instances.overview.noInstance')}
      </div>
    );
  }

  return (
    <Spin spinning={loading}>
      <div className="space-y-3">
        <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card p-4 space-y-3">
          <div className="text-sm font-medium flex items-center gap-1.5">
            <Activity size={14} />
            {l('monitor.instances.resources')}
            <span className="text-xs font-normal text-muted">
              · {l('monitor.instances.filter.allModels')}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
            <div>
              <div className="text-xs text-muted">
                {l('monitor.instances.pending')}
              </div>
              <div className="text-3xl font-semibold tabular-nums tracking-tight">
                {load.pending}
              </div>
            </div>
            <div>
              <div className="text-xs text-muted">
                {l('monitor.instances.active')}
              </div>
              <div className="text-3xl font-semibold tabular-nums tracking-tight">
                {load.active}
              </div>
            </div>
            {load.gpuUtilAvg != null ? (
              <div className="col-span-2 sm:col-span-1">
                <div className="text-xs text-muted mb-1 flex items-center gap-1">
                  <Gauge size={12} />
                  {l('monitor.instances.gpuUtilRatio')}
                </div>
                <Progress percent={load.gpuUtilAvg} size="small" />
              </div>
            ) : null}
          </div>
        </div>

        <div className="space-y-4 rounded-lg border border-[color:var(--c-border-light)] bg-card p-4">
          <div className="flex items-center justify-between gap-2">
            <div className="text-sm font-medium">
              {l('monitor.instances.perf.titleAll')}
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <Button
                size="small"
                icon={<RefreshCw size={14} />}
                loading={perfLoading}
                onClick={() => {
                  if (onRefreshSnapshot) onRefreshSnapshot();
                  refreshPerf();
                }}
              >
                {l('global.actions.refresh')}
              </Button>
              {headerExtra}
            </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm">
            {(
              [
                {
                  key: 'ttft_p95',
                  label: <MetricLabel zh="首Token时延 P95" en="TTFT P95" />,
                  value: fmt(cards.ttft_p95_ms, 1),
                  unit: 'ms',
                },
                {
                  key: 'ttft_p99',
                  label: <MetricLabel zh="首Token时延 P99" en="TTFT P99" />,
                  value: fmt(cards.ttft_p99_ms, 1),
                  unit: 'ms',
                },
                {
                  key: 'e2e_p95',
                  label: <MetricLabel zh="端到端时延 P95" en="E2E P95" />,
                  value: fmt(cards.e2e_p95_s, 3),
                  unit: 's',
                },
                {
                  key: 'e2e_p99',
                  label: <MetricLabel zh="端到端时延 P99" en="E2E P99" />,
                  value: fmt(cards.e2e_p99_s, 3),
                  unit: 's',
                },
                {
                  key: 'qps',
                  label: <MetricLabel zh="每秒查询数" en="QPS" />,
                  value: fmt(cards.qps, 3),
                  unit: '',
                },
                {
                  key: 'rpm',
                  label: <MetricLabel zh="每分钟请求数" en="RPM" />,
                  value: fmt(cards.rpm, 1),
                  unit: '',
                },
                {
                  key: 'tpm',
                  label: <MetricLabel zh="每分钟Token数" en="TPM" />,
                  value: fmt(cards.tpm, 0),
                  unit: '',
                },
                {
                  key: 'fail',
                  label: (
                    <MetricLabel
                      zh={l('monitor.instances.failRate')}
                      en="Fail rate"
                    />
                  ),
                  value: fmt(cards.inference_failure_rate, 4),
                  unit: '/s',
                },
              ] as const
            ).map(({ key, label, value, unit }) => (
              <div
                key={key}
                className="rounded-lg border border-[color:var(--c-border-light)] p-3"
              >
                <div className="text-xs text-muted">{label}</div>
                <div className="text-2xl font-semibold tabular-nums tracking-tight">
                  {value}
                  {unit ? (
                    <span className="text-xs text-muted font-normal ml-1">{unit}</span>
                  ) : null}
                </div>
              </div>
            ))}
          </div>

          <Alert
            type="info"
            showIcon
            message={l('monitor.instances.perf.allModelsNote',
            )}
          />

          {chartData.length ? (
            <div className="h-80">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="time" tick={{ fontSize: 11 }} />
                  <YAxis tick={{ fontSize: 11 }} />
                  <RTooltip />
                  <Legend />
                  <Line
                    type="monotone"
                    dataKey="ttft"
                    name="首Token时延 P95"
                    stroke="#2563eb"
                    dot={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="qps"
                    name="每秒查询数"
                    stroke="#16a34a"
                    dot={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="errors"
                    name={l('monitor.instances.failRate')}
                    stroke="#dc2626"
                    dot={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          ) : (
            <Empty
              description={l('monitor.instances.perf.noTrend',
              )}
            />
          )}

          <Table<ReplicaRow>
            size="small"
            rowKey="key"
            columns={columns}
            dataSource={replicaRows}
            pagination={replicaRows.length > 20 ? { pageSize: 20 } : false}
            locale={{
              emptyText: l('monitor.instances.perf.noReplica'),
            }}
          />
        </div>
      </div>
    </Spin>
  );
};

export default AllModelsLiveBoard;
