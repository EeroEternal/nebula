import { useMemo, type FC } from 'react';
import { useRequest } from 'ahooks';
import { Inbox } from 'lucide-react';
import {
  ResponsiveContainer,
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  Legend,
} from 'recharts';
import { ProCard } from '@ant-design/pro-components';
import { CHART_COLORS, TOOLTIP_STYLE } from '@/constants';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { formatTime } from '@/utils/fomatData';
import { extractPromMatrixResult } from '@/utils/promResult';

export type DcgmTimePreset = '15m' | '1h' | '6h' | '24h';

export const DCGM_PRESET_MINUTES: Record<DcgmTimePreset, number> = {
  '15m': 15,
  '1h': 60,
  '6h': 360,
  '24h': 1440,
};

export const DCGM_PRESET_STEP: Record<DcgmTimePreset, string> = {
  '15m': '30s',
  '1h': '60s',
  '6h': '300s',
  '24h': '1800s',
};

const MAX_SERIES = 12;
/** 默认不轮询，避免图表整卡 loading 打断查看；需要定时刷新时再显式传 live */

type QuerySpec = { name?: string; promql: string };

type Props = {
  title: string;
  /** 单指标或同图多指标（如 PCIe TX/RX） */
  queries: QuerySpec[];
  unit?: string;
  preset: DcgmTimePreset;
  live?: boolean;
  yDomain?: [number | 'auto', number | 'auto'];
};

export function dcgmSeriesLabel(
  metric: Record<string, string>,
  queryName?: string,
): string {
  const gpu = metric.gpu ?? metric.device ?? '?';
  const base = `gpu-${gpu}`;
  return queryName ? `${queryName}(${base})` : base;
}

/** 单卡 DCGM 时序图：/p/query_range → Recharts；无序列 Empty */
const DcgmChartCard: FC<Props> = ({
  title,
  queries,
  unit = '',
  preset,
  live = false,
  yDomain,
}) => {
  const { data, loading } = useRequest(
    async () => {
      const minutes = DCGM_PRESET_MINUTES[preset];
      const endSec = Math.floor(Date.now() / 1000);
      const startSec = endSec - minutes * 60;
      const step = DCGM_PRESET_STEP[preset];
      const results = await Promise.all(
        queries.map(async (q) => {
          const res = await request(
            `/p/query_range?query=${encodeURIComponent(
              q.promql,
            )}&start=${startSec}&end=${endSec}&step=${step}`,
          );
          return { name: q.name, res };
        }),
      );
      return results;
    },
    {
      refreshDeps: [preset, queries.map((q) => q.promql).join('|')],
      // live 预留：当前集群页禁用轮询，避免整页/图表频繁刷新
      pollingInterval: live ? 60_000 : undefined,
    },
  );

  const { chartData, legendNames } = useMemo(() => {
    const byIdx = new Map<number, Record<string, number | string>>();
    const legend = new Set<string>();
    for (const item of data || []) {
      const rows = extractPromMatrixResult(item.res as { success?: boolean });
      for (const row of rows) {
        const metric = row.metric || {};
        const name = dcgmSeriesLabel(metric, item.name);
        legend.add(name);
        for (const [ts, val] of row.values || []) {
          const t = Number(ts);
          if (!byIdx.has(t)) byIdx.set(t, { time: formatTime(t), date: t });
          const raw = parseFloat(val);
          byIdx.get(t)![name] = Number.isFinite(raw) ? raw : 0;
        }
      }
    }
    return {
      chartData: [...byIdx.values()].sort((a, b) => Number(a.date) - Number(b.date)),
      legendNames: [...legend].slice(0, MAX_SERIES),
    };
  }, [data]);

  const firstLoad = loading && !chartData.length;
  const domain = yDomain ?? (['auto', 'auto'] as [number | 'auto', number | 'auto']);

  return (
    <ProCard size="small" title={title} loading={firstLoad} className="h-full">
      {!chartData.length ? (
        <div className="flex h-[180px] flex-col items-center justify-center text-muted">
          <Inbox className="mb-2 h-8 w-8 opacity-50" strokeWidth={0.8} />
          <p className="text-sm">
            {l('monitor.cluster.hardware.empty',
            )}
          </p>
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={200}>
          <LineChart data={chartData} margin={{ left: -10, right: 8, top: 8 }}>
            <XAxis
              dataKey="time"
              stroke="hsl(var(--text-muted))"
              fontSize={11}
              tickLine={false}
              axisLine={false}
              interval="preserveStartEnd"
            />
            <YAxis
              stroke="hsl(var(--text-muted))"
              fontSize={11}
              tickLine={false}
              axisLine={false}
              unit={unit}
              domain={domain}
              width={48}
            />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Tooltip
              contentStyle={TOOLTIP_STYLE}
              formatter={(value, name) => [
                unit ? `${value}${unit}` : String(value),
                name,
              ]}
            />
            {legendNames.map((series, idx) => (
              <Line
                key={series}
                type="monotone"
                dataKey={series}
                stroke={CHART_COLORS[idx % CHART_COLORS.length]}
                strokeWidth={2}
                dot={false}
                isAnimationActive={false}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      )}
    </ProCard>
  );
};

export default DcgmChartCard;
