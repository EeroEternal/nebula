import { useRequest } from 'ahooks';
import { Button, Progress, Spin } from 'antd';
import { Activity, Gauge, RefreshCw } from 'lucide-react';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { matchGpuUtil } from '@/utils/monitorMetrics';
import { usePageVisible } from '@/hooks/usePageVisible';
import type { DeviceInfo } from '@/types/Public/data';

type ReplicaRuntime = {
  replica_model_uid?: string;
  worker_address?: string;
  gpu_idx?: number[];
  pending_requests?: number;
  active_requests?: number;
};

export type RuntimePayload = {
  pending_requests?: number;
  active_requests?: number;
  pending?: number;
  active?: number;
  replicas?: ReplicaRuntime[];
};

type Props = {
  modelUid: string;
  /** 空 / null = 实例全副本聚合 */
  replicaUid?: string | null;
  devices?: DeviceInfo[];
  /** 父级 snapshot；有则不再自己拉 */
  runtimeOverride?: RuntimePayload | null;
  onRefresh?: () => void;
};

/** 瞬时负载：排队 / 进行中 / 显存；支持按副本筛选 */
const InstantLoadCard = ({
  modelUid,
  replicaUid,
  devices = [],
  runtimeOverride,
  onRefresh,
}: Props) => {
  const pageVisible = usePageVisible();
  const useOverride = runtimeOverride != null;
  const { data, loading, refresh } = useRequest(
    () => request(`/models/${encodeURIComponent(modelUid)}/runtime_metrics`),
    {
      ready: !!modelUid && pageVisible && !useOverride,
      refreshDeps: [modelUid],
      onError: () => undefined,
    },
  );

  const httpRuntime = (data?.data?.data || data?.data) as RuntimePayload | undefined;
  const runtime: RuntimePayload = (
    useOverride ? runtimeOverride || {} : httpRuntime || {}
  ) as RuntimePayload;
  const replicas = runtime.replicas || [];
  const selected = replicaUid
    ? replicas.find((r) => r.replica_model_uid === replicaUid)
    : undefined;

  const pending = Math.max(
    0,
    Number(
      selected
        ? selected.pending_requests
        : runtime.pending_requests ?? runtime.pending ?? 0,
    ) || 0,
  );
  const active = Math.max(
    0,
    Number(
      selected
        ? selected.active_requests
        : runtime.active_requests ?? runtime.active ?? 0,
    ) || 0,
  );

  const workerAddress =
    selected?.worker_address || replicas[0]?.worker_address || '';
  const gpuIdx = (selected?.gpu_idx || replicas[0]?.gpu_idx) as
    | number[]
    | undefined;
  const gpuHint =
    workerAddress || gpuIdx?.length
      ? matchGpuUtil(devices, workerAddress, gpuIdx)
      : null;

  return (
    <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card p-4 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-medium flex items-center gap-1.5">
          <Activity size={14} />
          {l('monitor.instances.resources')}
          {replicaUid ? (
            <span className="text-xs font-normal text-muted truncate max-w-[240px]">
              · {replicaUid}
            </span>
          ) : (
            <span className="text-xs font-normal text-muted">
              · {l('monitor.instances.filter.allReplicas')}
            </span>
          )}
        </div>
        <Button
          size="small"
          icon={<RefreshCw size={14} />}
          onClick={() => (onRefresh ? onRefresh() : refresh())}
        >
          {l('global.actions.refresh')}
        </Button>
      </div>

      <Spin spinning={Boolean(loading && httpRuntime === undefined && !runtimeOverride)}>
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
          <div>
            <div className="text-xs text-muted">
              {l('monitor.instances.pending')}
            </div>
            <div className="text-3xl font-semibold tabular-nums tracking-tight">
              {pending}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted">
              {l('monitor.instances.active')}
            </div>
            <div className="text-3xl font-semibold tabular-nums tracking-tight">
              {active}
            </div>
          </div>
          {gpuHint?.util != null ? (
            <div className="col-span-2 sm:col-span-1">
              <div className="text-xs text-muted mb-1 flex items-center gap-1">
                <Gauge size={12} />
                {l('monitor.instances.gpuUtilRatio')}
              </div>
              <Progress percent={Math.round(gpuHint.util)} size="small" />
            </div>
          ) : null}
        </div>
      </Spin>
    </div>
  );
};

export default InstantLoadCard;
