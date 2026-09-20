import { useMemo, type ReactNode } from 'react';
import { useRequest } from 'ahooks';
import { usePageVisible } from '@/hooks/usePageVisible';
import { Alert, Button, Empty, Spin, Table, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip as RTooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import dayjs from 'dayjs';
import { RefreshCw } from 'lucide-react';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { matchGpuUtil } from '@/utils/monitorMetrics';
import MetricLabel from '../../components/MetricLabel';
import type { DeviceInfo } from '@/types/Public/data';
import { mergePerfPayload } from './mergePerfTrends';

type ReplicaRow = {
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
};

type PerfPayload = {
  model_uid: string;
  instance?: Record<string, number | null | undefined>;
  replicas?: ReplicaRow[];
  trends?: {
    ttft_p95_ms?: { ts: number; value: number }[];
    qps?: { ts: number; value: number }[];
    error_rate?: { ts: number; value: number }[];
  };
  notes?: string[];
};

const fmt = (n?: number | null, d = 2) =>
  n == null || Number.isNaN(Number(n)) ? '—' : Number(n).toFixed(d);

type Props = {
  modelUid: string;
  /** 空 / null = 实例聚合指标 */
  replicaUid?: string | null;
  devices?: DeviceInfo[];
  /** false=不轮询折线；默认页可见 15s 拉 trends */
  poll?: boolean;
  /** true=抽屉紧凑；false/默认=总览大屏 */
  compact?: boolean;
  /** 查询窗口秒数，默认 1h；历史均值用 24h */
  windowSeconds?: number;
  /** 自定义标题（如历史均值） */
  title?: string;
  /** 父级 snapshot 卡片；与 HTTP trends 合并 */
  perfOverride?: PerfPayload | null;
  onRefreshCards?: () => void;
  /** 标题栏「刷新」右侧额外操作（如压测入口） */
  headerExtra?: ReactNode;
};

/** 线上性能：聚合 API / SSE + 副本表 + 趋势；可按副本筛选卡片 */
const PerformanceMetrics = ({
  modelUid,
  replicaUid,
  devices = [],
  poll = true,
  compact = false,
  windowSeconds = 3600,
  title,
  perfOverride,
  onRefreshCards,
  headerExtra,
}: Props) => {
  const useOverride = perfOverride != null;
  const pageVisible = usePageVisible();
  const useCardsView = compact || windowSeconds >= 2 * 86400;
  const { data, loading, refresh } = useRequest(
    () =>
      request<{ data: PerfPayload }>(
        `/monitor/models/${encodeURIComponent(modelUid)}/performance`,
        {
          params: {
            from: Math.floor(Date.now() / 1000) - windowSeconds,
            to: Math.floor(Date.now() / 1000),
            view: useCardsView ? 'cards' : useOverride ? 'trends' : 'full',
          },
        },
      ),
    {
      ready: !!modelUid && pageVisible && (useCardsView || !useOverride || !compact),
      refreshDeps: [modelUid, windowSeconds, useCardsView, useOverride],
      pollingInterval:
        poll && pageVisible && !useCardsView ? 15_000 : undefined,
      onError: () => undefined,
    },
  );

  const firstLoad = loading && !data && !useOverride;

  const httpPerf = (data?.data?.data || data?.data || {}) as PerfPayload;
  const perf: PerfPayload = (
    useOverride
      ? mergePerfPayload(httpPerf, perfOverride as PerfPayload)
      : httpPerf
  ) as PerfPayload;
  const inst = perf.instance || {};
  const replicas = perf.replicas || [];
  const replicaRow = replicaUid
    ? replicas.find((r) => r.replica_model_uid === replicaUid)
    : undefined;

  const cards = useMemo(() => {
    if (replicaRow) {
      return {
        ttft_p95_ms: replicaRow.ttft_p95_ms ?? null,
        ttft_p99_ms: null as number | null,
        e2e_p95_s: replicaRow.e2e_p95_s ?? null,
        e2e_p99_s: null as number | null,
        qps: replicaRow.qps ?? null,
        rpm: null as number | null,
        tpm: null as number | null,
        inference_failure_rate: replicaRow.inference_failure_rate ?? null,
      };
    }
    return {
      ttft_p95_ms: (inst.ttft_p95_ms as number | null | undefined) ?? null,
      ttft_p99_ms: (inst.ttft_p99_ms as number | null | undefined) ?? null,
      e2e_p95_s: (inst.e2e_p95_s as number | null | undefined) ?? null,
      e2e_p99_s: (inst.e2e_p99_s as number | null | undefined) ?? null,
      qps: (inst.qps as number | null | undefined) ?? null,
      rpm: (inst.rpm as number | null | undefined) ?? null,
      tpm: (inst.tpm as number | null | undefined) ?? null,
      inference_failure_rate:
        (inst.inference_failure_rate as number | null | undefined) ?? null,
    };
  }, [inst, replicaRow]);

  const tableData = replicaUid
    ? replicas.filter((r) => r.replica_model_uid === replicaUid)
    : replicas;

  const chartData = useMemo(() => {
    const byTs = new Map<number, Record<string, number>>();
    const add = (key: string, points?: { ts: number; value: number }[]) => {
      (points || []).forEach((p) => {
        const row = byTs.get(p.ts) || { ts: p.ts };
        row[key] = p.value;
        byTs.set(p.ts, row);
      });
    };
    add('ttft', perf.trends?.ttft_p95_ms);
    add('qps', perf.trends?.qps);
    add('errors', perf.trends?.error_rate);
    return Array.from(byTs.values())
      .sort((a, b) => Number(a.ts) - Number(b.ts))
      .map((r) => ({
        ...r,
        time: dayjs(Number(r.ts) * 1000).format('HH:mm'),
      }));
  }, [perf.trends]);

  const columns: ColumnsType<ReplicaRow> = [
    {
      title: l('monitor.instances.replicaUid'),
      dataIndex: 'replica_model_uid',
      ellipsis: true,
    },
    {
      title: l('monitor.instances.replicaStatus'),
      dataIndex: 'replica_status',
      width: 100,
      render: (v) => v || '—',
    },
    {
      title: <MetricLabel zh="首Token时延 P95 (ms)" en="TTFT P95" />,
      dataIndex: 'ttft_p95_ms',
      width: 130,
      render: (v) => fmt(v, 1),
    },
    {
      title: <MetricLabel zh="端到端时延 P95 (s)" en="E2E P95" />,
      dataIndex: 'e2e_p95_s',
      width: 120,
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
      width: 72,
      render: (v: number | undefined) => Math.max(0, Number(v) || 0),
    },
    {
      title: l('monitor.instances.active'),
      dataIndex: 'active_requests',
      width: 72,
      render: (v: number | undefined) => Math.max(0, Number(v) || 0),
    },
    {
      title: l('monitor.instances.vramRatio'),
      key: 'vram',
      width: 90,
      render: (_, row) => {
        const hint = matchGpuUtil(
          devices,
          row.worker_address,
          row.gpu_idx as number[] | undefined,
        );
        return hint == null ? '—' : `${hint.usage}%`;
      },
    },
  ];

  const valueClass = compact
    ? 'text-xl font-semibold tabular-nums'
    : 'text-2xl font-semibold tabular-nums tracking-tight';
  const chartH = compact ? 'h-56' : 'h-80';

  return (
    <div
      className={
        compact
          ? 'space-y-4'
          : 'space-y-4 rounded-lg border border-[color:var(--c-border-light)] bg-card p-4'
      }
    >
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-medium">
          {title || l('monitor.instances.perf.title')}
          {replicaUid ? (
            <span className="ml-1.5 text-xs font-normal text-muted">
              · {replicaUid}
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <Button
            size="small"
            icon={<RefreshCw size={14} />}
            onClick={() => {
              if (onRefreshCards) onRefreshCards();
              refresh();
            }}
          >
            {l('global.actions.refresh')}
          </Button>
          {headerExtra}
        </div>
      </div>

      <Spin spinning={firstLoad}>
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
              <div className={valueClass}>
                {value}
                {unit ? (
                  <span className="text-xs text-muted font-normal ml-1">{unit}</span>
                ) : null}
              </div>
            </div>
          ))}
        </div>

        {/* 查询抽屉 compact：只保留上方指标卡，不展示说明/趋势/副本表 */}
        {!compact ? (
          <>
            <Alert
              className="mt-3"
              type="info"
              showIcon
              message={l('monitor.instances.perf.httpNote',
              )}
            />

            {replicaUid ? (
              <Alert
                className="mt-2"
                type="warning"
                showIcon
                message={l('monitor.instances.perf.replicaTrendNote',
                )}
              />
            ) : null}

            {chartData.length ? (
              <div className={`${chartH} mt-4`}>
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
                className="mt-4"
                description={l('monitor.instances.perf.noTrend',
                )}
              />
            )}

            <Table<ReplicaRow>
              className="mt-4"
              size="small"
              rowKey="replica_model_uid"
              columns={columns}
              dataSource={tableData}
              pagination={false}
              locale={{
                emptyText: l('monitor.instances.perf.noReplica'),
              }}
            />

            {(perf.notes || []).length > 0 && (
              <Tooltip title={(perf.notes || []).join('\n')}>
                <div className="text-xs text-muted mt-2 cursor-help">
                  {l('monitor.instances.perf.notes')}
                </div>
              </Tooltip>
            )}
          </>
        ) : null}
      </Spin>
    </div>
  );
};

export default PerformanceMetrics;
