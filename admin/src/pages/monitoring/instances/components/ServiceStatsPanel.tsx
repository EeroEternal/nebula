import { useMemo, type ReactNode } from 'react';
import { useRequest } from 'ahooks';
import { Spin } from 'antd';
import dayjs from 'dayjs';
import { ALL_LIST_PAGES_PARAMS, DATE_FORMAT } from '@/constants';
import { IntanceStatus } from '@/constants/intance';
import type { ModelsInstancesListItem } from '@/types/Public/data';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { convertDateToUTC } from '@/utils';
import MetricLabel from '../../components/MetricLabel';

/** 数据分析顶部：当前服务 / 近 24h 用量摘要 */
const ServiceStatsPanel = () => {
  const fromTs24h = convertDateToUTC(
    `${dayjs().subtract(1, 'day').format(DATE_FORMAT)} 00:00:00`,
  );
  const toTs = convertDateToUTC(`${dayjs().format(DATE_FORMAT)} 23:59:59`);

  const { data: listRes, loading: listLoading } = useRequest(() =>
    request<{ data: { count?: number; results?: ModelsInstancesListItem[] } }>(
      '/models/instances',
      { params: ALL_LIST_PAGES_PARAMS },
    ),
  );

  const { data: overviewRes, loading: usageLoading } = useRequest(
    () =>
      request('/l/metric/overview', {
        params: { fromTimestamp: fromTs24h, toTimestamp: toTs },
      }),
    { onError: () => undefined },
  );

  const { data: dailyRes, loading: dailyLoading } = useRequest(
    () =>
      request('/l/metric/daily', {
        params: { fromTimestamp: fromTs24h, toTimestamp: toTs },
      }),
    { onError: () => undefined },
  );

  const instances = useMemo(() => {
    const raw = Array.isArray(listRes?.data?.results)
      ? listRes!.data.results!
      : Array.isArray(listRes?.data)
        ? (listRes!.data as ModelsInstancesListItem[])
        : [];
    return raw;
  }, [listRes]);

  const ready = instances.filter((i) => i.status === IntanceStatus.READY).length;
  const errorList = instances.filter((i) => i.status === IntanceStatus.ERROR);
  const replicaSum = instances.reduce((s, i) => s + (Number(i.replica) || 0), 0);

  const latency = overviewRes?.data?.data || {};
  const avgLatency = latency.avg_latency ?? '—';
  const p95 = latency.p95_latency ?? '—';
  const p99 = latency.p99_latency ?? '—';

  const { calls24h, tokens24h } = useMemo(() => {
    const data = dailyRes?.data?.data?.data_source?.data || [];
    const sorted = [...data].sort((a, b) =>
      String(a.date || '').localeCompare(String(b.date || '')),
    );
    const last = sorted[sorted.length - 1];
    let tokens = 0;
    for (const u of last?.usage || []) {
      tokens += Number(u.totalUsage || 0);
    }
    return {
      calls24h: Number(last?.countTraces || 0),
      tokens24h: tokens,
    };
  }, [dailyRes]);

  const card = (title: string, children: ReactNode, className = '') => (
    <div className={`rounded-lg border border-border/50 bg-card p-3 ${className}`.trim()}>
      <div className="mb-2 text-sm font-medium text-default">{title}</div>
      {children}
    </div>
  );

  const metric = (label: ReactNode, value: ReactNode) => (
    <div className="flex flex-col gap-0.5 rounded-md bg-background-muted/40 px-2.5 py-2">
      <div className="text-[11px] text-muted">{label}</div>
      <div className="font-mono text-lg font-semibold tabular-nums text-default">{value}</div>
    </div>
  );

  return (
    <Spin spinning={listLoading || usageLoading || dailyLoading}>
      <div className="grid gap-3 lg:grid-cols-2">
        {card(
          l('monitor.instances.overview.current'),
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {metric(
              l('monitor.instances.overview.instances'),
              instances.length,
            )}
            {metric(l('monitor.instances.overview.ready'), ready)}
            {metric(l('monitor.instances.overview.error'), errorList.length)}
            {metric(l('monitor.instances.overview.replicas'), replicaSum)}
          </div>,
        )}

        {card(
          l('monitor.instances.overview.usage24h'),
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {metric(
              l('monitor.instances.overview.calls'),
              calls24h.toLocaleString(),
            )}
            {metric(
              l('monitor.instances.overview.tokens', 'Tokens'),
              tokens24h.toLocaleString(),
            )}
            {metric(
              <MetricLabel zh={l('monitor.traces.avgLatency')} en="Avg" />,
              typeof avgLatency === 'number' ? `${avgLatency}s` : avgLatency,
            )}
            {metric(
              <MetricLabel
                zh={l('monitor.instances.overview.p95Latency')}
                en="P95"
              />,
              typeof p95 === 'number' ? `${p95}s` : p95,
            )}
            {metric(
              <MetricLabel
                zh={l('monitor.instances.overview.p99Latency')}
                en="P99"
              />,
              typeof p99 === 'number' ? `${p99}s` : p99,
            )}
          </div>,
        )}
      </div>
    </Spin>
  );
};

export default ServiceStatsPanel;
