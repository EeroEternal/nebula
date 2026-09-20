import { useMemo, useState } from 'react';
import { Alert, Button, Segmented, message } from 'antd';
import dayjs from 'dayjs';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { formatTime } from '@/utils/fomatData';
import { extractPromMatrixResult } from '@/utils/promResult';
import { downloadCsv } from '@/utils/downloadCsv';
import DcgmChartCard, {
  dcgmSeriesLabel,
  DCGM_PRESET_MINUTES,
  DCGM_PRESET_STEP,
  type DcgmTimePreset,
} from '../components/DcgmChartCard';
import { MonitorIcons } from '../icons';

const TIME_PRESETS: DcgmTimePreset[] = ['15m', '1h', '6h', '24h'];

type ChartSpec = {
  title: string;
  queries: Array<{ name?: string; promql: string }>;
  unit?: string;
  yDomain?: [number | 'auto', number | 'auto'];
};

/** 集群监控 · 卡健康：进入页/切换时间窗拉取，不轮询 */
const HardwarePanel = () => {
  const [preset, setPreset] = useState<DcgmTimePreset>('1h');
  const [exporting, setExporting] = useState(false);
  const Download = MonitorIcons.download;

  const charts: ChartSpec[] = useMemo(
    () => [
      {
        title: l('monitor.cluster.hardware.temp'),
        queries: [{ promql: 'DCGM_FI_DEV_GPU_TEMP' }],
        unit: '°C',
      },
      {
        title: l('monitor.cluster.hardware.power'),
        queries: [{ promql: 'DCGM_FI_DEV_POWER_USAGE' }],
        unit: 'W',
      },
      {
        title: l('monitor.cluster.hardware.util'),
        queries: [{ promql: 'DCGM_FI_DEV_GPU_UTIL' }],
        unit: '%',
        yDomain: [0, 100],
      },
      {
        title: l('monitor.cluster.hardware.fb'),
        queries: [{ promql: 'DCGM_FI_DEV_FB_USED' }],
        unit: 'MiB',
      },
      {
        title: l('monitor.cluster.hardware.pcie'),
        queries: [
          { name: 'TX', promql: 'DCGM_FI_PROF_PCIE_TX_BYTES' },
          { name: 'RX', promql: 'DCGM_FI_PROF_PCIE_RX_BYTES' },
        ],
        unit: 'B/s',
      },
      {
        title: l('monitor.cluster.hardware.tensor',
        ),
        queries: [{ promql: 'DCGM_FI_PROF_PIPE_TENSOR_ACTIVE' }],
        yDomain: [0, 1],
      },
      {
        title: l('monitor.cluster.hardware.pcieReplay',
        ),
        queries: [{ promql: 'DCGM_FI_DEV_PCIE_REPLAY_COUNTER' }],
      },
      {
        title: l('monitor.cluster.hardware.ecc',
        ),
        queries: [{ promql: 'DCGM_FI_DEV_ECC_SBE_VOL_TOTAL' }],
      },
    ],
    [],
  );

  const handleDownload = async () => {
    if (exporting) return;
    setExporting(true);
    try {
      const minutes = DCGM_PRESET_MINUTES[preset];
      const endSec = Math.floor(Date.now() / 1000);
      const startSec = endSec - minutes * 60;
      const step = DCGM_PRESET_STEP[preset];
      const rows: Array<Array<unknown>> = [];

      for (const chart of charts) {
        for (const q of chart.queries) {
          const res = await request(
            `/p/query_range?query=${encodeURIComponent(
              q.promql,
            )}&start=${startSec}&end=${endSec}&step=${step}`,
          );
          const matrix = extractPromMatrixResult(res as { success?: boolean });
          for (const row of matrix) {
            const series = dcgmSeriesLabel(row.metric || {}, q.name);
            for (const [ts, val] of row.values || []) {
              rows.push([
                formatTime(Number(ts)),
                chart.title,
                series,
                val,
                chart.unit || '',
              ]);
            }
          }
        }
      }

      if (!rows.length) {
        message.warning(
          l('monitor.cluster.hardware.exportEmpty'),
        );
        return;
      }

      downloadCsv(
        `cluster-hardware-${preset}-${dayjs().format('YYYYMMDD-HHmmss')}.csv`,
        ['time', 'metric', 'series', 'value', 'unit'],
        rows,
      );
    } catch {
      message.error(l('monitor.cluster.hardware.exportFailed'));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Alert
        type="info"
        showIcon
        banner
        message={l('monitor.cluster.hardware.nvidiaOnly',
        )}
      />
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          size="small"
          icon={<Download size={14} />}
          loading={exporting}
          onClick={handleDownload}
        >
          {l('monitor.platform.downloadCsv')}
        </Button>
        <Segmented
          size="small"
          value={preset}
          options={TIME_PRESETS.map((p) => ({ label: p, value: p }))}
          onChange={(v) => setPreset(v as DcgmTimePreset)}
        />
      </div>
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {charts.map((chart) => (
          <DcgmChartCard
            key={chart.title}
            title={chart.title}
            queries={chart.queries}
            unit={chart.unit}
            preset={preset}
            live={false}
            yDomain={chart.yDomain}
          />
        ))}
      </div>
    </div>
  );
};

export default HardwarePanel;
