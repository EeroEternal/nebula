import { FC, useMemo } from 'react';
import { Card, Progress } from 'antd';
import { Server, Wifi, Cpu, HardDrive } from 'lucide-react';
import classNames from 'classnames';
import { isEmpty, size } from 'lodash';
import { history } from '@umijs/max';
import { l } from '@/utils/intl';
import type { OverviewResponse, Gpus } from '@/types/Public/data';
import {
  bytesToGB,
  transformRate,
  getUsageLevel,
  getUsageStrokeColor,
  getUsageProgressClassName,
} from '@/utils';

interface DeviceOverviewProps {
  avgGpuUsage: number;
  deviceInfo: OverviewResponse['device_info'];
}
type GpuInfo = Gpus[string];
interface GpuWithWorker extends GpuInfo {
  worker_ip: string;
  worker_address: string;
}

const DeviceOverview: FC<DeviceOverviewProps> = ({ deviceInfo, avgGpuUsage }) => {
  const onlineNodes = size(deviceInfo.filter((item) => item.status === 'online'));
  const totalNodes = size(deviceInfo);
  // 同机多 Worker 用 worker_address（ip:port）去重，不能只看 IP
  const gpuArray = useMemo(() => {
    const gpuList: GpuWithWorker[] = [];
    deviceInfo.forEach((item) => {
      const worker_address = item.worker_address;
      const worker_ip = worker_address.split(':')[0];
      if (
        !gpuList.some((gpu) => gpu.worker_address === worker_address) &&
        !isEmpty(item.gpus)
      ) {
        gpuList.push(
          ...Object.values(item.gpus).map((gpu) => ({
            ...gpu,
            worker_ip,
            worker_address,
          })),
        );
      }
    });
    return gpuList;
  }, [deviceInfo]);
  // NPU 键为 gpu-{npu}-{chip}，status 可能尚未贴上；缺省按在线计
  const activeGpus = gpuArray.filter(
    (item) => !item.status || item.status === 'online',
  ).length;
  const totalGpus = gpuArray.length;
  const { usedMemory, totalMemory, memoryPercent } = useMemo(() => {
    const { totalBytes, usedBytes } = gpuArray.reduce(
      (acc, { mem_total, mem_used }) => ({
        totalBytes: acc.totalBytes + mem_total,
        usedBytes: acc.usedBytes + mem_used,
      }),
      { totalBytes: 0, usedBytes: 0 },
    );
    return {
      usedMemory: bytesToGB(usedBytes),
      totalMemory: bytesToGB(totalBytes),
      memoryPercent: usedBytes > 0 ? transformRate(usedBytes / totalBytes) : 0,
    };
  }, [gpuArray]);

  const toDevices = () => history.push('/monitor/cluster');
  return (
    <Card
      title={
        <div className="inline-flex gap-2 items-center cursor-pointer" onClick={toDevices}>
          <Server size={16} className="text-muted" />
          {l('dashboard.deviceOverview')}
        </div>
      }
      extra={
        <div
          className={classNames(
            'flex items-center border border-transparent text-xs font-normal rounded-lg py-0.5 px-2.5 hover:bg-background/80',
            onlineNodes === totalNodes
              ? 'bg-emerald-100 text-emerald-700 '
              : 'bg-amber-100 text-amber-700',
          )}
        >
          <Wifi size={12} className="mr-1" />
          {onlineNodes}/{totalNodes} {l('dashboard.device.nodesOnline')}
        </div>
      }
      className="!rounded-lg !border-[color:var(--c-border-light)] !shadow-card"
      classNames={{ header: '!border-b-0 !p-6 !pb-4', body: '!pt-0' }}
    >
      <div className="grid grid-cols-2 gap-3">
        <div className="rounded-xl bg-background/50 p-4">
          <div className="flex items-center gap-2 text-muted mb-2">
            <Server size={16} />
            <span className="text-xs">{l('dashboard.device.nodesOnline')}</span>
          </div>
          <div className="flex items-baseline gap-1">
            <span className="text-2xl font-semibold text-default">{onlineNodes}</span>
            <span className="text-sm text-muted">/ {totalNodes}</span>
          </div>
        </div>

        <div className="rounded-xl bg-background/50 p-4">
          <div className="flex items-center gap-2 text-muted mb-2">
            <Cpu size={16} />
            <span className="text-xs">{l('dashboard.device.gpuDevices')}</span>
          </div>
          <div className="flex items-baseline gap-1">
            <span className="text-2xl font-semibold text-default">{activeGpus}</span>
            <span className="text-sm text-muted">/ {totalGpus}</span>
          </div>
        </div>
      </div>
      <div className="rounded-xl bg-background/50 p-4 flex flex-col gap-3 mt-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-muted">
            <Cpu size={16} />
            <span className="text-sm">{l('dashboard.device.avgGpuUsage')}</span>
          </div>
          <span
            className={classNames(
              'text-sm font-medium font-mono tabular-nums',
              getUsageLevel(avgGpuUsage) === 'critical' && 'text-error',
              getUsageLevel(avgGpuUsage) === 'warning' && 'text-warning',
              getUsageLevel(avgGpuUsage) === 'normal' && 'text-default',
            )}
          >
            {avgGpuUsage}%
          </span>
        </div>
        <Progress
          showInfo={false}
          percent={avgGpuUsage}
          strokeColor={getUsageStrokeColor(avgGpuUsage)}
          className={`!leading-[0] ${getUsageProgressClassName(avgGpuUsage)}`}
        />
      </div>
      <div className="rounded-xl bg-background/50 p-4 flex flex-col gap-3 mt-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-muted">
            <HardDrive size={16} />
            <span className="text-sm">{l('dashboard.device.memoryUsage')}</span>
          </div>
          <span className="text-sm text-muted font-mono tabular-nums">
            {usedMemory} / {totalMemory} GB
          </span>
        </div>
        <Progress
          showInfo={false}
          percent={memoryPercent}
          strokeColor={getUsageStrokeColor(memoryPercent)}
          className={`!leading-[0] ${getUsageProgressClassName(memoryPercent)}`}
        />
      </div>
    </Card>
  );
};
export default DeviceOverview;
