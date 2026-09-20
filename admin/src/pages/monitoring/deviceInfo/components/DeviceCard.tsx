import { Server, RefreshCw, Power } from 'lucide-react';
import { ProCard } from '@ant-design/pro-components';
import { Button, Progress, App } from 'antd';
import { FC } from 'react';
import { useRequest } from 'ahooks';
import cn from 'classnames';
import { DeviceStatus } from '@/components';
import { l, lGet } from '@/utils/intl';
import { getUsageStrokeColor, getUsageProgressClassName } from '@/utils';
import request from '@/utils/request';

import type { DeviceCardSource } from '../types';
import GpuCard from './GpuCard';
import SyncWorkModal from './SyncWorkModal';

interface DeviceCardProps {
  data: DeviceCardSource;
  reload: () => void;
  /** full | node（主机+紧凑 GPU）| ops（仅运维条） */
  variant?: 'full' | 'node' | 'ops';
}

const DeviceCard: FC<DeviceCardProps> = ({ data, reload, variant = 'full' }) => {
  const { message, modal } = App.useApp();
  const compact = variant === 'node' || variant === 'ops';
  const { runAsync: offline } = useRequest(
    () => request('/workers', { method: 'delete', data: { worker_address: data.worker_address } }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          message.success('Offline Success! ');
          reload();
        }
      },
    },
  );
  const handleOffline = () => {
    modal.confirm({
      title: lGet('monitor.deviceInfo.confirmOfflineWorker'),
      onOk: () => offline(),
    });
  };
  const btnSize = compact ? 'small' : 'large';
  const avgUtil = (() => {
    const utils = data.gpus
      .map((g) => g.utilRate)
      .filter((u): u is number => u != null && !Number.isNaN(u));
    if (!utils.length) return null;
    return Math.round(utils.reduce((s, u) => s + u, 0) / utils.length);
  })();
  const avgMem = data.gpus.length
    ? Math.round(data.gpus.reduce((s, g) => s + (g.usageRate || 0), 0) / data.gpus.length)
    : 0;

  return (
    <ProCard
      size={compact ? 'small' : 'default'}
      headStyle={compact ? { paddingBlock: 8 } : { paddingTop: 24 }}
      bodyStyle={
        variant === 'ops'
          ? { display: 'none', padding: 0 }
          : compact
            ? { padding: '8px 12px 12px' }
            : { paddingBottom: 24 }
      }
      title={
        <div className={cn('flex min-w-0 items-center', compact ? 'gap-2' : 'gap-3')}>
          <Server size={compact ? 16 : 20} className="shrink-0 text-muted" />
          <div className="min-w-0">
            <div
              className={cn(
                'flex flex-wrap items-center gap-2',
                compact ? 'text-sm font-medium' : 'font-semibold',
              )}
            >
              <span className="truncate">{data.name}</span>
              <DeviceStatus status={data.status} />
            </div>
            <p className="truncate text-xs text-muted">{data.worker_address}</p>
          </div>
        </div>
      }
      extra={
        <div className="flex shrink-0 items-center gap-2">
          <SyncWorkModal currentIpAddress={data.worker_address} submitCallback={reload}>
            <Button size={btnSize} icon={<RefreshCw size={14} />}>
              {l('monitor.deviceInfo.sync')}
            </Button>
          </SyncWorkModal>
          <Button danger size={btnSize} icon={<Power size={14} />} onClick={handleOffline}>
            {l('monitor.deviceInfo.offline')}
          </Button>
        </div>
      }
    >
      {variant !== 'ops' && (
        <>
          {variant === 'node' ? (
            <div className="mb-2 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-muted">
              <span>
                CPU <span className="font-mono text-default">{data.cpu.usageRate}%</span>
              </span>
              <span>
                {l('monitor.deviceInfo.cpu.memory')}{' '}
                <span className="font-mono text-default">
                  {data.cpu.memoryUsedGB}/{data.cpu.memoryTotalGB}G · {data.cpu.memoryRate}%
                </span>
              </span>
              <span>
                GPU <span className="font-mono text-default">{data.gpus.length}</span>
                {avgUtil != null ? (
                  <span className="font-mono text-default"> · util {avgUtil}%</span>
                ) : null}
                <span className="font-mono text-default"> · mem {avgMem}%</span>
              </span>
              {data.accelerator_kind ? <span>{data.accelerator_kind}</span> : null}
              {data.machine_model ? <span className="truncate">{data.machine_model}</span> : null}
            </div>
          ) : null}
          <div className={cn('grid grid-cols-2', compact ? 'mb-2 gap-2' : 'mb-4 gap-3')}>
            <div>
              <div className="flex justify-between text-xs leading-5">
                <span className="text-muted">CPU</span>
                <span className="font-mono tabular-nums">{data.cpu.usageRate}%</span>
              </div>
              <Progress
                showInfo={false}
                size={compact ? 'small' : undefined}
                percent={data.cpu.usageRate}
                strokeColor={getUsageStrokeColor(data.cpu.usageRate)}
                className={`!leading-[0] ${getUsageProgressClassName(data.cpu.usageRate)}`}
              />
            </div>
            <div>
              <div className="flex justify-between text-xs leading-5">
                <span className="text-muted">{l('monitor.deviceInfo.cpu.memory')}</span>
                <span className="font-mono tabular-nums">
                  {data.cpu.memoryUsedGB}/{data.cpu.memoryTotalGB}G
                </span>
              </div>
              <Progress
                showInfo={false}
                size={compact ? 'small' : undefined}
                percent={data.cpu.memoryRate}
                strokeColor={getUsageStrokeColor(data.cpu.memoryRate)}
                className={`!leading-[0] ${getUsageProgressClassName(data.cpu.memoryRate)}`}
              />
            </div>
          </div>
          <div
            className={cn(
              'grid gap-2',
              compact
                ? 'grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6'
                : 'grid-cols-4 gap-3',
            )}
          >
            {data.gpus.map((gpu, index) => (
              <GpuCard key={`gpu-${index}`} index={index} data={gpu} dense={compact} />
            ))}
          </div>
          {variant === 'node' ? (
            <p className="mt-1.5 text-[10px] text-disabled">
              {l('monitor.cluster.nodes.gpuDetailHint')}
            </p>
          ) : null}
        </>
      )}
    </ProCard>
  );
};
export default DeviceCard;
