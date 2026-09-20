import { useMemo, useState } from 'react';
import { useRequest } from 'ahooks';
import { Radio } from 'antd';
import {
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { EmptyState } from '@/components';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import { extractPromMatrixResult } from '@/utils/promResult';
import { formatTime } from '@/utils/fomatData';
import {
  DCGM_PRESET_MINUTES,
  DCGM_PRESET_STEP,
  type DcgmTimePreset,
} from '../components/DcgmChartCard';

type Props = {
  quotaGb?: number;
  gpuIndex?: string;
};

const HamiHistoryPane = ({ quotaGb, gpuIndex }: Props) => {
  const [preset, setPreset] = useState<DcgmTimePreset>('1h');
  const [kind, setKind] = useState<'gpu' | 'model'>('gpu');

  const { data, error } = useRequest(
    async () => {
      const minutes = DCGM_PRESET_MINUTES[preset];
      const endSec = Math.floor(Date.now() / 1000);
      const startSec = endSec - minutes * 60;
      const step = DCGM_PRESET_STEP[preset];
      const idx = gpuIndex && gpuIndex.includes('/') ? gpuIndex.split('/').pop() : gpuIndex;
      const filter = idx != null && idx !== '' ? `{gpu="${idx}"}` : '';
      const query = `powerllm:worker_gpu_memory_used${filter}`;
      const res = await request(
        `/p/query_range?query=${encodeURIComponent(query)}&start=${startSec}&end=${endSec}&step=${step}`,
        { skipNotification: true },
      );
      return res;
    },
    { refreshDeps: [preset, gpuIndex, kind] },
  );

  const chartData = useMemo(() => {
    const rows = extractPromMatrixResult(data as { success?: boolean });
    const byIdx = new Map<number, Record<string, number | string>>();
    for (const row of rows) {
      for (const [ts, val] of row.values || []) {
        const t = Number(ts);
        let raw = parseFloat(val);
        if (!Number.isFinite(raw)) continue;
        if (raw > 4096) raw = raw / (1024 * 1024 * 1024);
        if (!byIdx.has(t)) byIdx.set(t, { t, time: formatTime(t), used: raw });
        else byIdx.get(t)!.used = raw;
      }
    }
    return [...byIdx.values()].sort((a, b) => Number(a.t) - Number(b.t));
  }, [data]);

  const empty = Boolean(error) || !chartData.length;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Radio.Group
          size="small"
          value={kind}
          onChange={(e) => setKind(e.target.value)}
        >
          <Radio.Button value="gpu">{lGet('monitor.hami.history.gpu')}</Radio.Button>
          <Radio.Button value="model">{lGet('monitor.hami.history.model')}</Radio.Button>
        </Radio.Group>
        <Radio.Group
          size="small"
          value={preset}
          onChange={(e) => setPreset(e.target.value)}
        >
          {(['15m', '1h', '6h', '24h'] as DcgmTimePreset[]).map((p) => (
            <Radio.Button key={p} value={p}>
              {p}
            </Radio.Button>
          ))}
        </Radio.Group>
      </div>
      {empty ? (
        <EmptyState
          title={l('monitor.hami.history.empty')}
          description={l('monitor.hami.history.emptyHint',
          )}
        />
      ) : (
        <div className="h-[220px] w-full">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={chartData} margin={{ left: 8, right: 8, top: 8 }}>
              <XAxis dataKey="time" fontSize={11} tickLine={false} />
              <YAxis fontSize={11} tickLine={false} unit="GB" />
              <Tooltip />
              {quotaGb && quotaGb > 0 ? (
                <ReferenceLine
                  y={quotaGb}
                  strokeDasharray="4 4"
                  label={lGet('monitor.hami.history.quota')}
                />
              ) : null}
              <Line
                type="monotone"
                dataKey="used"
                name={lGet('monitor.hami.history.used')}
                dot={false}
                strokeWidth={1.5}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
};

export default HamiHistoryPane;
