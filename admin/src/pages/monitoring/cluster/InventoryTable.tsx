import type { ReactNode } from 'react';
import { Table, Tag, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ProCard } from '@ant-design/pro-components';
import dayjs from 'dayjs';
import { l } from '@/utils/intl';

export type InventoryNic = {
  name: string;
  speed_gbps?: number | null;
  reason?: string | null;
};

export type InventoryDisk = {
  label?: string;
  path?: string;
  total?: number | null;
  used?: number | null;
  free?: number | null;
  percent?: number | null;
  reason?: string | null;
};

export type InventoryRow = {
  worker_address: string;
  name?: string;
  online?: boolean;
  accelerator_kind?: string;
  cpu?: { model?: string | null; arch?: string | null; cores?: number | null; reason?: string | null };
  mem?: { used?: number | null; total?: number | null };
  disk?: InventoryDisk[];
  nics?: { items?: InventoryNic[]; reason?: string | null };
  ib?: {
    items?: Array<{ device?: string; port?: string; rate?: string | null; reason?: string | null }>;
    reason?: string | null;
  };
  nvlink?: {
    present?: boolean;
    active_links?: number;
    rated?: string | null;
    reason?: string | null;
  };
  gpu?: { models?: Array<{ name?: string; count?: number; mem_total?: number | null }>; count?: number };
  inter?: {
    single_node?: boolean;
    reason?: string | null;
    gpu_comm?: { rated?: string | null; reason?: string | null };
  };
  last_bench?: {
    scope?: string;
    bandwidth?: string | null;
    reason?: string | null;
    finished_at?: number | null;
  } | null;
};

type Props = {
  rows: InventoryRow[];
  loading?: boolean;
  toolbar?: ReactNode;
};

const REASON_KEYS: Record<string, [string, string]> = {
  '无 NVLink': ['monitor.cluster.inventory.reason.noNvlink', '无 NVLink'],
  '无 NVML': ['monitor.cluster.inventory.reason.noNvml', '无 NVML'],
  '不适用': ['monitor.cluster.inventory.reason.na', '不适用'],
  'no sysfs speed': ['monitor.cluster.inventory.reason.noSpeed', '速率未知'],
  'no sysfs': ['monitor.cluster.inventory.reason.noSysfs', '无法读取网卡'],
  'no physical NICs': ['monitor.cluster.inventory.reason.noNics', '无物理网口'],
  '无 IB 设备': ['monitor.cluster.inventory.reason.noIb', '无 IB'],
  'no sysfs rate': ['monitor.cluster.inventory.reason.noIbRate', 'IB 速率未知'],
  'worker 未上报清单': ['monitor.cluster.inventory.reason.noReport', 'Worker 未上报'],
  '本集群仅一机': ['monitor.cluster.inventory.singleNode', '本集群仅一机'],
  '无 NVLink（未测到带宽）': [
    'monitor.cluster.inventory.reason.benchNoNvlink',
    '无 NVLink，未测到带宽',
  ],
};

function localizeReason(reason?: string | null): string {
  if (!reason) return '';
  const mapped = REASON_KEYS[reason];
  if (mapped) return l(mapped[0], mapped[1]);
  return reason;
}

function fmtBytes(n?: number | null): string {
  if (n == null || Number.isNaN(Number(n))) return '—';
  const gb = Number(n) / 1024 ** 3;
  if (gb >= 1) return `${gb >= 100 ? Math.round(gb) : Math.round(gb * 10) / 10}G`;
  const mb = Number(n) / 1024 ** 2;
  return `${Math.round(mb)}M`;
}

function looksLikeGpuHeader(text: string): boolean {
  const s = text.toLowerCase();
  return (
    s.includes('uuid:') ||
    /^gpu\s+\d+\s*:/.test(s) ||
    (s.includes('nvidia ') && s.includes('uuid'))
  );
}

function benchScopeLabel(scope?: string): string {
  if (scope === 'intra') return l('monitor.cluster.bench.intra');
  if (scope === 'inter') return l('monitor.cluster.bench.inter');
  return scope || '';
}

const Meta = ({ children }: { children: ReactNode }) => (
  <div className="text-xs leading-5 text-muted">{children}</div>
);

const InventoryTable = ({ rows, loading, toolbar }: Props) => {
  const columns: ColumnsType<InventoryRow> = [
    {
      title: l('monitor.cluster.inventory.col.node'),
      dataIndex: 'name',
      width: 180,
      render: (_, r) => (
        <div className="min-w-0">
          <div className="truncate text-sm font-medium leading-5">{r.name || r.worker_address}</div>
          <Meta>{r.worker_address}</Meta>
          <Tag
            color={r.online === false ? 'default' : 'success'}
            className="mt-1 m-0 text-[11px] leading-5"
          >
            {r.online === false
              ? l('monitor.platform.filter.offline')
              : l('monitor.cluster.inventory.online')}
          </Tag>
        </div>
      ),
    },
    {
      title: l('monitor.cluster.inventory.col.cpu', 'CPU'),
      width: 200,
      render: (_, r) => {
        const model = r.cpu?.model;
        if (!model) {
          return (
            <span className="text-sm text-muted">
              {localizeReason(r.cpu?.reason) || '—'}
            </span>
          );
        }
        return (
          <div>
            <div className="text-sm leading-5">{model}</div>
            <Meta>
              {[
                r.cpu?.arch,
                r.cpu?.cores != null
                  ? `${r.cpu.cores} ${l('monitor.cluster.inventory.cores')}`
                  : null,
              ]
                .filter(Boolean)
                .join(' · ') || '—'}
            </Meta>
          </div>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.mem'),
      width: 120,
      render: (_, r) => (
        <span className="text-sm tabular-nums">
          {fmtBytes(r.mem?.used)} / {fmtBytes(r.mem?.total)}
        </span>
      ),
    },
    {
      title: l('monitor.cluster.inventory.col.disk'),
      width: 160,
      render: (_, r) => {
        const disks = r.disk || [];
        if (!disks.length) return <span className="text-sm text-muted">—</span>;
        return (
          <div>
            {disks.map((d) => (
              <div key={`${d.label}-${d.path}`} className="text-sm leading-5">
                <span className="text-muted">{d.label || d.path} </span>
                {d.reason
                  ? localizeReason(d.reason)
                  : `${fmtBytes(d.free)} ${l('monitor.cluster.inventory.free')} / ${fmtBytes(d.total)}`}
              </div>
            ))}
          </div>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.gpu', 'GPU'),
      width: 180,
      render: (_, r) => {
        const models = r.gpu?.models || [];
        if (!models.length) return <span className="text-sm text-muted">—</span>;
        return (
          <div>
            {models.map((m) => (
              <div key={m.name} className="text-sm leading-5">
                {m.name} × {m.count}
                {m.mem_total != null ? (
                  <span className="ml-1 text-xs text-muted">{fmtBytes(m.mem_total)}</span>
                ) : null}
              </div>
            ))}
          </div>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.nvlink'),
      width: 120,
      render: (_, r) => {
        if (r.nvlink?.present) {
          const links = r.nvlink.active_links ?? 0;
          return (
            <div>
              <div className="text-sm leading-5">NVLink</div>
              <Meta>
                {[
                  links
                    ? `${links} ${l('monitor.cluster.inventory.links')}`
                    : null,
                  r.nvlink.rated,
                ]
                  .filter(Boolean)
                  .join(' · ') || '—'}
              </Meta>
            </div>
          );
        }
        return (
          <span className="text-sm text-muted">
            {localizeReason(r.nvlink?.reason) ||
              l('monitor.cluster.inventory.reason.noNvlink')}
          </span>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.nic'),
      width: 140,
      render: (_, r) => {
        const items = r.nics?.items || [];
        if (!items.length) {
          return (
            <span className="text-sm text-muted">
              {localizeReason(r.nics?.reason) || '—'}
            </span>
          );
        }
        return (
          <div>
            {items.map((n) => (
              <div key={n.name} className="text-sm leading-5">
                {n.name}
                <span className="ml-1 text-xs text-muted">
                  {n.speed_gbps != null
                    ? `${n.speed_gbps}G`
                    : localizeReason(n.reason) ||
                      l('monitor.cluster.inventory.reason.noSpeed')}
                </span>
              </div>
            ))}
          </div>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.inter'),
      width: 140,
      render: (_, r) => {
        if (r.inter?.single_node) {
          return (
            <span className="text-sm text-muted">
              {l('monitor.cluster.inventory.singleNode')}
            </span>
          );
        }
        const ibs = r.ib?.items || [];
        if (!ibs.length) {
          return (
            <span className="text-sm text-muted">
              {localizeReason(r.inter?.reason || r.ib?.reason) || '—'}
            </span>
          );
        }
        return (
          <div>
            {ibs.map((ib) => (
              <div key={`${ib.device}-${ib.port}`} className="text-sm leading-5">
                {ib.device}
                <span className="ml-1 text-xs text-muted">
                  {ib.rate || localizeReason(ib.reason) || '—'}
                </span>
              </div>
            ))}
            {r.inter?.gpu_comm?.rated ? (
              <Meta>{r.inter.gpu_comm.rated}</Meta>
            ) : null}
          </div>
        );
      },
    },
    {
      title: l('monitor.cluster.inventory.col.lastBench'),
      width: 160,
      render: (_, r) => {
        const b = r.last_bench;
        if (!b) {
          return (
            <span className="text-sm text-muted">
              {l('monitor.cluster.inventory.benchNone')}
            </span>
          );
        }
        const scope = benchScopeLabel(b.scope);
        const raw = (b.bandwidth || '').trim();
        const usable = raw && !looksLikeGpuHeader(raw);
        const primary = usable
          ? raw
          : localizeReason(b.reason) ||
            l('monitor.cluster.inventory.benchNoBw');
        const when =
          b.finished_at != null ? dayjs(b.finished_at * 1000).format('HH:mm:ss') : '';
        return (
          <Tooltip title={usable ? raw : b.reason || raw || undefined}>
            <div>
              {scope ? <Meta>{scope}</Meta> : null}
              <div className="text-sm leading-5">{primary}</div>
              {when ? <Meta>{when}</Meta> : null}
            </div>
          </Tooltip>
        );
      },
    },
  ];

  return (
    <ProCard
      size="small"
      title={l('monitor.cluster.inventory.title')}
      extra={
        <span className="text-xs font-normal text-muted">
          {l('monitor.cluster.inventory.hint')}
        </span>
      }
      loading={loading}
      bodyStyle={{ padding: 12 }}
      headStyle={{ paddingBlock: 8 }}
    >
      {toolbar ? <div className="mb-2 flex justify-end">{toolbar}</div> : null}
      <Table<InventoryRow>
        size="small"
        rowKey={(r) => r.worker_address}
        columns={columns}
        dataSource={rows}
        pagination={false}
        scroll={{ x: 1280 }}
        locale={{ emptyText: l('global.data.empty') }}
      />
    </ProCard>
  );
};

export default InventoryTable;
