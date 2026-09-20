import { useEffect, useMemo, useState, type ComponentProps } from 'react';
import { useRequest } from 'ahooks';
import { Button, Drawer, Select, Switch } from 'antd';
import { FlaskConical } from 'lucide-react';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import type { DeviceInfo, ModelsInstancesListItem } from '@/types/Public/data';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { usePageVisible } from '@/hooks/usePageVisible';
import AllModelsLiveBoard from './AllModelsLiveBoard';
import InstantLoadCard, { type RuntimePayload } from './InstantLoadCard';
import PerformanceMetrics from './PerformanceMetrics';
import BenchmarkPanel from './BenchmarkPanel';

/** Select 哨兵：全部模型综合 */
const ALL_MODELS = '__all__';

type PerfPayload = NonNullable<ComponentProps<typeof PerformanceMetrics>['perfOverride']>;
type BoardPerformances = NonNullable<
  ComponentProps<typeof AllModelsLiveBoard>['performancesOverride']
>;

type LiveSnapshot = {
  instances?: ModelsInstancesListItem[];
  runtimes?: Record<string, RuntimePayload> | null;
  performances?: Record<string, PerfPayload> | null;
};

/** 模型监控 · 实时监控：0.5s snapshot；压测入口在线上性能标题栏 */
const OverviewPanel = () => {
  const pageVisible = usePageVisible();
  const [live, setLive] = useState(true);
  const [modelUid, setModelUid] = useState<string>(ALL_MODELS);
  const [replicaUid, setReplicaUid] = useState<string | undefined>();
  const [benchOpen, setBenchOpen] = useState(false);

  const { data: listRes } = useRequest(
    () =>
      request<{ data: { count?: number; results?: ModelsInstancesListItem[] } }>(
        '/models/instances',
        { params: ALL_LIST_PAGES_PARAMS },
      ),
  );

  const isAll = modelUid === ALL_MODELS;

  const { data: deviceRes } = useRequest(
    () =>
      request<{ data: { results: DeviceInfo[] } }>('/device/info', {
        params: ALL_LIST_PAGES_PARAMS,
      }),
    {
      onError: () => undefined,
      pollingInterval: live && pageVisible ? 5_000 : undefined,
      refreshDeps: [live, pageVisible],
    },
  );

  const {
    data: snapRes,
    refresh: refreshSnap,
  } = useRequest(() => request<{ data: LiveSnapshot }>('/monitor/models/live/snapshot'), {
    ready: live && pageVisible,
    pollingInterval: live && pageVisible ? 500 : undefined,
    onError: () => undefined,
  });

  const snap = (snapRes?.data?.data || snapRes?.data || {}) as LiveSnapshot;

  const instances: ModelsInstancesListItem[] = useMemo(() => {
    if (Array.isArray(snap.instances) && snap.instances.length) {
      return snap.instances;
    }
    if (Array.isArray(listRes?.data?.results)) return listRes!.data.results!;
    if (Array.isArray(listRes?.data)) return listRes!.data as ModelsInstancesListItem[];
    return [];
  }, [snap.instances, listRes]);

  const devices =
    deviceRes?.data?.results ||
    (deviceRes?.data as { data?: { results?: DeviceInfo[] } } | undefined)?.data
      ?.results ||
    [];
  const allUids = useMemo(() => instances.map((i) => i.model_uid), [instances]);
  const runtimes = snap.runtimes ?? null;
  const performances = snap.performances ?? null;

  useEffect(() => {
    if (isAll) return;
    if (!instances.length) {
      setModelUid(ALL_MODELS);
      return;
    }
    if (!instances.some((i) => i.model_uid === modelUid)) {
      setModelUid(ALL_MODELS);
      setReplicaUid(undefined);
    }
  }, [instances, modelUid, isAll]);

  const selectedInstance = useMemo(
    () => (isAll ? undefined : instances.find((i) => i.model_uid === modelUid)),
    [instances, modelUid, isAll],
  );

  const replicaOptions = useMemo(() => {
    const src = selectedInstance?.replica_data_source || [];
    return src
      .map((r) => String(r.replica_model_uid || ''))
      .filter(Boolean)
      .map((uid) => ({ value: uid, label: uid }));
  }, [selectedInstance]);

  useEffect(() => {
    if (!replicaUid) return;
    if (!replicaOptions.some((o) => o.value === replicaUid)) {
      setReplicaUid(undefined);
    }
  }, [replicaOptions, replicaUid]);

  const instanceOptions = [
    {
      value: ALL_MODELS,
      label: l('monitor.instances.filter.allModels'),
    },
    ...instances.map((i) => ({
      value: i.model_uid,
      label: `${i.model_name || i.model_uid} (${i.model_uid})`,
    })),
  ];

  const selectInstance = (uid: string) => {
    setModelUid(uid);
    setReplicaUid(undefined);
  };

  const benchUids = isAll ? allUids : modelUid ? [modelUid] : [];

  const benchEntry = (
    <Button
      size="small"
      type="primary"
      ghost
      icon={<FlaskConical size={14} />}
      disabled={!benchUids.length}
      onClick={() => setBenchOpen(true)}
    >
      {l('monitor.instances.bench')}
    </Button>
  );

  return (
    <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card p-3 space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="text-sm font-medium">
          {l('monitor.instances.overview.liveBoard')}
          <span className="ml-2 text-xs font-normal text-muted">
            {isAll
              ? l('monitor.instances.filter.allModels')
              : modelUid}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Select
            showSearch
            optionFilterProp="label"
            className="min-w-[240px]"
            options={instanceOptions}
            value={modelUid}
            onChange={(v) => selectInstance(v)}
            allowClear={false}
          />
          <Select
            allowClear
            showSearch
            optionFilterProp="label"
            className="min-w-[200px]"
            placeholder={l('monitor.instances.filter.allReplicas')}
            options={replicaOptions}
            value={replicaUid}
            onChange={(v) => setReplicaUid(v)}
            disabled={isAll || !replicaOptions.length}
          />
          <span className="inline-flex items-center gap-1.5 text-xs text-muted">
            <Switch size="small" checked={live} onChange={setLive} />
            {l('monitor.cluster.live')}
          </span>
        </div>
      </div>

      {isAll ? (
        <AllModelsLiveBoard
          modelUids={allUids}
          devices={devices}
          runtimesOverride={runtimes}
          performancesOverride={performances as BoardPerformances}
          onRefreshSnapshot={refreshSnap}
          headerExtra={benchEntry}
        />
      ) : (
        <div className="space-y-3">
          <InstantLoadCard
            modelUid={modelUid}
            replicaUid={replicaUid}
            devices={devices}
            runtimeOverride={runtimes != null ? runtimes[modelUid] ?? null : null}
            onRefresh={refreshSnap}
          />
          <PerformanceMetrics
            modelUid={modelUid}
            replicaUid={replicaUid}
            devices={devices}
            compact={false}
            perfOverride={
              performances != null ? performances[modelUid] ?? null : null
            }
            onRefreshCards={refreshSnap}
            headerExtra={benchEntry}
          />
        </div>
      )}

      <Drawer
        width={960}
        open={benchOpen}
        onClose={() => setBenchOpen(false)}
        title={l('monitor.instances.bench')}
        destroyOnClose
      >
        <BenchmarkPanel modelUids={benchUids} embedded />
      </Drawer>
    </div>
  );
};

export default OverviewPanel;
