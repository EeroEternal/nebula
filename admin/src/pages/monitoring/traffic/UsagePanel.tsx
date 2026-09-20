import { Collapse, DatePicker, Progress, Button } from 'antd';
import dayjs from 'dayjs';
import { ProCard, ProTable } from '@ant-design/pro-components';
import { ProColumns } from '@ant-design/pro-components';
import { useRequest } from 'ahooks';
import { RefreshCw, Zap, TrendingUp, Clock, Inbox, Repeat2 } from 'lucide-react';
import {
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
  ComposedChart,
  Bar,
  Line,
} from 'recharts';

import request from '@/utils/request';
import { useMemo, useState } from 'react';
import { DATE_FORMAT, TOOLTIP_STYLE, CHART_COLORS } from '@/constants';
import { l } from '@/utils/intl';
import { convertDateToUTC, calculatePercentage } from '@/utils';
import { IconButton } from '@/components';
import MetricLabel, {
  normalizeInstanceModel,
} from '../components/MetricLabel';

const { RangePicker } = DatePicker;
type DateRangeValue = [dayjs.Dayjs, dayjs.Dayjs];

interface MetricsResponse {
  data: {
    countTraces: number;
    date: string;
    usage: {
      countObservations: number;
      countTraces: number;
      inputUsage: number;
      model: string;
      outputUsage: number;
      totalCost: number;
      totalUsage: number;
    }[];
  }[];
  meta: {
    page: number;
    limit: number;
    totalItems: number;
    totalPages: number;
  };
}

const Empty = () => (
  <div className="h-full flex flex-col items-center justify-center text-muted">
    <Inbox className="h-10 w-10 mb-3 opacity-50" strokeWidth={0.8} />
    <p className="text-sm">{l('global.data.empty')}</p>
  </div>
);

/** 对话监控 · 总览（仅作面板，不单独路由） */
type BreakdownRow = {
  key?: unknown;
  dimValue?: string;
  rowId?: string;
  requests?: number;
  [key: string]: unknown;
};

const mapBreakdownEntities = (raw: unknown[]) =>
  (Array.isArray(raw) ? raw : []).map((e, idx) => {
    const rec = e && typeof e === 'object' ? (e as BreakdownRow) : {};
    const rawKey = rec.key != null ? String(rec.key) : '';
    const empty = !rawKey || rawKey === '(empty)';
    return {
      ...rec,
      rowId: `${rawKey || 'empty'}-${idx}`,
      // 避免 ProTable dataIndex=key 与行 key 冲突导致首列空白
      dimValue: empty
        ? String(l('monitor.traffic.breakdown.empty'))
        : rawKey,
    };
  });

const UsagePanel = () => {
  const [timeStamp, setTimeStamp] = useState<DateRangeValue>([
    dayjs().subtract(1, 'month'),
    dayjs(),
  ]);
  const [isModelTracesSum, setModelTracesSum] = useState(true);
  const {
    run: getOverview,
    data: overviewData,
    loading: overviewLoading,
  } = useRequest(
    () =>
      request('/l/metric/overview', {
        params: {
          fromTimestamp: convertDateToUTC(`${dayjs(timeStamp[0]).format(DATE_FORMAT)} 00:00:00`),
          toTimestamp: convertDateToUTC(`${dayjs(timeStamp[1]).format(DATE_FORMAT)} 23:59:59`),
        },
      }),
    { refreshDeps: [timeStamp] },
  );
  const { avg_latency = 0, p99_latency = 0, p95_latency = 0 } = overviewData?.data?.data || {};

  const {
    data: metricsResponse,
    loading: dailyLoading,
    run: getDaily,
  } = useRequest(
    () =>
      request<{ data: { data: { data_source: MetricsResponse } } }>('/l/metric/daily', {
        params: {
          fromTimestamp: convertDateToUTC(`${dayjs(timeStamp[0]).format(DATE_FORMAT)} 00:00:00`),
          toTimestamp: convertDateToUTC(`${dayjs(timeStamp[1]).format(DATE_FORMAT)} 23:59:59`),
        },
      }),
    {
      refreshDeps: [timeStamp],
    },
  );

  const fromTs = convertDateToUTC(`${dayjs(timeStamp[0]).format(DATE_FORMAT)} 00:00:00`);
  const toTs = convertDateToUTC(`${dayjs(timeStamp[1]).format(DATE_FORMAT)} 23:59:59`);

  const { data: userBreakdownRes, loading: userBdLoading } = useRequest(
    () =>
      request('/monitor/usage/breakdown', {
        params: { fromTimestamp: fromTs, toTimestamp: toTs, dim: 'user' },
      }),
    { refreshDeps: [timeStamp] },
  );
  const { data: ipBreakdownRes, loading: ipBdLoading } = useRequest(
    () =>
      request('/monitor/usage/breakdown', {
        params: { fromTimestamp: fromTs, toTimestamp: toTs, dim: 'sourceIp' },
      }),
    { refreshDeps: [timeStamp] },
  );
  // umi 拦截后：{ data: { code, message, data: { entities } } }
  const userEntities = mapBreakdownEntities(
    userBreakdownRes?.data?.data?.entities ||
      userBreakdownRes?.data?.entities ||
      [],
  );
  const ipEntities = mapBreakdownEntities(
    ipBreakdownRes?.data?.data?.entities ||
      ipBreakdownRes?.data?.entities ||
      [],
  );

  const breakdownColumns = (dimTitle: string): ProColumns<BreakdownRow>[] => [
    {
      title: dimTitle,
      dataIndex: 'dimValue',
      ellipsis: true,
      width: 200,
      render: (_, r) => (
        <span className="font-mono text-xs break-all">{r.dimValue}</span>
      ),
    },
    {
      title: l('monitor.modelsUsage.tableCard.modelCall'),
      dataIndex: 'requests',
      width: 100,
      sorter: (a, b) => (a.requests || 0) - (b.requests || 0),
    },
    {
      title: l('monitor.modelsUsage.tableCard.inputTokens'),
      dataIndex: 'input_tokens',
      width: 120,
      render: (_, r) => (r.input_tokens ?? 0).toLocaleString(),
    },
    {
      title: l('monitor.modelsUsage.tableCard.outputTokens'),
      dataIndex: 'output_tokens',
      width: 120,
      render: (_, r) => (r.output_tokens ?? 0).toLocaleString(),
    },
    {
      title: l('monitor.traffic.breakdown.avgLatency'),
      dataIndex: 'duration_avg',
      width: 110,
      render: (_, r) =>
        r.duration_avg != null ? `${Number(r.duration_avg).toFixed(2)}s` : '—',
    },
  ];
  const {
    countTraces,
    totalInputUsage,
    totalOutputUsage,
    maxCountTraces,
    tableDataSource,
    comboChatList,
    modelsName,
    tokensNeedTransform,
    maxDailyTokens,
  } = useMemo(() => {
    const data = metricsResponse?.data?.data?.data_source?.data || [];

    const modelStats: Record<
      string,
      { c: number; i: number; o: number; replicaTags: Set<string> }
    > = {};
    const dateTraceMap: Record<string, number> = {}; // date → total countTraces
    const dateModelTraceMap: Record<string, Record<string, number>> = {};
    const dateTokenMap: Record<string, Record<string, number>> = {};

    let totalCount = 0;
    let totalInput = 0;
    let totalOutput = 0;
    let globalMax = 0;
    let needTransform = false;
    const modelSet = new Set<string>();

    for (const { date, countTraces: dailyTotal, usage = [] } of data) {
      totalCount += dailyTotal;
      dateTraceMap[date] = dailyTotal;
      for (const { model, countTraces, inputUsage, outputUsage, totalUsage } of usage) {
        if (!model) continue;
        // xtrace 写入的是副本级 uid（如 qwen3.5-0）；总览按逻辑实例聚合
        const logical = normalizeInstanceModel(model);
        modelSet.add(logical);
        totalInput += inputUsage;
        totalOutput += outputUsage;
        if (countTraces > globalMax) globalMax = countTraces;
        if (totalUsage >= 3500) needTransform = true;

        modelStats[logical] ||= { c: 0, i: 0, o: 0, replicaTags: new Set() };
        modelStats[logical].c += countTraces;
        modelStats[logical].i += inputUsage;
        modelStats[logical].o += outputUsage;
        modelStats[logical].replicaTags.add(model);

        dateModelTraceMap[date] ||= {};
        dateTokenMap[date] ||= {};
        dateModelTraceMap[date][logical] =
          (dateModelTraceMap[date][logical] || 0) + countTraces;
        dateTokenMap[date][logical] = (dateTokenMap[date][logical] || 0) + totalUsage;
        dateTokenMap[date][`${logical}_input`] =
          (dateTokenMap[date][`${logical}_input`] || 0) + inputUsage;
        dateTokenMap[date][`${logical}_output`] =
          (dateTokenMap[date][`${logical}_output`] || 0) + outputUsage;
      }
    }
    // 按日期排序（YYYY-MM-DD）
    const sortedDates = Object.keys(dateTraceMap).sort();
    const modelsArr = Array.from(modelSet);
    let maxTokens = 0;
    const comboChatList = sortedDates.map((date) => {
      const tokenRow = dateTokenMap[date] || {};
      let tokens = 0;
      for (const m of modelsArr) tokens += Number(tokenRow[m] || 0);
      if (tokens > maxTokens) maxTokens = tokens;
      return {
        date,
        calls: dateTraceMap[date] || 0,
        tokens,
        // 模型名 → 当日调用次数（与 tokens 总量分轴；勿混入 per-model token）
        ...(dateModelTraceMap[date] || {}),
      };
    });

    return {
      countTraces: totalCount,
      totalInputUsage: totalInput,
      totalOutputUsage: totalOutput,
      maxCountTraces: globalMax,
      maxDailyTokens: maxTokens,
      tokensNeedTransform: needTransform,
      modelsName: modelsArr,
      comboChatList,
      tableDataSource: Object.entries(modelStats).map(([model, s]) => ({
        model,
        countTraces: s.c,
        inputUsage: s.i,
        outputUsage: s.o,
        replicaTagCount: s.replicaTags.size,
      })),
    };
  }, [metricsResponse?.data]);
  type UsageTableRow = {
    model: string;
    countTraces: number;
    inputUsage: number;
    outputUsage: number;
    replicaTagCount: number;
  };
  const columns: ProColumns<UsageTableRow>[] = [
    {
      title: l('monitor.modelsUsage.tableCard.modelName'),
      dataIndex: 'model',
      ellipsis: {
        showTitle: true,
      },
      render: (_, row) => (
        <div className="min-w-0">
          <div className="truncate">{row.model}</div>
          {row.replicaTagCount > 1 ? (
            <div className="text-[11px] text-muted truncate">
              {l('monitor.traffic.model.replicaTags', {
                n: row.replicaTagCount,
              })}
            </div>
          ) : null}
        </div>
      ),
    },
    {
      title: l('monitor.modelsUsage.tableCard.modelCall'),
      dataIndex: 'countTraces',
      align: 'center',
      render: (count) => {
        return (
          <div className="flex justify-center items-center gap-2">
            <Progress
              type="circle"
              percent={calculatePercentage(count as number, countTraces)}
              strokeColor={window.THEME_PRIMARY_COLOR}
              size={20}
            />
            {`${calculatePercentage(count as number, countTraces)}% (${count?.toLocaleString()})`}
          </div>
        );
      },
    },
    {
      title: l('monitor.modelsUsage.tableCard.tokenTotal'),
      dataIndex: 'tokenTotal',
      align: 'center',
      render: (_, record) => (record.inputUsage + record.outputUsage).toLocaleString(),
    },
    {
      title: (
        <>
          {l('monitor.modelsUsage.tableCard.inputTokens')} /{' '}
          {l('monitor.modelsUsage.tableCard.outputTokens')}
        </>
      ),
      dataIndex: 'inputTokens',
      align: 'right',
      render: (_, record) => (
        <>
          {record.inputUsage.toLocaleString()} / {record.outputUsage.toLocaleString()}
        </>
      ),
    },
  ];
  const handleRefresh = () => {
    getOverview();
    getDaily();
  };

  const handleExchange = () => {
    setModelTracesSum(!isModelTracesSum);
  };

  const extraContent = (
    <div className="flex gap-2 items-center">
      <RangePicker
        size="small"
        value={timeStamp}
        format={DATE_FORMAT}
        onChange={(data) => setTimeStamp(data as DateRangeValue)}
        allowClear={false}
      />
      <Button
        size="small"
        icon={<RefreshCw size={14} />}
        loading={overviewLoading || dailyLoading}
        onClick={handleRefresh}
      >
        {l('global.actions.refresh')}
      </Button>
    </div>
  );

  return (
      <div className="flex flex-col gap-3">
        <div className="flex justify-end">{extraContent}</div>
        <div className="grid grid-cols-2 lg:grid-cols-6 gap-2">
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted flex items-center gap-1 mb-0.5">
              <Zap size={12} />
              {l('monitor.modelsUsage.countTracesTotal')}
            </div>
            <div className="text-xl font-semibold tabular-nums">
              {countTraces.toLocaleString()}
            </div>
          </div>
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted flex items-center gap-1 mb-0.5">
              <TrendingUp size={12} />
              {l('monitor.modelsUsage.tokensTotal')}
            </div>
            <div className="text-xl font-semibold tabular-nums">
              {(totalInputUsage + totalOutputUsage).toLocaleString()}
            </div>
            <div className="text-[11px] text-muted truncate">
              {totalInputUsage.toLocaleString()} / {totalOutputUsage.toLocaleString()}
            </div>
          </div>
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted flex items-center gap-1 mb-0.5">
              <Clock size={12} />
              <MetricLabel zh={l('monitor.traces.avgLatency')} en="Avg latency" />
            </div>
            <div className="text-xl font-semibold tabular-nums">{avg_latency}s</div>
          </div>
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted mb-0.5">
              <MetricLabel zh="时延 P95" en="P95" />
            </div>
            <div className="text-xl font-semibold tabular-nums">{p95_latency}s</div>
          </div>
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted mb-0.5">
              <MetricLabel zh="时延 P99" en="P99" />
            </div>
            <div className="text-xl font-semibold tabular-nums">{p99_latency}s</div>
          </div>
          <div className="rounded-lg bg-card text-default border border-border/50 px-3 py-2.5">
            <div className="text-xs text-muted mb-0.5">
              {l('monitor.modelsUsage.tableCard.extra', {
                value: tableDataSource.length,
              })}
            </div>
            <div className="text-xl font-semibold tabular-nums">
              {tableDataSource.length}
            </div>
          </div>
        </div>
        <ProCard
          size="small"
          title={l('monitor.modelsUsage.trendTitle')}
          extra={
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
              <span>
                {l('monitor.modelsUsage.trend.comboHint')}
              </span>
              <IconButton
                className="!w-7 !h-7 hover:text-primary hover:bg-primary/15"
                onClick={handleExchange}
                title={
                  isModelTracesSum
                    ? l('monitor.modelsUsage.modelsTracesTrends')
                    : l('monitor.modelsUsage.countTracesTotal')
                }
              >
                <Repeat2 size={16} />
              </IconButton>
            </div>
          }
          className="h-[320px]"
          bodyStyle={{ paddingTop: 8, paddingBottom: 8 }}
        >
          {comboChatList.length ? (
            <ResponsiveContainer width="100%" height="100%">
              <ComposedChart
                data={comboChatList}
                margin={{ top: 8, right: 8, left: 0, bottom: 0 }}
              >
                <CartesianGrid
                  strokeDasharray="3 3"
                  stroke="hsl(var(--border))"
                  vertical={false}
                />
                <XAxis
                  dataKey="date"
                  stroke="hsl(var(--text-muted))"
                  fontSize={11}
                  tickLine={false}
                  axisLine={false}
                  interval="preserveStartEnd"
                  tickFormatter={(value) => dayjs(value).format('MM-DD')}
                />
                <YAxis
                  yAxisId="left"
                  stroke={CHART_COLORS[0]}
                  fontSize={11}
                  tickLine={false}
                  axisLine={false}
                  ticks={maxCountTraces <= 4 ? [0, 1, 2, 3, 4] : undefined}
                  tickFormatter={(value) =>
                    maxCountTraces >= 3500 ? `${(value / 1000).toFixed(0)}k` : value
                  }
                />
                <YAxis
                  yAxisId="right"
                  orientation="right"
                  stroke={CHART_COLORS[1] || '#ef4444'}
                  fontSize={11}
                  tickLine={false}
                  axisLine={false}
                  tickFormatter={(value) =>
                    maxDailyTokens >= 3500 || tokensNeedTransform
                      ? `${(value / 1000).toFixed(0)}k`
                      : value
                  }
                />
                <Tooltip
                  contentStyle={TOOLTIP_STYLE}
                  formatter={(value, name) => [
                    (value as number).toLocaleString(),
                    name,
                  ]}
                  labelFormatter={(label) => dayjs(label).format('YYYY-MM-DD')}
                />
                <Legend
                  wrapperStyle={{
                    fontSize: 10,
                    overflowY: 'auto',
                    maxHeight: 36,
                  }}
                />
                {isModelTracesSum ? (
                  <Bar
                    yAxisId="left"
                    dataKey="calls"
                    name={String(l('monitor.modelsUsage.trend.calls'))}
                    fill={CHART_COLORS[0]}
                    radius={[4, 4, 0, 0]}
                    barSize={28}
                  />
                ) : (
                  modelsName.map((model, idx) => (
                    <Bar
                      key={model}
                      yAxisId="left"
                      dataKey={model}
                      name={model}
                      stackId="calls"
                      fill={CHART_COLORS[idx % CHART_COLORS.length]}
                      radius={
                        idx === modelsName.length - 1 ? [4, 4, 0, 0] : [0, 0, 0, 0]
                      }
                    />
                  ))
                )}
                <Line
                  yAxisId="right"
                  type="monotone"
                  dataKey="tokens"
                  name={String(l('monitor.modelsUsage.trend.tokens'))}
                  stroke={CHART_COLORS[1] || '#ef4444'}
                  strokeWidth={2}
                  dot={{
                    r: 3,
                    fill: CHART_COLORS[1] || '#ef4444',
                    strokeWidth: 0,
                  }}
                  activeDot={{ r: 5 }}
                  isAnimationActive={false}
                />
              </ComposedChart>
            </ResponsiveContainer>
          ) : (
            <Empty />
          )}
        </ProCard>

        <div className="flex flex-col gap-2">
          <ProCard
            size="small"
            title={l('monitor.modelsUsage.tableCard.title')}
            extra={
              <span className="text-muted text-xs">
                {l('monitor.modelsUsage.tableCard.extra', '0', {
                  value: tableDataSource.length,
                })}
              </span>
            }
            headerBordered
            className="overflow-hidden"
            bodyStyle={{ padding: 0 }}
            headStyle={{ paddingInline: 12, minHeight: 40 }}
          >
            <ProTable
              options={false}
              search={false}
              size="small"
              rowKey="model"
              dataSource={tableDataSource}
              columns={columns}
              pagination={false}
              scroll={{ x: 720 }}
            />
          </ProCard>

          <ProCard
            size="small"
            title={l('monitor.traffic.breakdown.title')}
            headerBordered
            className="overflow-hidden"
            bodyStyle={{ padding: '0 8px 8px' }}
            headStyle={{ paddingInline: 12, minHeight: 40 }}
          >
            <Collapse
              size="small"
              className="bg-transparent border-0"
              defaultActiveKey={['user', 'ip']}
              items={[
                {
                  key: 'user',
                  label: l('monitor.traffic.breakdown.byUser'),
                  children: (
                    <ProTable
                      options={false}
                      search={false}
                      loading={userBdLoading}
                      rowKey="rowId"
                      dataSource={userEntities}
                      columns={breakdownColumns(
                        String(l('monitor.traffic.breakdown.user')),
                      )}
                      pagination={{ pageSize: 8, hideOnSinglePage: true, size: 'small' }}
                      size="small"
                      scroll={{ x: 720 }}
                    />
                  ),
                },
                {
                  key: 'ip',
                  label: l('monitor.traffic.breakdown.byIp'),
                  children: (
                    <ProTable
                      options={false}
                      search={false}
                      loading={ipBdLoading}
                      rowKey="rowId"
                      dataSource={ipEntities}
                      columns={breakdownColumns(
                        String(l('monitor.traffic.breakdown.sourceIp')),
                      )}
                      pagination={{ pageSize: 8, hideOnSinglePage: true, size: 'small' }}
                      size="small"
                      scroll={{ x: 720 }}
                    />
                  ),
                },
              ]}
            />
          </ProCard>
        </div>
      </div>
  );
};

export default UsagePanel;
