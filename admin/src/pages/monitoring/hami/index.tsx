import { useEffect, useMemo, useState } from 'react';
import { history, useLocation } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Button, Drawer, Radio, Switch, Table, Tag } from 'antd';
import { Layers } from 'lucide-react';
import { EmptyState, PageContainer, PillTabs, SectionLoading } from '@/components';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import { mapClusterGpuApiRows, type GpuRow } from '@/utils/monitorMetrics';
import { useK8sRuntime } from '@/hooks/useK8sRuntime';
import { usePageVisible } from '@/hooks/usePageVisible';
import HamiHistoryPane from './HamiHistoryPane';

type HamiTab = 'pool' | 'live' | 'history' | 'status' | 'policy' | 'insight';

type Slice = {
  model_uid?: string;
  model_name?: string;
  mem_mib?: number;
  gpu_cores?: number;
  status?: string;
};

type ApiGpu = Record<string, unknown> & {
  slices?: Slice[];
  alloc_mem?: number;
  free_mem?: number;
  free_core?: number;
  product?: string;
  uuid?: string;
  capacity_ready?: boolean;
  mem_total?: number;
};

function gpuKey(row: ApiGpu): string {
  return `${row.node || row.worker_address}/${row.gpu_index}`;
}

function mibToGb(mib?: number | null): string {
  if (mib == null || Number.isNaN(Number(mib))) return '—';
  const n = Number(mib);
  if (n > 4096) return `${(n / 1024).toFixed(1)}`;
  return String(n);
}

const HamiPage = () => {
  const location = useLocation();
  const { hamiEnabled, probed, runtime } = useK8sRuntime();
  const [unresolvedOnly, setUnresolvedOnly] = useState(false);
  const pageVisible = usePageVisible();
  const params = new URLSearchParams(location.search);
  const tab = (params.get('tab') as HamiTab) || 'pool';
  const gpuQ = params.get('gpu') || '';
  const [liveOn, setLiveOn] = useState(true);
  const [filter, setFilter] = useState<'all' | 'free' | 'full' | 'alert'>('all');
  const [drawerKey, setDrawerKey] = useState<string | null>(gpuQ || null);

  useEffect(() => {
    if (probed && !hamiEnabled) {
      history.replace('/monitor/cluster');
    }
  }, [probed, hamiEnabled]);

  const setTab = (next: HamiTab) => {
    const p = new URLSearchParams(location.search);
    if (next === 'pool') p.delete('tab');
    else p.set('tab', next);
    history.replace({ pathname: '/monitor/hami', search: p.toString() || undefined });
  };

  const { data: gpuRaw, loading } = useRequest(
    async () => {
      const all: ApiGpu[] = [];
      let offset = 0;
      for (let i = 0; i < 3; i += 1) {
        const res = await request<{
          data: { data: { next_offset: number | null; results: ApiGpu[] } };
        }>('/monitor/cluster/gpus', { params: { offset, limit: 200 } });
        const page = res?.data?.data;
        all.push(...(page?.results || []));
        if (page?.next_offset == null) break;
        offset = page.next_offset;
      }
      return all;
    },
    {
      ready: probed && hamiEnabled && pageVisible,
      pollingInterval: liveOn && (tab === 'pool' || tab === 'live') ? 8000 : 0,
      pollingWhenHidden: false,
    },
  );

  const { data: statusData } = useRequest(
    () => request<{ data: { data: Record<string, unknown> } }>('/monitor/hami/status'),
    { ready: probed && hamiEnabled && tab === 'status' },
  );
  const { data: eventsData } = useRequest(
    () => request<{ data: { data: { results: Array<Record<string, unknown>> } } }>(
      '/monitor/hami/events',
    ),
    { ready: probed && hamiEnabled && tab === 'status' },
  );
  const { data: insightData } = useRequest(
    () =>
      request<{ data: { data: { results: Array<Record<string, unknown>> } } }>(
        '/monitor/hami/insight',
      ),
    { ready: probed && hamiEnabled && tab === 'insight' },
  );
  const { data: policyData, refresh: refreshPolicy } = useRequest(
    () => request<{ data: { data: { policy: string } } }>('/monitor/hami/policy'),
    { ready: probed && hamiEnabled && tab === 'policy' },
  );

  const rows = gpuRaw || [];
  const mapped: GpuRow[] = useMemo(() => mapClusterGpuApiRows(rows), [rows]);
  const capacityReady = rows.some((r) => r.capacity_ready);
  const kpi = useMemo(() => {
    const freeMem = rows.reduce((s, r) => s + Number(r.free_mem || 0), 0);
    const freeCore = rows.reduce((s, r) => s + Number(r.free_core || 0), 0);
    const slices = rows.reduce((s, r) => s + (r.slices || []).length, 0);
    return {
      physical: rows.length,
      freeMemGb: capacityReady ? (freeMem / 1024).toFixed(1) : '—',
      freeCore: capacityReady ? String(freeCore) : '—',
      slices,
    };
  }, [rows, capacityReady]);

  const filtered = useMemo(() => {
    return rows.filter((r) => {
      const alloc = Number(r.alloc_mem || 0);
      const free = Number(r.free_mem || 0);
      const total = alloc + free;
      if (filter === 'free') return free > 0 && (total === 0 || free / (total || 1) > 0.05);
      if (filter === 'full') return total > 0 && free / total <= 0.05;
      if (filter === 'alert') return (r.status as string) === 'warn' || (r.status as string) === 'crit';
      return true;
    });
  }, [rows, filter]);

  const grouped = useMemo(() => {
    const map = new Map<string, ApiGpu[]>();
    for (const r of filtered) {
      const key = String(r.node || r.worker_address || '—');
      const list = map.get(key) || [];
      list.push(r);
      map.set(key, list);
    }
    return Array.from(map.entries());
  }, [filtered]);

  const drawerRow = rows.find((r) => gpuKey(r) === drawerKey) || null;
  const liveMapped = useMemo(() => {
    if (!gpuQ) return mapped;
    return mapped.filter(
      (r) => `${r.node}/${r.idx}` === gpuQ || String(r.idx) === gpuQ.split('/').pop(),
    );
  }, [mapped, gpuQ]);
  const liveSlices = useMemo(() => {
    const src = gpuQ ? rows.filter((r) => gpuKey(r) === gpuQ) : rows;
    return src.flatMap((r) => {
      const usedRaw = Number(r.mem_used || 0);
      const usedMib = usedRaw > 10000 ? usedRaw / (1024 * 1024) : usedRaw;
      return (r.slices || []).map((s) => ({
        ...s,
        gpu_index: r.gpu_index,
        used_mib: usedMib,
      }));
    });
  }, [rows, gpuQ]);
  const eventRows = useMemo(() => {
    const raw =
      (eventsData?.data?.data?.results as Array<Record<string, unknown>>) || [];
    return raw
      .filter((r) => !unresolvedOnly || !r.resolved)
      .map((r) => {
        const t = String(r.type || '');
        let hint = '';
        if (t === 'oom') hint = lGet('monitor.hami.event.oom');
        else if (t === 'resource') hint = lGet('monitor.hami.event.resource');
        else if (t === 'type_mismatch') hint = lGet('monitor.hami.event.type');
        else if (t === 'capacity') hint = lGet('monitor.hami.event.capacity');
        return { ...r, hint };
      });
  }, [eventsData, unresolvedOnly]);
  const statusNodes = useMemo(() => {
    const raw =
      (statusData?.data?.data?.nodes as Array<Record<string, unknown>>) || [];
    return raw.map((n) => {
      const name = String(n.name || '');
      const gpus = rows.filter(
        (r) => String(r.node || r.worker_address || '') === name,
      );
      const alloc = gpus.reduce((s, r) => s + Number(r.alloc_mem || 0), 0);
      const free = gpus.length
        ? gpus.reduce((s, r) => s + Number(r.free_mem || 0), 0)
        : Number(n.gpumem || 0);
      return {
        ...n,
        gpu_count: n.gpu ?? gpus.length,
        alloc_mem: alloc,
        free_mem: free,
      };
    });
  }, [statusData, rows]);
  const historyQuotaGb = (() => {
    const hit = gpuQ ? rows.find((r) => gpuKey(r) === gpuQ) : rows[0];
    const mib = Number(hit?.alloc_mem || 0);
    return mib > 0 ? mib / 1024 : undefined;
  })();

  if (!probed || !hamiEnabled) {
    return <SectionLoading />;
  }
  if (loading && !gpuRaw) {
    return (
      <PageContainer title={l('menu.monitor.hami')}>
        <SectionLoading />
      </PageContainer>
    );
  }

  const chip = (label: string, value: string) => (
    <div className="rounded-md border border-border/40 bg-card px-2.5 py-1.5">
      <div className="text-[12px] text-muted">{label}</div>
      <div className="font-mono text-sm font-semibold tabular-nums text-default">{value}</div>
    </div>
  );

  return (
    <PageContainer
      title={l('menu.monitor.hami')}
      subTitle={l('monitor.hami.subTitle')}
    >
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <PillTabs
            aria-label="GPU slice tabs"
            value={tab}
            options={[
              { value: 'pool', label: l('monitor.hami.tab.pool') },
              { value: 'live', label: l('monitor.hami.tab.live') },
              { value: 'history', label: l('monitor.hami.tab.history') },
              { value: 'status', label: l('monitor.hami.tab.status') },
              { value: 'policy', label: l('monitor.hami.tab.policy') },
              { value: 'insight', label: l('monitor.hami.tab.insight') },
            ]}
            onChange={(key) => setTab(key as HamiTab)}
          />
          {(tab === 'pool' || tab === 'live') && (
            <label className="inline-flex items-center gap-2 text-xs text-muted">
              Live
              <Switch size="small" checked={liveOn} onChange={setLiveOn} />
            </label>
          )}
        </div>

        {tab === 'pool' && (
          <>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              {chip(l('monitor.hami.kpi.physical'), String(kpi.physical))}
              {chip(l('monitor.hami.kpi.freeMem'), `${kpi.freeMemGb} GB`)}
              {chip(l('monitor.hami.kpi.freeCore'), String(kpi.freeCore))}
              {chip(l('monitor.hami.kpi.slices'), String(kpi.slices))}
            </div>
            {!capacityReady && rows.length > 0 ? (
              <div className="rounded-md border border-border/40 bg-card px-3 py-2 text-sm text-muted">
                {l('monitor.hami.capacityNone')}
                <Button type="link" size="small" onClick={() => setTab('status')}>
                  {l('monitor.hami.tab.status')}
                </Button>
              </div>
            ) : null}
            <div className="flex flex-wrap gap-2">
              {(['all', 'free', 'full', 'alert'] as const).map((k) => (
                <Button
                  key={k}
                  size="small"
                  type={filter === k ? 'primary' : 'default'}
                  onClick={() => setFilter(k)}
                >
                  {l(`monitor.hami.filter.${k}`, k)}
                </Button>
              ))}
            </div>
            {rows.length === 0 ? (
              <EmptyState
                customIcon={<Layers size={48} className="text-muted" />}
                title={l('monitor.hami.empty')}
                description={l('monitor.hami.emptyHint')}
              />
            ) : (
              grouped.map(([node, list]) => (
                <section key={node} className="flex flex-col gap-2">
                  <h2 className="text-xs font-medium text-muted">{node}</h2>
                  <div className="grid gap-2 md:grid-cols-2">
                    {list.map((row) => {
                      const alloc = Number(row.alloc_mem || 0);
                      const free = Number(row.free_mem || 0);
                      const total = alloc + free || 1;
                      const pct = Math.min(100, (alloc / total) * 100);
                      const low = free / total < 0.2 && alloc > 0;
                      const idle = alloc === 0;
                      return (
                        <article
                          key={String(row.id)}
                          className={`cursor-pointer rounded-lg border bg-card p-3 ${
                            low
                              ? 'border-l-[3px] border-l-destructive'
                              : idle
                                ? 'border-l-[3px] border-l-success'
                                : 'border-[color:var(--c-border-light)]'
                          }`}
                          onClick={() => setDrawerKey(gpuKey(row))}
                        >
                          <div className="mb-2 flex items-start justify-between gap-2">
                            <div className="min-w-0">
                              <div className="truncate text-sm font-medium">
                                GPU {String(row.gpu_index)} · {String(row.product || row.name || '')}
                              </div>
                              <div className="font-mono text-xs text-muted tabular-nums">
                                {mibToGb(alloc)}/{mibToGb(alloc + free)} GB · Core{' '}
                                {row.alloc_core ?? 0}/{row.free_core ?? 100}
                              </div>
                            </div>
                            <Button
                              type="link"
                              size="small"
                              className="px-0"
                              onClick={(e) => {
                                e.stopPropagation();
                                const p = new URLSearchParams();
                                p.set('tab', 'live');
                                p.set('gpu', gpuKey(row));
                                history.replace({ pathname: '/monitor/hami', search: p.toString() });
                              }}
                            >
                              {l('monitor.hami.viewLive')}
                            </Button>
                          </div>
                          <div className="h-2 overflow-hidden rounded-full bg-[color:var(--c-border-light)]">
                            <div
                              className="h-full rounded-full bg-primary/70"
                              style={{ width: `${pct}%` }}
                            />
                          </div>
                          <div className="mt-2 flex flex-wrap gap-1 text-xs text-muted">
                            {idle
                              ? l('monitor.hami.idleCard')
                              : (row.slices || []).map((s) => (
                                  <button
                                    key={s.model_uid}
                                    type="button"
                                    className="text-primary hover:underline"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      if (s.model_uid) {
                                        history.push(
                                          `/monitor/instances?id=${encodeURIComponent(s.model_uid)}`,
                                        );
                                      }
                                    }}
                                  >
                                    {s.model_name || s.model_uid}
                                  </button>
                                ))}
                          </div>
                        </article>
                      );
                    })}
                  </div>
                </section>
              ))
            )}
          </>
        )}

        {tab === 'live' && (
          <div className="flex flex-col gap-3">
            <Table
              size="small"
              rowKey={(r) => String(r.id)}
              dataSource={liveMapped}
              pagination={false}
              columns={[
                { title: 'GPU', dataIndex: 'idx', width: 64 },
                { title: 'Node', dataIndex: 'node' },
                {
                  title: 'Util',
                  render: (_, r) => (r.util == null ? '—' : `${r.util}%`),
                },
                {
                  title: 'Mem',
                  render: (_, r) => `${r.memUsedGb}/${r.memTotalGb}G`,
                },
                {
                  title: 'Temp',
                  render: (_, r) =>
                    r.temperatureC == null ? '—' : `${Math.round(r.temperatureC)}°C`,
                },
                {
                  title: 'Power',
                  render: (_, r) => (r.powerW == null ? '—' : `${Math.round(r.powerW)}W`),
                },
              ]}
            />
            <Table
              size="small"
              rowKey={(r) => `${r.model_uid}-${r.gpu_index}`}
              dataSource={liveSlices}
              pagination={false}
              columns={[
                { title: l('monitor.hami.slice.model'), dataIndex: 'model_name' },
                {
                  title: lGet('monitor.hami.slice.limit'),
                  render: (_, s) => (s.mem_mib ? `${mibToGb(s.mem_mib)} GB` : '—'),
                },
                {
                  title: lGet('monitor.hami.slice.used'),
                  render: (_, s) => (s.used_mib ? `${mibToGb(s.used_mib)} GB` : '—'),
                },
                { title: 'Core', dataIndex: 'gpu_cores', width: 72 },
                {
                  title: '',
                  render: (_, s) =>
                    s.model_uid ? (
                      <Button
                        type="link"
                        size="small"
                        onClick={() =>
                          history.push(
                            `/monitor/instances?id=${encodeURIComponent(String(s.model_uid))}`,
                          )
                        }
                      >
                        {lGet('monitor.hami.openInstance')}
                      </Button>
                    ) : null,
                },
              ]}
            />
          </div>
        )}

        {tab === 'history' && (
          <HamiHistoryPane quotaGb={historyQuotaGb} gpuIndex={gpuQ} />
        )}

        {tab === 'status' && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-2">
              {((statusData?.data?.data?.components as Array<Record<string, string>>) || []).map(
                (c) => (
                  <Tag
                    key={c.id}
                    color={c.state === 'running' ? 'success' : c.state === 'error' ? 'error' : 'default'}
                  >
                    {c.id}: {c.state}
                  </Tag>
                ),
              )}
              {runtime?.gpu_mode ? <Tag>mode {runtime.gpu_mode}</Tag> : null}
            </div>
            <Table
              size="small"
              rowKey={(r) => String(r.name)}
              dataSource={statusNodes}
              pagination={false}
              columns={[
                { title: lGet('monitor.hami.node'), dataIndex: 'name' },
                {
                  title: lGet('monitor.hami.node.gpus'),
                  dataIndex: 'gpu_count',
                  width: 80,
                },
                {
                  title: lGet('monitor.hami.alloc'),
                  render: (_, n) =>
                    n.alloc_mem != null ? `${mibToGb(Number(n.alloc_mem))} GB` : '—',
                },
                {
                  title: lGet('monitor.hami.free'),
                  render: (_, n) =>
                    n.free_mem != null ? `${mibToGb(Number(n.free_mem))} GB` : '—',
                },
                {
                  title: lGet('monitor.hami.node.ready'),
                  render: (_, n) =>
                    n.capacity_ready
                      ? lGet('monitor.hami.node.readyOk')
                      : lGet('monitor.hami.node.readyNo'),
                },
              ]}
            />
            <div>
              <Button
                size="small"
                type={unresolvedOnly ? 'primary' : 'default'}
                onClick={() => setUnresolvedOnly((v) => !v)}
              >
                {lGet('monitor.hami.event.unresolved')}
              </Button>
            </div>
            <Table
              size="small"
              rowKey={(r) => String(r.model_uid || r.summary)}
              dataSource={eventRows}
              pagination={false}
              columns={[
                { title: 'Type', dataIndex: 'type', width: 120 },
                { title: 'Model', dataIndex: 'model_uid' },
                { title: 'Summary', dataIndex: 'summary', ellipsis: true },
                {
                  title: lGet('monitor.hami.event.hint'),
                  dataIndex: 'hint',
                  ellipsis: true,
                },
                {
                  title: '',
                  render: (_, r) =>
                    r.model_uid ? (
                      <Button
                        type="link"
                        size="small"
                        onClick={() =>
                          history.push(
                            `/models/instances?id=${encodeURIComponent(String(r.model_uid))}`,
                          )
                        }
                      >
                        {r.type === 'oom'
                          ? lGet('monitor.hami.event.redeploy')
                          : lGet('monitor.hami.openInstance')}
                      </Button>
                    ) : null,
                },
              ]}
            />
          </div>
        )}

        {tab === 'policy' && (
          <PolicyPane
            value={(policyData?.data?.data?.policy as string) || 'auto'}
            onSaved={refreshPolicy}
          />
        )}

        {tab === 'insight' && (
          <Table
            size="small"
            rowKey={(r) => String(r.id)}
            dataSource={(insightData?.data?.data?.results as Array<Record<string, unknown>>) || []}
            pagination={false}
            columns={[
              { title: 'GPU', dataIndex: 'gpu_index', width: 64 },
              { title: 'Alloc', dataIndex: 'alloc_mem' },
              { title: 'Used', dataIndex: 'used_mem' },
              { title: 'Verdict', dataIndex: 'verdict' },
              {
                title: '',
                render: (_, r) =>
                  r.verdict === 'oversold' && r.suggest_mem_mib ? (
                    <span className="text-xs text-muted">
                      {lGet('monitor.hami.suggest')} {String(r.suggest_mem_mib)} MiB
                    </span>
                  ) : null,
              },
            ]}
          />
        )}
      </div>

      <Drawer
        open={Boolean(drawerRow)}
        width={420}
        title={
          drawerRow
            ? `GPU ${drawerRow.gpu_index} · ${drawerRow.product || drawerRow.name || ''}`
            : ''
        }
        onClose={() => setDrawerKey(null)}
      >
        {drawerRow ? (
          <div className="flex flex-col gap-3 text-sm">
            <div>UUID {String(drawerRow.uuid || '—')}</div>
            <div>
              {lGet('monitor.hami.alloc')} {mibToGb(Number(drawerRow.alloc_mem || 0))} GB /{' '}
              {lGet('monitor.hami.free')} {mibToGb(Number(drawerRow.free_mem || 0))} GB
            </div>
            <Table
              size="small"
              pagination={false}
              rowKey={(s) => String(s.model_uid)}
              dataSource={drawerRow.slices || []}
              columns={[
                { title: l('monitor.hami.slice.model'), dataIndex: 'model_name' },
                {
                  title: 'Mem',
                  render: (_, s) => (s.mem_mib ? `${mibToGb(s.mem_mib)} GB` : '—'),
                },
                { title: 'Core', dataIndex: 'gpu_cores' },
              ]}
              locale={{
                emptyText: l('monitor.hami.drawer.empty',
                ),
              }}
            />
            {!(drawerRow.slices || []).length ? (
              <Button type="default" onClick={() => history.push('/models/repository')}>
                {l('monitor.hami.goDeploy')}
              </Button>
            ) : null}
            <div className="flex gap-2">
              <Button
                onClick={() => {
                  const p = new URLSearchParams();
                  p.set('tab', 'live');
                  p.set('gpu', gpuKey(drawerRow));
                  history.replace({ pathname: '/monitor/hami', search: p.toString() });
                  setDrawerKey(null);
                }}
              >
                {l('monitor.hami.viewLive')}
              </Button>
              <Button
                onClick={() => {
                  const p = new URLSearchParams();
                  p.set('tab', 'history');
                  p.set('gpu', gpuKey(drawerRow));
                  history.replace({ pathname: '/monitor/hami', search: p.toString() });
                  setDrawerKey(null);
                }}
              >
                {l('monitor.hami.viewHistory')}
              </Button>
            </div>
          </div>
        ) : null}
      </Drawer>
    </PageContainer>
  );
};

function PolicyPane({ value, onSaved }: { value: string; onSaved: () => void }) {
  const [policy, setPolicy] = useState(value);
  return (
    <div className="flex max-w-xl flex-col gap-3">
      <Radio.Group value={policy} onChange={(e) => setPolicy(e.target.value)}>
        <div className="flex flex-col gap-2">
          <Radio value="auto">
            {lGet('monitor.hami.policy.auto')}
          </Radio>
          <Radio value="binpack">
            {lGet('monitor.hami.policy.binpack')}
          </Radio>
          <Radio value="spread">
            {lGet('monitor.hami.policy.spread')}
          </Radio>
        </div>
      </Radio.Group>
      <Button
        type="primary"
        onClick={async () => {
          await request('/monitor/hami/policy', { method: 'put', data: { policy } });
          onSaved();
        }}
      >
        {lGet('global.actions.save')}
      </Button>
    </div>
  );
}

export default HamiPage;
