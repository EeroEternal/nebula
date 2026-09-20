import { useMemo } from 'react';
import { useRequest } from 'ahooks';
import { Button, Empty, Spin, Tooltip } from 'antd';
import dayjs from 'dayjs';
import { RefreshCw } from 'lucide-react';
import { DATE_FORMAT } from '@/constants';
import { convertDateToUTC } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import MetricLabel, { normalizeInstanceModel } from '../../components/MetricLabel';

type UsageRow = {
  model?: string;
  countTraces?: number;
  inputUsage?: number;
  outputUsage?: number;
  totalUsage?: number;
};

type DailyRow = {
  date?: string;
  countTraces?: number;
  usage?: UsageRow[];
};

type BreakdownEntity = {
  key?: string;
  requests?: number;
  input_tokens?: number;
  output_tokens?: number;
  total_tokens?: number;
  duration_avg?: number | null;
  duration_sum?: number;
};

type ConvStatsPayload = {
  model_uid?: string;
  daily?: { data?: DailyRow[] } | DailyRow[] | Record<string, unknown>;
  breakdown?: { entities?: BreakdownEntity[] };
};

const fmtInt = (n?: number | null) =>
  n == null || Number.isNaN(Number(n)) ? '—' : Number(n).toLocaleString();

const fmtLat = (n?: number | null) =>
  n == null || Number.isNaN(Number(n)) ? '—' : `${Number(n).toFixed(2)}s`;

type Props = {
  modelUid: string;
  days?: number;
};

function matchesModel(model: string, modelUid: string): boolean {
  const m = String(model || '').trim();
  const uid = String(modelUid || '').trim();
  if (!m || !uid) return false;
  if (m === uid) return true;
  if (normalizeInstanceModel(m) === uid) return true;
  if (m.startsWith(`${uid}-`)) return true;
  return false;
}

function extractDailyList(daily: ConvStatsPayload['daily']): DailyRow[] {
  if (!daily) return [];
  if (Array.isArray(daily)) return daily;
  if (Array.isArray((daily as { data?: DailyRow[] }).data)) {
    return (daily as { data: DailyRow[] }).data;
  }
  return [];
}

/**
 * 近 N 天对话用量：与「对话监控」同源（Langfuse/xtrace daily + breakdown），
 * 按逻辑实例聚合副本级 model 标签。
 */
const UsageDailyTotals = ({ modelUid, days = 30 }: Props) => {
  const fromDay = dayjs().subtract(days - 1, 'day').startOf('day');
  const toDay = dayjs().endOf('day');
  const fromTimestamp = convertDateToUTC(`${fromDay.format(DATE_FORMAT)} 00:00:00`);
  const toTimestamp = convertDateToUTC(`${toDay.format(DATE_FORMAT)} 23:59:59`);

  const { data, loading, refresh } = useRequest(
    () =>
      request<{ data: ConvStatsPayload }>(
        `/monitor/models/${encodeURIComponent(modelUid)}/conversation-stats`,
        {
          params: { fromTimestamp, toTimestamp },
        },
      ),
    {
      ready: !!modelUid,
      refreshDeps: [modelUid, fromTimestamp, toTimestamp],
      onError: () => undefined,
    },
  );

  const payload = (data?.data?.data || data?.data || {}) as ConvStatsPayload;

  const totals = useMemo(() => {
    const dailyList = extractDailyList(payload.daily);
    let requests = 0;
    let inputTokens = 0;
    let outputTokens = 0;
    let daysWithData = 0;

    for (const row of dailyList) {
      const usages = Array.isArray(row.usage) ? row.usage : [];
      let dayReq = 0;
      for (const u of usages) {
        if (!matchesModel(String(u.model || ''), modelUid)) continue;
        dayReq += Number(u.countTraces || 0);
        inputTokens += Number(u.inputUsage || 0);
        outputTokens += Number(u.outputUsage || 0);
      }
      if (dayReq > 0) {
        requests += dayReq;
        daysWithData += 1;
      }
    }

    // breakdown 补充平均延迟（同源对话监控）
    const entities = payload.breakdown?.entities || [];
    let durationSum = 0;
    let durationReq = 0;
    for (const e of entities) {
      if (!matchesModel(String(e.key || ''), modelUid)) continue;
      const req = Number(e.requests || 0);
      if (e.duration_sum != null && req > 0) {
        durationSum += Number(e.duration_sum);
        durationReq += req;
      } else if (e.duration_avg != null && req > 0) {
        durationSum += Number(e.duration_avg) * req;
        durationReq += req;
      }
    }
    const durationAvg = durationReq > 0 ? durationSum / durationReq : null;

    return {
      requests,
      inputTokens,
      outputTokens,
      totalTokens: inputTokens + outputTokens,
      durationAvg,
      daysWithData,
    };
  }, [payload, modelUid]);

  const hasData = totals.requests > 0 || totals.inputTokens > 0 || totals.outputTokens > 0;

  const cards = [
    {
      key: 'request',
      zh: l('monitor.instances.usage.requests'),
      en: 'Traces',
      value: fmtInt(totals.requests),
    },
    {
      key: 'in',
      zh: l('monitor.instances.usage.inputTokens'),
      en: 'Input tok',
      value: fmtInt(totals.inputTokens),
    },
    {
      key: 'out',
      zh: l('monitor.instances.usage.outputTokens'),
      en: 'Output tok',
      value: fmtInt(totals.outputTokens),
    },
    {
      key: 'total',
      zh: l('monitor.instances.usage.totalTokens'),
      en: 'Total tok',
      value: fmtInt(totals.totalTokens),
    },
    {
      key: 'latency',
      zh: l('monitor.instances.usage.avgLatency'),
      en: 'Avg latency',
      value: fmtLat(totals.durationAvg),
    },
  ];

  const rangeHint = `${fromDay.format(DATE_FORMAT)} ~ ${toDay.format(DATE_FORMAT)}`;

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">
            {l('monitor.instances.usage.titleConv', { days })}
          </div>
          <div className="text-xs text-muted mt-0.5">
            {rangeHint}
            {' · '}
            {l('monitor.instances.usage.fromTraffic')}
          </div>
        </div>
        <Tooltip title={l('global.actions.refresh')}>
          <Button
            size="small"
            type="text"
            icon={<RefreshCw size={14} />}
            loading={loading}
            onClick={() => refresh()}
          />
        </Tooltip>
      </div>
      <Spin spinning={loading && !hasData}>
        {!loading && !hasData ? (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={l('global.noData')}
          />
        ) : (
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {cards.map((c) => (
              <div
                key={c.key}
                className="rounded-lg border border-[color:var(--c-border-light)] bg-[var(--c-bg)] px-3 py-2.5"
              >
                <MetricLabel zh={c.zh} en={c.en} />
                <div className="mt-1 text-lg font-semibold tabular-nums tracking-tight">
                  {c.value}
                </div>
              </div>
            ))}
          </div>
        )}
      </Spin>
    </div>
  );
};

export default UsageDailyTotals;
