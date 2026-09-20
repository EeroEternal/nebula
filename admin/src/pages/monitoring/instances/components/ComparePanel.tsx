import { useEffect, useMemo, useState } from 'react';
import { useRequest } from 'ahooks';
import { Alert, Button, Select, Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { formatDisplayTime } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import type { ModelsInstancesListItem } from '@/types/Public/data';
import MetricLabel from '../../components/MetricLabel';

const fmt = (n?: number | null, d = 2) =>
  n == null || Number.isNaN(Number(n)) ? '—' : Number(n).toFixed(d);

type MetricBlock = {
  mean?: number;
  p50?: number;
  p95?: number;
  p99?: number;
};

type BenchTask = {
  id: number;
  model_uid: string;
  status: string;
  created_at?: number;
  finished_at?: number | null;
  report?: {
    ttft_ms?: MetricBlock;
    tpot_ms?: MetricBlock;
    e2el_ms?: MetricBlock;
    output_token_throughput?: number;
    request_throughput?: number;
    throughput_rps?: number;
    success?: number;
    failed?: number;
  } | null;
};

type SideMetrics = {
  taskId?: number;
  finishedAt?: number | null;
  ttftP95?: number | null;
  ttftP99?: number | null;
  e2eP95?: number | null;
  e2eP99?: number | null;
  rps?: number | null;
  tokPerS?: number | null;
  failRate?: number | null;
  missing?: boolean;
};

type Props = {
  defaultA?: string;
  /** 抽屉内嵌时隐藏标题区 */
  hideTitle?: boolean;
};

function pickLatestSucceeded(tasks: BenchTask[]): BenchTask | undefined {
  return tasks.find((t) => t.status === 'succeeded' && t.report);
}

function toSide(task?: BenchTask): SideMetrics {
  if (!task?.report) return { missing: true };
  const r = task.report;
  const success = Number(r.success || 0);
  const failed = Number(r.failed || 0);
  const total = success + failed;
  return {
    taskId: task.id,
    finishedAt: task.finished_at,
    ttftP95: r.ttft_ms?.p95 ?? null,
    ttftP99: r.ttft_ms?.p99 ?? null,
    e2eP95: r.e2el_ms?.p95 != null ? r.e2el_ms.p95 / 1000 : null,
    e2eP99: r.e2el_ms?.p99 != null ? r.e2el_ms.p99 / 1000 : null,
    rps: r.request_throughput ?? r.throughput_rps ?? null,
    tokPerS: r.output_token_throughput ?? null,
    failRate: total > 0 ? failed / total : null,
    missing: false,
  };
}

async function fetchBenchSide(modelUid: string): Promise<SideMetrics> {
  const res = await request<{ data: { data?: BenchTask[]; code?: number } | BenchTask[] }>(
    '/benchmarks',
    { params: { model_uid: modelUid } },
  );
  const list = (res?.data?.data || res?.data || []) as BenchTask[];
  const tasks = Array.isArray(list) ? list : [];
  return toSide(pickLatestSucceeded(tasks));
}

/** 两实例对比：取各自最近一次「就绪基线压测」报告（非线上实时指标） */
const ComparePanel = ({ defaultA, hideTitle }: Props) => {
  const [a, setA] = useState<string | undefined>(defaultA);
  const [b, setB] = useState<string | undefined>();

  useEffect(() => {
    if (defaultA) setA(defaultA);
  }, [defaultA]);

  const { data: listRes } = useRequest(() =>
    request<{ data: { count?: number; results?: ModelsInstancesListItem[] } }>(
      '/models/instances',
      {
        params: ALL_LIST_PAGES_PARAMS,
      },
    ),
  );
  const listRaw = Array.isArray(listRes?.data?.results)
    ? listRes.data.results
    : Array.isArray(listRes?.data)
      ? (listRes.data as ModelsInstancesListItem[])
      : [];
  const options = listRaw.map((i) => ({
    value: i.model_uid,
    label: `${i.model_uid} (${i.model_name || i.status})`,
  }));

  const {
    data: left,
    loading: loadingA,
    refresh: refreshA,
  } = useRequest(() => fetchBenchSide(a!), {
    ready: !!a,
    refreshDeps: [a],
  });

  const {
    data: right,
    loading: loadingB,
    refresh: refreshB,
  } = useRequest(() => fetchBenchSide(b!), {
    ready: !!b,
    refreshDeps: [b],
  });

  const loading = loadingA || loadingB;
  const leftSide = left || {};
  const rightSide = right || {};
  const hasAny = Boolean(a || b);

  const metaHint = useMemo(() => {
    if (!hasAny) return null;
    const fmtTask = (side: SideMetrics, uid?: string) => {
      if (!uid) return null;
      if (side.missing) {
        return l('monitor.instances.compare.noBench', {
          uid,
        });
      }
      const when = side.finishedAt
        ? formatDisplayTime(side.finishedAt, 'YYYY-MM-DD HH:mm')
        : '—';
      return l('monitor.instances.compare.benchMeta', {
        uid,
        id: side.taskId,
        time: when,
      });
    };
    const parts = [fmtTask(leftSide, a), fmtTask(rightSide, b)].filter(Boolean);
    return parts.length ? parts.join(' ｜ ') : null;
  }, [hasAny, leftSide, rightSide, a, b]);

  const rows = [
    {
      key: 'ttft_p95',
      label: <MetricLabel zh="首Token P95" en="TTFT P95 (ms)" />,
      a: leftSide.ttftP95,
      b: rightSide.ttftP95,
    },
    {
      key: 'ttft_p99',
      label: <MetricLabel zh="首Token P99" en="TTFT P99 (ms)" />,
      a: leftSide.ttftP99,
      b: rightSide.ttftP99,
    },
    {
      key: 'e2e_p95',
      label: <MetricLabel zh="端到端 P95" en="E2E P95 (s)" />,
      a: leftSide.e2eP95,
      b: rightSide.e2eP95,
    },
    {
      key: 'e2e_p99',
      label: <MetricLabel zh="端到端 P99" en="E2E P99 (s)" />,
      a: leftSide.e2eP99,
      b: rightSide.e2eP99,
    },
    {
      key: 'rps',
      label: <MetricLabel zh="QPS" en="QPS" />,
      a: leftSide.rps,
      b: rightSide.rps,
    },
    {
      key: 'toks',
      label: <MetricLabel zh="输出吞吐" en="Output tok/s" />,
      a: leftSide.tokPerS,
      b: rightSide.tokPerS,
    },
    {
      key: 'fail',
      label: (
        <MetricLabel
          zh={l('monitor.instances.failRate')}
          en="Fail rate"
        />
      ),
      a: leftSide.failRate,
      b: rightSide.failRate,
      digits: 4,
    },
  ];

  const columns: ColumnsType<(typeof rows)[0]> = [
    {
      title: l('monitor.instances.compare.metric'),
      dataIndex: 'label',
      width: 140,
    },
    {
      title: a || 'A',
      dataIndex: 'a',
      render: (v, row) => fmt(v as number, row.digits ?? 2),
    },
    {
      title: b || 'B',
      dataIndex: 'b',
      render: (v, row) => fmt(v as number, row.digits ?? 2),
    },
  ];

  const showNeedBench =
    (a && leftSide.missing) || (b && rightSide.missing);

  return (
    <div className="space-y-3">
      {hideTitle ? null : (
        <div>
          <div className="text-sm font-medium">
            {l('monitor.instances.compare.title')}
          </div>
          <div className="text-xs text-muted mt-0.5">
            {l('monitor.instances.compare.hint',
            )}
          </div>
        </div>
      )}
      {hideTitle ? (
        <div className="text-xs text-muted">
          {l('monitor.instances.compare.hint',
          )}
        </div>
      ) : null}
      <div className="flex flex-wrap gap-2 items-center">
        <Select
          allowClear
          showSearch
          optionFilterProp="label"
          className="min-w-[220px]"
          placeholder="A"
          options={options}
          value={a}
          onChange={(v) => setA(v)}
          notFoundContent={l('global.noData')}
        />
        <Select
          allowClear
          showSearch
          optionFilterProp="label"
          className="min-w-[220px]"
          placeholder="B"
          options={options}
          value={b}
          onChange={(v) => setB(v)}
          notFoundContent={l('global.noData')}
        />
        <Button
          type="primary"
          loading={loading}
          disabled={!hasAny}
          onClick={() => {
            if (a) refreshA();
            if (b) refreshB();
          }}
        >
          {l('monitor.instances.compare.run')}
        </Button>
      </div>
      {metaHint ? (
        <Alert type="info" showIcon banner className="!mb-0" message={metaHint} />
      ) : null}
      {showNeedBench ? (
        <Alert
          type="warning"
          showIcon
          banner
          className="!mb-0"
          message={l('monitor.instances.compare.needBench',
          )}
        />
      ) : null}
      <Table
        size="small"
        rowKey="key"
        loading={loading}
        columns={columns}
        dataSource={rows}
        pagination={false}
      />
    </div>
  );
};

export default ComparePanel;
