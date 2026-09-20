import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
  LineChart,
  Line,
  Legend,
} from 'recharts';
import { BarChart3, PieChartIcon, TrendingUp, Inbox } from 'lucide-react';
import { Card } from 'antd';
import { FC, useMemo } from 'react';
import { maxBy, size, sumBy } from 'lodash';
import { l } from '@/utils/intl';
import { calculatePercentage } from '@/utils';
import type { OverviewResponse } from '@/types/Public/data';
import dayjs from 'dayjs';
import { CHART_COLORS, TOOLTIP_STYLE } from '@/constants';

/** 副本 uid 形如 `{model_uid}-{n}`；前缀为已知主实例（或同批数据中的另一 model）时视为副本 */
const isReplicaModelName = (
  model: string,
  primaryUids: Set<string>,
  siblingModels?: Set<string>,
) => {
  const m = model.match(/^(.*)-(\d+)$/);
  if (!m) return false;
  const base = m[1];
  if (primaryUids.size > 0) return primaryUids.has(base);
  return !!siblingModels?.has(base);
};

const Empty = () => (
  <div className="h-full flex flex-col items-center justify-center text-muted">
    <Inbox className="h-10 w-10 mb-3 opacity-50" strokeWidth={0.8} />
    <p className="text-sm">{l('global.data.empty')}</p>
  </div>
);
interface UsageRankingChartPorps {
  dataSource: OverviewResponse['data_source'];
}
const UsageRankingChart: FC<UsageRankingChartPorps> = ({ dataSource }) => {
  const maxCountTraces = useMemo(
    () => maxBy(dataSource, 'count_traces')?.count_traces || 0,
    [dataSource],
  );
  return (
    <Card
      title={
        <div className="flex gap-2 items-center cursor-pointer">
          <BarChart3 size={16} className="text-muted" />
          {l('dashboard.usageRanking')}
        </div>
      }
      classNames={{ header: '!border-b-0 !p-6 !pb-3', body: '!pt-0' }}
    >
      <div className="h-[200px]">
        {size(dataSource) ? (
          <ResponsiveContainer width="100%" height="100%">
            <BarChart
              data={dataSource}
              layout="vertical"
              margin={{ top: 5, right: 20, left: 70, bottom: 5 }}
            >
              <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" horizontal={false} />
              <XAxis
                type="number"
                stroke="hsl(var(--text-muted))"
                fontSize={11}
                tickLine={false}
                axisLine={false}
                // 防止图表出现 0.5的刻度
                ticks={maxCountTraces <= 4 ? [0, 1, 2, 3, 4] : undefined}
                // 最大值 大于 3500 进行刻度转换
                tickFormatter={(value) =>
                  maxCountTraces >= 3500 ? `${(value / 1000).toFixed(0)}k` : value
                }
              />
              <YAxis
                dataKey="model"
                type="category"
                stroke="hsl(var(--text-muted))"
                fontSize={11}
                tickLine={false}
                axisLine={false}
                interval="preserveStartEnd" // 保留首尾标签
                allowDuplicatedCategory={false}
              />
              <Tooltip
                contentStyle={TOOLTIP_STYLE}
                formatter={(value) => [
                  (value as number).toLocaleString(),
                  l('dashboard.usageRanking.callCount'),
                ]}
              />
              <Bar dataKey="count_traces" radius={[0, 4, 4, 0]}>
                {dataSource.map((_, index) => (
                  <Cell key={`cell-${index}`} fill={CHART_COLORS[index % CHART_COLORS.length]} />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        ) : (
          <Empty />
        )}
      </div>
    </Card>
  );
};
interface CallDistributionChartPorps {
  dataSource: OverviewResponse['data_source'];
}
const CallDistributionChart: FC<CallDistributionChartPorps> = ({ dataSource }) => {
  const totalCalls = useMemo(() => sumBy(dataSource || [], 'count_traces'), [dataSource]);
  const data = useMemo(() => {
    return (dataSource || []).map((item) => ({
      ...item,
      value: calculatePercentage(item.count_traces, totalCalls),
    }));
  }, [dataSource, totalCalls]);

  return (
    <Card
      title={
        <div className="flex gap-2 items-center cursor-pointer">
          <PieChartIcon size={16} className="text-muted" />
          {l('dashboard.callDistribution')}
        </div>
      }
      classNames={{ header: '!border-b-0 !p-6 !pb-3', body: '!pt-0' }}
    >
      <div className="h-[200px]">
        {size(data) ? (
          <ResponsiveContainer width="100%" height="100%">
            <PieChart>
              <Pie
                data={data}
                cx="50%"
                cy="45%"
                innerRadius={35}
                outerRadius={60}
                paddingAngle={3}
                dataKey="value"
                nameKey="model"
                label={({ percent }) => `${((percent ?? 0) * 100).toFixed(0)}%`}
                labelLine={false}
                fontSize={10}
                isAnimationActive={false}
              >
                {data.map((_, index) => (
                  <Cell
                    key={`cell-${index}`}
                    fill={CHART_COLORS[index % CHART_COLORS.length]}
                    stroke="none"
                  />
                ))}
              </Pie>
              <Tooltip
                contentStyle={TOOLTIP_STYLE}
                formatter={(value, item) => [`${value}%`, item]}
              />
              <Legend
                verticalAlign="bottom"
                height={36}
                wrapperStyle={{
                  fontSize: 10,
                  overflowY: 'auto',
                  // scrollbarWidth: 'thin',
                }}
                formatter={(value) => <span className="text-muted">{value}</span>}
              />
            </PieChart>
          </ResponsiveContainer>
        ) : (
          <Empty />
        )}
      </div>
    </Card>
  );
};
interface TokenTrendChartProps {
  dataSource: OverviewResponse['tokens_usage'];
}
const TokenTrendChart: FC<TokenTrendChartProps> = ({ dataSource }) => {
  const { needTransform, lines, data } = useMemo(() => {
    // 过滤掉 model 为null 的数据
    const data = (dataSource || []).filter((item) => item.model);

    const grouped: Record<
      string,
      {
        date: string;
        [modelName: string]: number | string;
      }
    > = data.reduce((acc, item) => {
      const { date, model, total_usage } = item;
      if (!acc[date]) {
        acc[date] = { date };
      }
      acc[date][model] = total_usage;
      return acc;
    }, {} as Record<string, { date: string; [modelName: string]: number | string }>);
    return {
      lines: [...new Set(data.map((d) => d.model))],
      // 是否需要刻度转化
      needTransform: (maxBy(data, 'total_usage')?.total_usage || 0) >= 3500,
      data: Object.values(grouped).sort((a, b) => a.date.localeCompare(b.date)),
    };
    // 是否需要刻度转换
  }, [dataSource]);
  return (
    <Card
      title={
        <div className="flex gap-2 items-center cursor-pointer">
          <TrendingUp size={16} className="text-muted" />
          {l('dashboard.tokenTrend')}
        </div>
      }
      classNames={{ header: '!border-b-0 !p-6 !pb-3', body: '!pt-0' }}
    >
      <div className="h-[200px]">
        {size(data) ? (
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={data} margin={{ top: 5, right: 10, left: -10, bottom: 5 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
              <XAxis
                dataKey="date"
                stroke="hsl(var(--text-muted))"
                fontSize={11}
                tickLine={false}
                axisLine={false}
                interval="preserveStartEnd"
                tickFormatter={(value) => {
                  return dayjs(value).format('MM-DD');
                }}
              />
              <YAxis
                stroke="hsl(var(--text-muted))"
                fontSize={11}
                tickLine={false}
                axisLine={false}
                tickFormatter={(value) => (needTransform ? `${(value / 1000).toFixed(0)}k` : value)}
              />
              <Tooltip
                contentStyle={TOOLTIP_STYLE}
                formatter={(value, item) => {
                  return [(value as number).toLocaleString(), item];
                }}
              />
              <Legend
                wrapperStyle={{
                  fontSize: 10,
                  overflowY: 'auto',
                  // scrollbarWidth: 'thin',
                  maxHeight: 36,
                }}
              />
              {lines.map((model, index) => (
                <Line
                  key={model}
                  type="monotone"
                  dataKey={model}
                  name={model}
                  stroke={CHART_COLORS[index % CHART_COLORS.length]}
                  strokeWidth={2}
                  dot={false}
                  isAnimationActive={false}
                />
              ))}
            </LineChart>
          </ResponsiveContainer>
        ) : (
          <Empty />
        )}
      </div>
    </Card>
  );
};

interface ModelUsageChartProps {
  modelCallsData: OverviewResponse['data_source'];
  tokenTrendData: OverviewResponse['tokens_usage'];
  /** 当前运行中的模型实例 uid（不含副本）；用于图表只展示主实例 */
  modelUids?: string[];
}
const ModelUsageChart: FC<ModelUsageChartProps> = ({
  modelCallsData,
  tokenTrendData,
  modelUids = [],
}) => {
  const primaryUids = useMemo(
    () => new Set((modelUids || []).filter(Boolean)),
    [modelUids],
  );

  const newModelCallsData = useMemo(() => {
    const siblings = new Set(
      (modelCallsData || []).map((i) => i.model).filter(Boolean) as string[],
    );
    return (modelCallsData || []).filter(
      (item) =>
        !!item.model && !isReplicaModelName(item.model, primaryUids, siblings),
    );
  }, [modelCallsData, primaryUids]);

  const primaryTokenTrendData = useMemo(() => {
    const siblings = new Set(
      (tokenTrendData || []).map((i) => i.model).filter(Boolean) as string[],
    );
    return (tokenTrendData || []).filter(
      (item) =>
        !!item.model && !isReplicaModelName(item.model, primaryUids, siblings),
    );
  }, [tokenTrendData, primaryUids]);

  return (
    <div className="grid gap-6 lg:grid-cols-3 mt-6">
      <UsageRankingChart dataSource={newModelCallsData} />
      <CallDistributionChart dataSource={newModelCallsData} />
      <TokenTrendChart dataSource={primaryTokenTrendData} />
    </div>
  );
};
export default ModelUsageChart;
