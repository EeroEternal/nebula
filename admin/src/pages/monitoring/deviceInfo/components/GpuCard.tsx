import { Progress, Tooltip, Tag } from 'antd';
import { FC } from 'react';
import { HelpCircle } from 'lucide-react';
import cn from 'classnames';
import { l } from '@/utils/intl';
import { getUsageStrokeColor, getUsageProgressClassName } from '@/utils';
import type { GpuCardSource } from '../types';

interface GpuCardProps {
  index: number;
  data: GpuCardSource;
  dense?: boolean;
}

/** dense：单行级摘要，避免型号名与进度条堆叠错位 */
const GpuCard: FC<GpuCardProps> = ({ index, data, dense = false }) => {
  if (dense) {
    return (
      <div
        className="rounded border border-border/50 px-2 py-1.5"
        title={`${data.name || ''} · mem ${data.usageRate}%${
          data.utilRate != null ? ` · util ${data.utilRate}%` : ''
        }`}
      >
        <div className="mb-1 flex items-center justify-between gap-1 text-[11px]">
          <span className="shrink-0 font-medium">G{index}</span>
          {data.status === 'expired' ? (
            <Tooltip title={l('global.license.disabledTips')}>
              <Tag bordered={false} className="mr-0 !h-4 !px-1 text-[10px]" color="warning">
                <HelpCircle size={10} />
              </Tag>
            </Tooltip>
          ) : (
            <span className="font-mono text-[10px] tabular-nums text-muted">
              {data.utilRate != null ? `u${data.utilRate}% ` : ''}
              m{data.usageRate}%
            </span>
          )}
        </div>
        <Progress
          status="normal"
          size="small"
          percent={data.usageRate}
          showInfo={false}
          strokeColor={getUsageStrokeColor(data.usageRate)}
          className={`!mb-0 !leading-[0] ${getUsageProgressClassName(data.usageRate)}`}
        />
        <div className="mt-0.5 font-mono text-[10px] tabular-nums text-muted">
          {data.memoryUsedGB}/{data.memoryTotalGB}G
        </div>
      </div>
    );
  }

  return (
    <div className={cn('space-y-2 rounded-lg border border-border/50 p-3')}>
      <div className="flex items-center justify-between gap-1">
        <span className="text-xs font-medium">GPU {index}</span>
        {data.status === 'expired' ? (
          <Tooltip title={l('global.license.disabledTips')}>
            <Tag
              bordered={false}
              className="mr-0 h-4 cursor-pointer !px-1 text-[10px]"
              color="warning"
            >
              {l('monitor.deviceInfo.gpu.status.unauthorized')} <HelpCircle size={12} />
            </Tag>
          </Tooltip>
        ) : (
          <Tag bordered={false} className="mr-0 h-4 !px-1.5 text-[10px]" color="success">
            {l('monitor.deviceInfo.gpu.status.authorized')}
          </Tag>
        )}
      </div>
      <Progress
        status="normal"
        percent={data.usageRate}
        showInfo
        strokeColor={getUsageStrokeColor(data.usageRate)}
        className={`!mb-0 !leading-[0] ${getUsageProgressClassName(data.usageRate)}`}
      />
      <div className="flex flex-col gap-2 text-xs text-muted">
        <div className="font-mono tabular-nums">
          {l('monitor.deviceInfo.gpu.vram')}: {data.memoryUsedGB}/{data.memoryTotalGB}GB
        </div>
        <div>
          {l('monitor.deviceInfo.gpu.type')}: {data.name || '-'}
        </div>
      </div>
    </div>
  );
};
export default GpuCard;
