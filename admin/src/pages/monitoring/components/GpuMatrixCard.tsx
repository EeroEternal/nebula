import { history } from '@umijs/max';
import cn from 'classnames';
import { getUsageStrokeColor } from '@/utils';
import type { GpuRow } from '@/utils/monitorMetrics';
import { MonitorIcons } from '../icons';
import { l } from '@/utils/intl';
import { useK8sRuntime } from '@/hooks/useK8sRuntime';

const BORDER: Record<GpuRow['status'], string> = {
  ok: 'border-l-success',
  warn: 'border-l-warning',
  crit: 'border-l-error bg-error/5',
  idle: 'border-l-muted',
  offline: 'border-l-muted opacity-55',
};

const DOT: Record<GpuRow['status'], string> = {
  ok: 'bg-success',
  warn: 'bg-warning',
  crit: 'bg-error',
  idle: 'bg-muted',
  offline: 'bg-muted',
};

type Props = { row: GpuRow };

/** 算力矩阵卡：标题/角标分行，避免窄宽下文字堆叠 */
const GpuMatrixCard = ({ row }: Props) => {
  const { hamiEnabled } = useK8sRuntime();
  const util = row.util;
  const utilColor = util === null ? 'var(--c-muted)' : getUsageStrokeColor(util);
  const memColor = getUsageStrokeColor(row.memUsage);
  const high = row.status === 'warn' || row.status === 'crit';
  const AccelIcon = MonitorIcons.accelerator;
  const ExternalLink = MonitorIcons.externalLink;
  const AlertTriangle = MonitorIcons.alert;
  const isNpu = (row.acceleratorKind || '').toLowerCase() === 'npu';

  return (
    <article
      className={cn(
        'flex min-h-[96px] flex-col rounded-md border border-[color:var(--c-border-light)] border-l-[3px] bg-[var(--c-surface)] px-2.5 py-2',
        BORDER[row.status],
      )}
    >
      <div className="mb-1 flex flex-nowrap items-center justify-between gap-1">
        <div className="flex min-w-0 flex-nowrap items-center gap-1 font-mono text-[11px] font-bold">
          <AccelIcon size={11} aria-hidden className="shrink-0 text-muted" />
          <span className="shrink-0 whitespace-nowrap">
            {isNpu ? `NPU-${row.idx}` : `GPU-${row.idx}`}
          </span>
          {isNpu ? (
            <span className="shrink-0 rounded bg-background-muted px-0.5 text-[9px] font-sans font-medium text-muted">
              NPU
            </span>
          ) : null}
        </div>
        <div className="flex shrink-0 flex-nowrap items-center gap-1 whitespace-nowrap">
          <span
            className={cn(
              'rounded px-1 text-[10px] font-medium tabular-nums',
              row.temperatureC != null && row.temperatureC >= 80
                ? 'bg-warning/15 text-warning'
                : 'bg-background-muted text-muted',
            )}
          >
            {row.temperatureC != null
              ? `${Math.round(row.temperatureC)}°C`
              : l('monitor.cluster.gpu.temp.na')}
          </span>
          <span className="rounded bg-background-muted px-1 text-[10px] font-medium tabular-nums text-muted">
            {row.powerW != null
              ? `${Math.round(row.powerW)}W`
              : l('monitor.cluster.gpu.power.na')}
          </span>
          <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', DOT[row.status])} />
        </div>
      </div>
      <div className="mb-1.5 truncate text-[10px] text-muted" title={row.name}>
        {row.name}
      </div>
      <div className="flex flex-col gap-1">
        <Metric
          label={l('monitor.cluster.gpu.util.short', 'Util')}
          value={
            util === null
              ? isNpu
                ? l('monitor.cluster.gpu.util.na')
                : '—'
              : `${util}%`
          }
          color={utilColor}
          fill={util ?? 0}
          warn={high && util !== null && util < 10 && row.memUsage >= 70}
          AlertIcon={AlertTriangle}
        />
        <Metric
          label={l('monitor.cluster.gpu.mem.short', 'Mem')}
          value={`${row.memUsedGb}/${row.memTotalGb}G`}
          color={memColor}
          fill={row.memUsage}
          warn={high}
          AlertIcon={AlertTriangle}
        />
      </div>
      <div className="mt-auto pt-1.5 truncate text-[10px] text-muted">
        {row.occupant?.model_uid ? (
          <button
            type="button"
            className="inline-flex max-w-full items-center gap-0.5 text-left text-primary hover:underline"
            title={
              row.occupant.replica_uids?.[0] ||
              row.occupant.model_name ||
              row.occupant.model_uid
            }
            onClick={() =>
              history.push(
                `/monitor/instances?id=${encodeURIComponent(row.occupant!.model_uid)}`,
              )
            }
          >
            <ExternalLink size={9} aria-hidden />
            <span className="truncate">
              {row.occupant.replica_uids?.[0] ||
                l('monitor.cluster.gpu.replica.unknown')}
            </span>
          </button>
        ) : row.status === 'idle' ? (
          l('monitor.platform.filter.idle')
        ) : row.status === 'offline' ? (
          l('monitor.platform.filter.offline')
        ) : (
          `${l('monitor.deviceInfo.status')} ${row.status}`
        )}
        {hamiEnabled ? (
          <button
            type="button"
            className="ml-1 text-primary hover:underline"
            onClick={() =>
              history.push(
                `/monitor/hami?gpu=${encodeURIComponent(`${row.node}/${row.idx}`)}`,
              )
            }
          >
            {l('menu.monitor.hami')}
          </button>
        ) : null}
      </div>
    </article>
  );
};

function Metric({
  label,
  value,
  color,
  fill,
  warn,
  AlertIcon,
}: {
  label: string;
  value: string;
  color: string;
  fill: number;
  warn?: boolean;
  AlertIcon: typeof MonitorIcons.alert;
}) {
  return (
    <div className="grid grid-cols-[2rem_1fr_auto] items-center gap-1.5">
      <span className="inline-flex items-center gap-0.5 text-[10px] text-muted">
        {label}
        {warn ? <AlertIcon size={9} className="text-warning" aria-hidden /> : null}
      </span>
      <div className="h-1 min-w-0 overflow-hidden rounded-full bg-[color:var(--c-border-light)]">
        <div
          className="h-full rounded-full"
          style={{ width: `${Math.min(fill, 100)}%`, background: color }}
        />
      </div>
      <span
        className="min-w-[4.25rem] text-right font-mono text-[10px] font-semibold tabular-nums"
        style={{ color }}
      >
        {value}
      </span>
    </div>
  );
}

export default GpuMatrixCard;
