import { useCallback, useEffect, useMemo, useState } from 'react';
import { history, useLocation } from '@umijs/max';
import { useRequest } from 'ahooks';
import {
  Button,
  Drawer,
  Dropdown,
  Input,
  Select,
  Table,
  Tag,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { MenuProps } from 'antd';
import {
  Download,
  ExternalLink,
  RefreshCw,
  ScrollText,
  Network,
  MoreHorizontal,
  HeartPulse,
  FileSearch,
  GitCompareArrows,
} from 'lucide-react';
import dayjs from 'dayjs';
import { IconButton, PageContainer } from '@/components';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import { IntanceStatus, INTANCE_STATUS_COLOR } from '@/constants/intance';
import type { ModelsInstancesListItem } from '@/types/Public/data';
import { formatDisplayTime } from '@/utils';
import { readRequestFailure } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import { downloadCsv } from '@/utils/downloadCsv';
import InstanceLogDrawer from '@/pages/management/instance/components/InstanceLogDrawer';
import ComparePanel from './components/ComparePanel';
import ServiceStatsPanel from './components/ServiceStatsPanel';
import PerformanceMetrics from './components/PerformanceMetrics';
import UsageDailyTotals from './components/UsageDailyTotals';
import TraceList from '../traces/TraceList';

type Row = ModelsInstancesListItem & {
  last_probe_ok?: boolean | null;
  last_probe_at?: number | null;
  last_probe_message?: string | null;
};

type Props = {
  /** 嵌入「模型统计」Tab 时不包 PageContainer */
  embedded?: boolean;
};

/** 数据总览：服务统计 + 实例表；操作收拢为「⋯」菜单 */
const InstanceOpsPanel = ({ embedded = false }: Props) => {
  const location = useLocation();
  const [keyword, setKeyword] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [selected, setSelected] = useState<Row | null>(null);
  const [queryOpen, setQueryOpen] = useState(false);
  const [logOpen, setLogOpen] = useState(false);
  const [tracesOpen, setTracesOpen] = useState(false);
  const [compareOpen, setCompareOpen] = useState(false);
  const [probingUid, setProbingUid] = useState<string | null>(null);

  const idFromUrl = useMemo(
    () => new URLSearchParams(location.search).get('id') || undefined,
    [location.search],
  );

  const {
    data: listRes,
    loading,
    refresh,
  } = useRequest(() =>
    request<{ data: { count?: number; results?: Row[] } }>('/models/instances', {
      params: ALL_LIST_PAGES_PARAMS,
    }),
  );

  const { data: probeMapRes, refresh: refreshProbes } = useRequest(
    () =>
      request<{ data: Record<string, { ok: boolean; at: number; message?: string }> }>(
        '/monitor/probes',
      ),
    { onError: () => undefined },
  );

  const instances: Row[] = useMemo(() => {
    const raw = Array.isArray(listRes?.data?.results)
      ? listRes.data.results
      : Array.isArray(listRes?.data)
        ? (listRes.data as Row[])
        : [];
    const probes =
      (probeMapRes?.data?.data as unknown as Record<
        string,
        { ok: boolean; at: number; message?: string }
      >) ||
      (probeMapRes?.data as unknown as Record<
        string,
        { ok: boolean; at: number; message?: string }
      >) ||
      {};
    return raw.map((item) => {
      const p = probes[item.model_uid];
      return {
        ...item,
        last_probe_ok: p ? p.ok : null,
        last_probe_at: p?.at ?? null,
        last_probe_message: p?.message ?? null,
      };
    });
  }, [listRes, probeMapRes]);

  const filtered = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    return instances.filter((r) => {
      if (statusFilter !== 'all' && r.status !== statusFilter) return false;
      if (!q) return true;
      return (
        r.model_uid?.toLowerCase().includes(q) ||
        r.model_name?.toLowerCase().includes(q) ||
        String(r.model_type || '')
          .toLowerCase()
          .includes(q)
      );
    });
  }, [instances, keyword, statusFilter]);

  useEffect(() => {
    if (!idFromUrl) return;
    const hit = instances.find((i) => i.model_uid === idFromUrl);
    if (hit) {
      setSelected(hit);
      setQueryOpen(true);
    }
  }, [idFromUrl, instances]);

  // 探活刷新后同步抽屉内选中行的 probe 字段
  useEffect(() => {
    if (!selected) return;
    const hit = instances.find((i) => i.model_uid === selected.model_uid);
    if (hit) setSelected(hit);
  }, [instances, selected?.model_uid]);

  const syncUrlId = useCallback(
    (modelUid?: string) => {
      const params = new URLSearchParams(location.search);
      params.delete('tab');
      if (modelUid) params.set('id', modelUid);
      else params.delete('id');
      const search = params.toString();
      history.replace({
        pathname: '/monitor/instances',
        search: search || undefined,
      });
    },
    [location.search],
  );

  const openQuery = useCallback(
    (row: Row) => {
      setSelected(row);
      setLogOpen(false);
      setTracesOpen(false);
      setCompareOpen(false);
      setQueryOpen(true);
      syncUrlId(row.model_uid);
    },
    [syncUrlId],
  );

  /** 菜单点击后直接探活，不打开抽屉 */
  const runProbe = useCallback(
    async (row: Row) => {
      if (probingUid) return;
      setProbingUid(row.model_uid);
      const tipKey = `probe-${row.model_uid}`;
      message.loading({
        content: String(lGet('monitor.instances.probe.running')),
        key: tipKey,
        duration: 0,
      });
      try {
        const res = await request(`/models/${row.model_uid}/probe`, {
          method: 'POST',
          data: {},
          skipNotification: true,
        });
        const body = res?.data?.data || res?.data || {};
        const ok = body?.ok ?? body?.success;
        const detail =
          body?.message ||
          body?.detail ||
          (ok === false
            ? String(lGet('monitor.instances.probe.fail'))
            : String(lGet('monitor.instances.probe.ok')));
        if (ok === false) {
          message.error({
            content: `${String(lGet('monitor.instances.probe'))}: ${detail}`,
            key: tipKey,
          });
        } else {
          message.success({
            content: `${String(lGet('monitor.instances.probe'))}: ${detail}`,
            key: tipKey,
          });
        }
        refreshProbes();
        refresh();
      } catch (e: unknown) {
        message.error({
          content:
            readRequestFailure(e) ||
            String(lGet('monitor.instances.probe.fail')),
          key: tipKey,
        });
        refreshProbes();
      } finally {
        setProbingUid(null);
      }
    },
    [probingUid, refreshProbes, refresh],
  );

  const openCompare = useCallback((row: Row) => {
    setSelected(row);
    setQueryOpen(false);
    setLogOpen(false);
    setTracesOpen(false);
    setCompareOpen(true);
  }, []);

  const openLogs = useCallback((row: Row) => {
    setSelected(row);
    setQueryOpen(false);
    setTracesOpen(false);
    setCompareOpen(false);
    setLogOpen(true);
  }, []);

  const openTraces = useCallback((row: Row) => {
    setSelected(row);
    setQueryOpen(false);
    setLogOpen(false);
    setCompareOpen(false);
    setTracesOpen(true);
  }, []);

  const closeQuery = () => {
    setQueryOpen(false);
    if (!logOpen && !tracesOpen && !compareOpen) {
      setSelected(null);
      syncUrlId();
    }
  };

  const rowActionMenu = useCallback(
    (row: Row): MenuProps => ({
      items: [
        {
          key: 'detail',
          icon: <FileSearch size={14} />,
          label: l('global.actions.detail'),
          onClick: () => openQuery(row),
        },
        {
          key: 'probe',
          icon: <HeartPulse size={14} />,
          label:
            probingUid === row.model_uid
              ? l('monitor.instances.probe.running')
              : l('monitor.instances.probe'),
          disabled: probingUid === row.model_uid,
          onClick: () => runProbe(row),
        },
        {
          key: 'logs',
          icon: <ScrollText size={14} />,
          label: l('monitor.instances.logs'),
          onClick: () => openLogs(row),
        },
        {
          key: 'traces',
          icon: <Network size={14} />,
          label: l('monitor.instances.traces'),
          onClick: () => openTraces(row),
        },
        {
          key: 'compare',
          icon: <GitCompareArrows size={14} />,
          label: l('monitor.instances.compare.run'),
          onClick: () => openCompare(row),
        },
      ],
    }),
    [openQuery, runProbe, openLogs, openTraces, openCompare, probingUid],
  );

  const exportCsv = () => {
    if (!filtered.length) {
      message.info(String(lGet('global.data.empty')));
      return;
    }
    downloadCsv(
      `monitor-instances-${dayjs().format('YYYYMMDD-HHmmss')}.csv`,
      ['model_uid', 'model_name', 'model_type', 'status', 'replica', 'probe_ok', 'probe_at'],
      filtered.map((r) => [
        r.model_uid,
        r.model_name,
        r.model_type,
        r.status,
        r.replica,
        r.last_probe_ok == null ? '' : r.last_probe_ok ? 'ok' : 'fail',
        r.last_probe_at ? formatDisplayTime(r.last_probe_at) : '',
      ]),
    );
  };

  const columns: ColumnsType<Row> = [
    {
      title: l('models.instances.modelUid'),
      dataIndex: 'model_uid',
      ellipsis: true,
    },
    {
      title: l('models.instances.modelName'),
      dataIndex: 'model_name',
      ellipsis: true,
    },
    {
      title: l('models.instances.status'),
      dataIndex: 'status',
      width: 120,
      render: (st: string) => (
        <Tag color={INTANCE_STATUS_COLOR[st as IntanceStatus] || 'default'}>{st}</Tag>
      ),
    },
    {
      title: l('monitor.instances.probe'),
      key: 'probe',
      width: 140,
      render: (_, row) => {
        if (row.last_probe_ok == null) {
          return <span className="text-muted text-xs">—</span>;
        }
        return (
          <span className="text-xs">
            <Tag color={row.last_probe_ok ? 'success' : 'error'}>
              {row.last_probe_ok
                ? l('monitor.instances.probe.ok')
                : l('monitor.instances.probe.fail')}
            </Tag>
            {row.last_probe_at
              ? formatDisplayTime(row.last_probe_at, 'HH:mm:ss')
              : null}
          </span>
        );
      },
    },
    {
      title: l('models.instances.replica'),
      dataIndex: 'replica',
      width: 72,
    },
    {
      title: l('global.actions.action'),
      key: 'actions',
      width: 64,
      render: (_, row) => (
        <div onClick={(e) => e.stopPropagation()}>
          <Dropdown menu={rowActionMenu(row)} trigger={['click']} placement="bottomRight">
            <IconButton
              className="!w-7 !h-7"
              aria-label={String(lGet('models.repository.moreActions'))}
            >
              <MoreHorizontal size={16} />
            </IconButton>
          </Dropdown>
        </div>
      ),
    },
  ];

  const logNodeOptions = useMemo(() => {
    const reps = selected?.replica_data_source || [];
    return reps.map((r) => ({
      label: r.worker_address || r.replica_model_uid || 'node',
      value: r.worker_address || '',
    }));
  }, [selected]);

  const logReplicaUidOptions = useMemo(() => {
    const reps = selected?.replica_data_source || [];
    return reps
      .map((r) => r.replica_model_uid)
      .filter(Boolean)
      .map((uid) => ({ label: String(uid), value: String(uid) }));
  }, [selected]);

  const toolbar = (
    <div className="flex items-center gap-2">
      <Button size="small" icon={<Download size={14} />} onClick={exportCsv}>
        {l('monitor.platform.downloadCsv')}
      </Button>
      <Button
        size="small"
        icon={<RefreshCw size={14} />}
        onClick={() => {
          refresh();
          refreshProbes();
        }}
      >
        {l('global.actions.refresh')}
      </Button>
    </div>
  );

  const body = (
    <>
      <div className="space-y-4">
        <ServiceStatsPanel />

        <div className="flex flex-wrap items-center gap-2">
          <Select
            size="small"
            className="min-w-[120px]"
            value={statusFilter}
            onChange={setStatusFilter}
            options={[
              { value: 'all', label: l('monitor.platform.filter.all') },
              ...Object.values(IntanceStatus).map((s) => ({ value: s, label: s })),
            ]}
          />
          <Input
            allowClear
            size="small"
            className="w-[200px]"
            placeholder={l('monitor.instances.filter')}
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
          />
          <Button
            size="small"
            icon={<ExternalLink size={14} />}
            onClick={() => history.push('/models/instances')}
          >
            {l('monitor.instances.toManage')}
          </Button>
          {embedded ? <div className="ml-auto">{toolbar}</div> : null}
        </div>

        <Table<Row>
          size="small"
          rowKey="model_uid"
          loading={loading}
          columns={columns}
          dataSource={filtered}
          pagination={{ pageSize: 20, showSizeChanger: true }}
          onRow={(row) => ({
            onClick: (e) => {
              const t = e.target as HTMLElement | null;
              if (t?.closest?.('button, a, .ant-btn, .ant-tag')) return;
              openQuery(row);
            },
            className: 'cursor-pointer',
          })}
          rowClassName={(row) =>
            [
              'cursor-pointer',
              row.status === IntanceStatus.ERROR ? 'bg-danger/5' : '',
            ]
              .filter(Boolean)
              .join(' ')
          }
        />
      </div>

      <Drawer
        width={560}
        open={queryOpen && !!selected && !logOpen && !tracesOpen && !compareOpen}
        onClose={closeQuery}
        title={
          selected
            ? l('monitor.instances.query.title30d', {
                uid: selected.model_uid,
              })
            : l('global.actions.detail')
        }
        destroyOnClose
      >
        {selected && (
          <div className="space-y-4">
            <div className="flex flex-wrap gap-2">
              <Tag color={INTANCE_STATUS_COLOR[selected.status as IntanceStatus]}>
                {selected.status}
              </Tag>
              <Button
                size="small"
                icon={<ExternalLink size={12} />}
                onClick={() =>
                  history.push(
                    `/models/instances?id=${encodeURIComponent(selected.model_uid)}`,
                  )
                }
              >
                {l('monitor.instances.toManage')}
              </Button>
            </div>
            <UsageDailyTotals modelUid={selected.model_uid} days={30} />
            <PerformanceMetrics
              modelUid={selected.model_uid}
              poll={false}
              compact
              windowSeconds={30 * 86400}
              title="近 30 天性能指标"
            />
          </div>
        )}
      </Drawer>

      <Drawer
        width={960}
        open={tracesOpen && !!selected}
        onClose={() => {
          setTracesOpen(false);
          if (!queryOpen && !logOpen && !compareOpen) setSelected(null);
        }}
        title={
          selected
            ? `${l('monitor.instances.traces')} · ${selected.model_uid}`
            : l('monitor.instances.traces')
        }
        destroyOnClose
      >
        {selected ? <TraceList modelUid={selected.model_uid} /> : null}
      </Drawer>

      <Drawer
        width={720}
        open={compareOpen && !!selected}
        onClose={() => {
          setCompareOpen(false);
          if (!queryOpen && !logOpen && !tracesOpen) setSelected(null);
        }}
        title={l('monitor.instances.compare.title')}
        destroyOnClose
      >
        <ComparePanel defaultA={selected?.model_uid} hideTitle />
      </Drawer>

      {selected && (
        <InstanceLogDrawer
          open={logOpen}
          onClose={() => {
            setLogOpen(false);
            if (!queryOpen && !tracesOpen && !compareOpen) setSelected(null);
          }}
          modelUid={selected.model_uid}
          replicaOptions={logNodeOptions}
          replicaUidOptions={logReplicaUidOptions}
          workerAddress={selected.replica_data_source?.[0]?.worker_address}
        />
      )}
    </>
  );

  if (embedded) {
    return body;
  }

  return (
    <PageContainer
      title={l('monitor.instances.tab.perf')}
      subTitle={l('monitor.instances.perf.subTitle',
      )}
      extraContent={toolbar}
    >
      {body}
    </PageContainer>
  );
};

export default InstanceOpsPanel;
