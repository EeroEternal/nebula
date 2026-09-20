import { ProTable } from '@ant-design/pro-components';
import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { history, useLocation } from '@umijs/max';
import { Alert, Tag, Button, message, Switch, DatePicker, Select, Typography } from 'antd';
import { createContext, useContext, useEffect, useRef, useState } from 'react';
import { RefreshCw, Download } from 'lucide-react';
import dayjs from 'dayjs';

import request from '@/utils/request';
import { convertDateToUTC, formatDisplayTime } from '@/utils';
import { downloadCsv } from '@/utils/downloadCsv';
import { l, lGet } from '@/utils/intl';
import { DATE_FORMAT } from '@/constants';
import {
  asServerPageRows,
  normalizePage,
  normalizePageSize,
} from '@/utils/tablePagination';

const { RangePicker } = DatePicker;
type DateRangeValue = [dayjs.Dayjs, dayjs.Dayjs];

/** 实例 uid 或其副本 tag（uid / uid-0 / uid-P） */
const rowMatchesModel = (r: TraceRow, modelUid: string) => {
  const uid = modelUid.trim();
  if (!uid) return true;
  const tags = Array.isArray(r.tags) ? r.tags.map(String) : [];
  if (tags.some((t) => t === uid || t.startsWith(`${uid}-`))) return true;
  const meta = r.metadata || {};
  for (const k of ['model', 'model_uid', 'modelUid', 'served_model_name']) {
    const v = meta[k];
    if (v != null && (String(v) === uid || String(v).startsWith(`${uid}-`))) return true;
  }
  return false;
};

type TraceRow = {
  id: string;
  timestamp?: string;
  name?: string;
  userId?: string;
  latency?: number;
  tags?: string[];
  metadata?: Record<string, unknown>;
  /** 排序/展示用派生字段，避免 sorter 读空 dataIndex 导致列空白 */
  _ts?: number;
  _latency?: number;
  _ttft?: number;
  _tps?: number;
  _sourceIp?: string;
  _status?: string;
};

const LIVE_INTERVAL_MS = 10_000;
/** 下拉/查询用的空维哨兵；展示文案固定 None */
const EMPTY_DIM = '(empty)';
const EMPTY_DIM_LABEL = 'None';

const parseFacetRows = (res: {
  data?: { data?: { data?: unknown; unavailable?: boolean } | unknown[] };
}): { value?: unknown }[] | null => {
  const page = res?.data?.data;
  if (page == null) return null;
  if (!Array.isArray(page) && page.unavailable === true) return null;
  const raw = Array.isArray(page)
    ? page
    : Array.isArray(page.data)
      ? page.data
      : null;
  if (!Array.isArray(raw)) return null;
  return raw as { value?: unknown }[];
};

const toFacetOpts = (
  rows: { value?: unknown }[],
): { label: string; value: string }[] => {
  const rest: { label: string; value: string }[] = [];
  const seen = new Set<string>([EMPTY_DIM]);
  for (const row of rows) {
    const raw = row?.value == null ? '' : String(row.value).trim();
    const v = raw ? raw : EMPTY_DIM;
    if (seen.has(v)) continue;
    seen.add(v);
    rest.push({ value: v, label: v });
  }
  return [{ value: EMPTY_DIM, label: EMPTY_DIM_LABEL }, ...rest];
};

type FacetQuery = {
  fromTimestamp: string;
  toTimestamp: string;
  tags?: string;
};

const FacetQueryCtx = createContext<FacetQuery>({
  fromTimestamp: '',
  toTimestamp: '',
});

/** 自己拉 facets。必须受控，否则选值进不了 ProTable params。 */
const TraceFacetSelect = ({
  field,
  value,
  onChange,
}: {
  field: 'sourceIp' | 'userId';
  value?: string;
  onChange?: (value?: string) => void;
}) => {
  const { fromTimestamp, toTimestamp, tags } = useContext(FacetQueryCtx);
  const [options, setOptions] = useState<{ label: string; value: string }[]>([
    { value: EMPTY_DIM, label: EMPTY_DIM_LABEL },
  ]);
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const res = await request('/l/traces/facets', {
          params: {
            field,
            fromTimestamp,
            toTimestamp,
            tags: tags || undefined,
            limit: 1000,
          },
        });
        if (cancelled) return;
        const rows = parseFacetRows(res);
        setOptions(rows ? toFacetOpts(rows) : [{ value: EMPTY_DIM, label: EMPTY_DIM_LABEL }]);
      } catch {
        if (!cancelled) {
          setOptions([{ value: EMPTY_DIM, label: EMPTY_DIM_LABEL }]);
        }
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [field, fromTimestamp, toTimestamp, tags]);
  return (
    <Select
      size="small"
      showSearch
      allowClear
      optionFilterProp="label"
      options={options}
      value={value}
      onChange={(next) => onChange?.(next ?? undefined)}
      className="!w-full max-w-[200px]"
      popupMatchSelectWidth={240}
    />
  );
};

const metaStr = (meta: Record<string, unknown> | undefined, ...keys: string[]) => {
  if (!meta) return undefined;
  for (const k of keys) {
    const v = meta[k];
    if (v != null && v !== '') return String(v);
  }
  return undefined;
};

const metaNum = (meta: Record<string, unknown> | undefined, ...keys: string[]) => {
  if (!meta) return undefined;
  for (const k of keys) {
    const v = meta[k];
    if (v == null || v === '') continue;
    const n = Number(v);
    if (!Number.isNaN(n)) return n;
  }
  return undefined;
};

const enrichRow = (r: TraceRow): TraceRow => {
  const status = metaStr(r.metadata, 'status', 'level');
  let _status = status?.toLowerCase();
  if (!_status && Array.isArray(r.tags) && r.tags.some((t) => /error|fail/i.test(t))) {
    _status = 'error';
  }
  return {
    ...r,
    _ts: r.timestamp ? dayjs(r.timestamp).valueOf() : 0,
    _latency: r.latency ?? metaNum(r.metadata, 'latency', 'totalLatency', 'duration'),
    _ttft: metaNum(r.metadata, 'timeToFirstToken', 'time_to_first_token', 'ttftMs', 'ttft'),
    _tps: metaNum(r.metadata, 'tokensPerSecond', 'tokens_per_second', 'tokenPerSecond'),
    _sourceIp: metaStr(r.metadata, 'sourceIp', 'source_ip', 'clientIp', 'client_ip'),
    _status,
  };
};

const compareNum = (a?: number, b?: number) => {
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  return a - b;
};

const colTitle = (text: string) => (
  <span className="inline-flex items-center whitespace-nowrap">{text}</span>
);

type TraceListProps = {
  /** 嵌入模型监控时强制按实例过滤（优先于 URL ?model=） */
  modelUid?: string;
};

/** 链路列表（对话链路 Tab / 模型监控·行为 Tab） */
const TraceList = ({ modelUid }: TraceListProps = {}) => {
  const location = useLocation();
  const modelFilter =
    modelUid || new URLSearchParams(location.search).get('model') || undefined;
  const proTableRef = useRef<ActionType>();
  const reqSeqRef = useRef(0);
  const lastRowsRef = useRef<TraceRow[]>([]);
  const lastTotalRef = useRef(0);
  const fetchingRef = useRef(false);
  const [tableRows, setTableRows] = useState<TraceRow[]>([]);
  const [tableTotal, setTableTotal] = useState(0);
  const [listLoading, setListLoading] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [live, setLive] = useState(true);
  /** tags/metadata 均未命中实例时降级展示全量，并提示 */
  const [modelFilterMiss, setModelFilterMiss] = useState(false);
  const [timeStamp, setTimeStamp] = useState<DateRangeValue>([
    dayjs().subtract(6, 'days'),
    dayjs(),
  ]);
  const [userFilter, setUserFilter] = useState<string | undefined>();
  const [ipFilter, setIpFilter] = useState<string | undefined>();

  const fromTs = convertDateToUTC(
    `${dayjs(timeStamp[0]).format(DATE_FORMAT)} 00:00:00`,
  );
  const toTs = convertDateToUTC(
    `${dayjs(timeStamp[1]).format(DATE_FORMAT)} 23:59:59`,
  );

  const facetQuery: FacetQuery = {
    fromTimestamp: fromTs,
    toTimestamp: toTs,
    tags: modelFilter,
  };

  const handleRefresh = () => {
    proTableRef.current?.reload();
  };

  useEffect(() => {
    if (!live) return undefined;
    const timer = window.setInterval(() => {
      if (fetchingRef.current) return;
      proTableRef.current?.reload();
    }, LIVE_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [live]);

  const handleExportCsv = () => {
    if (!tableRows.length) {
      message.info(String(lGet('global.data.empty')));
      return;
    }
    downloadCsv(
      `traces-${dayjs().format('YYYYMMDD-HHmmss')}.csv`,
      ['id', 'timestamp', 'name', 'userId', 'status', 'sourceIp', 'latency', 'ttftMs', 'tokensPerSecond', 'tags'],
      tableRows.map((r) => [
        r.id,
        r.timestamp ?? '',
        r.name ?? '',
        r.userId ?? '',
        r._status ?? '',
        r._sourceIp ?? '',
        r._latency ?? '',
        r._ttft ?? '',
        r._tps ?? '',
        Array.isArray(r.tags) ? r.tags.join(';') : (r.tags ?? ''),
      ]),
    );
  };

  const linkToDetail = (id: string) => history.push(`/monitor/traffic/${id}`);

  const columns: ProColumns<TraceRow>[] = [
    {
      dataIndex: 'id',
      title: l('monitor.traces.table.id'),
      width: 96,
      hideInSearch: true,
      onCell: () => ({ style: { paddingInline: 8 } }),
      render: (_, record) => {
        const id = record.id || '';
        if (!id) return '-';
        const short = id.length > 6 ? `${id.slice(0, 6)}...` : id;
        return (
          <Typography.Text copyable={{ text: id }} className="!mb-0 font-mono text-xs">
            {short}
          </Typography.Text>
        );
      },
    },
    {
      dataIndex: 'tags',
      title: l('monitor.traces.table.tags'),
      hideInSearch: true,
      width: 120,
      onCell: () => ({ style: { paddingInline: 8 } }),
      render: (tags: unknown) => {
        const list = Array.isArray(tags) ? tags.map(String).filter(Boolean) : [];
        if (!list.length) return '-';
        // Prefer replica-looking ids (…-0); fall back to first tag for legacy traces.
        const replicaLike = list.filter((t) => /-\d+$/.test(t));
        const shown = replicaLike.length ? replicaLike : list.slice(0, 1);
        return (
          <div className="flex flex-wrap gap-0.5">
            {shown.map((item: string) => (
              <Tag bordered={false} key={item} className="!m-0 !px-1.5 text-xs">
                {item}
              </Tag>
            ))}
          </div>
        );
      },
    },
    {
      dataIndex: '_ttft',
      title: colTitle(l('monitor.traces.table.ttft')),
      hideInSearch: true,
      width: 96,
      onCell: () => ({ style: { paddingInline: 8 } }),
      sorter: (a, b) => compareNum(a._ttft, b._ttft),
      showSorterTooltip: false,
      render: (_, record) =>
        record._ttft != null ? `${Number(record._ttft).toFixed(0)}ms` : '-',
    },
    {
      dataIndex: '_tps',
      title: colTitle(l('monitor.traces.table.tokensPerSecond')),
      hideInSearch: true,
      width: 120,
      onCell: () => ({ style: { paddingInline: 8 } }),
      sorter: (a, b) => compareNum(a._tps, b._tps),
      showSorterTooltip: false,
      render: (_, record) =>
        record._tps != null ? `${Number(record._tps).toFixed(1)} tok/s` : '-',
    },
    {
      dataIndex: '_latency',
      title: colTitle(l('monitor.traces.table.latency')),
      hideInSearch: true,
      width: 88,
      onCell: () => ({ style: { paddingInline: 8 } }),
      sorter: (a, b) => compareNum(a._latency, b._latency),
      showSorterTooltip: false,
      render: (_, record) =>
        record._latency != null ? `${Number(record._latency).toFixed(2)}s` : '-',
    },
    {
      dataIndex: 'userId',
      title: l('monitor.traces.table.user'),
      valueType: 'select',
      ellipsis: true,
      width: 100,
      onCell: () => ({ style: { paddingInline: 8 } }),
      renderFormItem: () => (
        <TraceFacetSelect
          field="userId"
          value={userFilter}
          onChange={setUserFilter}
        />
      ),
      formItemProps: {
        label: l('monitor.traces.table.user'),
        className: 'trace-filter-item',
      },
      render: (_, record) => record.userId || '-',
    },
    {
      dataIndex: 'sourceIp',
      title: l('monitor.traces.table.sourceIp'),
      valueType: 'select',
      width: 110,
      onCell: () => ({ style: { paddingInline: 8 } }),
      renderFormItem: () => (
        <TraceFacetSelect
          field="sourceIp"
          value={ipFilter}
          onChange={setIpFilter}
        />
      ),
      formItemProps: {
        label: l('monitor.traces.table.sourceIp'),
        className: 'trace-filter-item',
      },
      render: (_, record) => record._sourceIp || '-',
    },
    {
      dataIndex: '_ts',
      title: colTitle(l('monitor.traces.table.timestamp')),
      hideInSearch: true,
      width: 156,
      onCell: () => ({ style: { paddingInline: 8 } }),
      sorter: (a, b) => compareNum(a._ts, b._ts),
      showSorterTooltip: false,
      render: (_, record) =>
        formatDisplayTime(record.timestamp),
    },
    {
      dataIndex: 'status',
      title: l('monitor.traces.table.status'),
      hideInSearch: true,
      width: 72,
      onCell: () => ({ style: { paddingInline: 8 } }),
      render: (_, record) => {
        const s = record._status;
        if (!s) return '-';
        const color =
          s === 'error' ? 'error' : s === 'ok' ? 'success' : s === 'cancelled' ? 'warning' : 'default';
        return (
          <Tag color={color} className="!m-0 !px-1.5 text-xs">
            {s}
          </Tag>
        );
      },
    },
  ];

  return (
    <FacetQueryCtx.Provider value={facetQuery}>
    <div className="flex flex-col gap-4">
      {fetchError ? (
        <Alert
          type="error"
          showIcon
          message={l('monitor.traces.fetchFailed')}
          description={fetchError}
        />
      ) : null}
      {modelFilter ? (
        <Alert
          type={modelFilterMiss ? 'warning' : 'info'}
          showIcon
          message={l('monitor.traces.modelFilter',
            { uid: modelFilter },
          )}
          description={
            modelFilterMiss
              ? l('monitor.traces.modelFilter.emptyHint',
                )
              : undefined
          }
        />
      ) : null}
      <div className="flex flex-wrap items-center justify-end gap-2">
        <RangePicker
          size="small"
          value={timeStamp}
          format={DATE_FORMAT}
          allowClear={false}
          onChange={(data) => {
            if (data?.[0] && data?.[1]) setTimeStamp([data[0], data[1]]);
          }}
        />
        <label className="inline-flex items-center gap-2 text-sm text-muted">
          <Switch size="small" checked={live} onChange={setLive} />
          {l('monitor.traces.live')}
        </label>
        <Button
          size="small"
          icon={<Download size={14} />}
          onClick={handleExportCsv}
          disabled={!tableRows.length}
        >
          {l('monitor.traces.exportCsv')}
        </Button>
        <Button size="small" icon={<RefreshCw size={14} />} onClick={handleRefresh}>
          {l('global.actions.refresh')}
        </Button>
      </div>
      <ProTable<TraceRow>
        actionRef={proTableRef}
        options={false}
        size="small"
        className="[&_.ant-table-thead>tr>th]:!px-2 [&_.ant-table-tbody>tr>td]:!px-2 [&_.ant-table-tbody>tr]:cursor-pointer"
        scroll={{ x: 980 }}
        search={{
          className: 'pro-table-filter text-sm [&_.ant-form-item-label>label]:text-xs',
          labelWidth: 64,
          span: 6,
          defaultCollapsed: false,
          searchText: String(lGet('global.actions.search')),
          resetText: String(lGet('global.actions.reset')),
          onReset: () => {
            setUserFilter(undefined);
            setIpFilter(undefined);
          },
        } as never}
        columns={columns}
        dataSource={tableRows}
        loading={listLoading}
        pagination={{ defaultPageSize: 10, total: tableTotal }}
        onRow={(record) => ({
          onClick: (e) => {
            const el = e.target as HTMLElement | null;
            // 复制按钮 / 链接等交互元素不跳转
            if (el?.closest?.('.ant-typography-copy, .anticon-copy, button, a')) {
              return;
            }
            if (record.id) linkToDetail(record.id);
          },
        })}
        params={{
          modelFilter: modelFilter || '',
          fromTimestamp: fromTs,
          toTimestamp: toTs,
          userFilter: userFilter || '',
          ipFilter: ipFilter || '',
        }}
        request={async (params) => {
          const reqId = ++reqSeqRef.current;
          fetchingRef.current = true;
          setListLoading(true);
          try {
          const fromTimestamp =
            params.fromTimestamp != null ? String(params.fromTimestamp) : fromTs;
          const toTimestamp =
            params.toTimestamp != null ? String(params.toTimestamp) : toTs;

          const userQ = (params.userFilter ?? userFilter ?? '').toString().trim();
          const ipQ = (params.ipFilter ?? ipFilter ?? '').toString().trim();
          const pageSize = normalizePageSize(params.pageSize);
          const current = normalizePage(params.current);
          const apiUserId =
            userQ && userQ !== EMPTY_DIM ? userQ : undefined;
          const apiSourceIp =
            ipQ === EMPTY_DIM ? '__empty__' : ipQ || undefined;
          const localOnly = userQ === EMPTY_DIM;

          const common = {
            limit: pageSize,
            page: current,
            userId: apiUserId,
            sourceIp: apiSourceIp,
            fromTimestamp,
            toTimestamp,
          };

          const commit = (rows: TraceRow[], total?: number) => {
            const safeTotal = total ?? rows.length;
            lastRowsRef.current = rows;
            lastTotalRef.current = safeTotal;
            setTableRows(rows);
            setTableTotal(safeTotal);
            return {
              data: asServerPageRows(rows, pageSize),
              success: true,
              total: safeTotal,
            };
          };

          const keepLast = () => {
            const rows = lastRowsRef.current;
            const total = lastTotalRef.current;
            setTableRows(rows);
            setTableTotal(total);
            return {
              data: asServerPageRows(rows, pageSize),
              success: true,
              total,
            };
          };

          type PullOk = { rows: TraceRow[]; total?: number; unavailable?: false };
          type PullMiss = { rows: TraceRow[]; total?: number; unavailable: true };
          const pull = async (tags?: string): Promise<PullOk | PullMiss> => {
            const res = await request('/l/traces', {
              params: { ...common, tags: tags || undefined },
            });
            const page = res?.data?.data;
            const raw = Array.isArray(page)
              ? page
              : Array.isArray(page?.data)
                ? page.data
                : null;
            if (
              res?.success === false ||
              page?.unavailable === true ||
              !Array.isArray(raw)
            ) {
              return { rows: lastRowsRef.current, unavailable: true };
            }
            const total = page?.meta?.totalItems;
            return {
              rows: raw.map(enrichRow) as TraceRow[],
              total: typeof total === 'number' ? total : undefined,
            };
          };

          // request 回调里不能用 l()（内部 useIntl，会在发请求前炸掉并钉死 loading）
          const failMsg = String(
            lGet('monitor.traces.fetchFailedHint',
            ),
          );
            let modelMiss = false;
            if (!modelFilter) {
              setModelFilterMiss(false);
            }
            const first = modelFilter
              ? await pull(modelFilter)
              : await pull(undefined);
            if (first.unavailable) {
              setFetchError(failMsg);
              return lastRowsRef.current.length ? keepLast() : commit([], 0);
            }
            let { rows, total } = first;
            if (modelFilter) {
              const tagged = rows.filter((r) => rowMatchesModel(r, modelFilter));
              if (tagged.length === 0) {
                const fallback = await pull(undefined);
                if (fallback.unavailable) {
                  setFetchError(failMsg);
                  return lastRowsRef.current.length
                    ? keepLast()
                    : commit(rows, total);
                }
                rows = fallback.rows.filter((r) => rowMatchesModel(r, modelFilter));
                total = rows.length;
                if (rows.length === 0) {
                  modelMiss = true;
                  rows = fallback.rows;
                  total = fallback.total ?? rows.length;
                }
              } else {
                rows = tagged;
                total = tagged.length;
              }
            }

            if (userQ === EMPTY_DIM) {
              rows = rows.filter((r) => !r.userId?.trim());
              total = rows.length;
            }

            if (reqId !== reqSeqRef.current) {
              if (!lastRowsRef.current.length) {
                setFetchError(null);
                setModelFilterMiss(modelMiss);
                return commit(rows, localOnly ? rows.length : total);
              }
              return keepLast();
            }
            setFetchError(null);
            setModelFilterMiss(modelMiss);
            return commit(rows, localOnly ? rows.length : total);
          } catch {
            const hint = String(
              lGet('monitor.traces.fetchFailedHint',
              ),
            );
            setFetchError(hint);
            const rows = lastRowsRef.current;
            const total = lastTotalRef.current;
            const size = normalizePageSize(params.pageSize);
            setTableRows(rows);
            setTableTotal(total);
            return {
              data: asServerPageRows(rows, size),
              success: true,
              total,
            };
          } finally {
            if (reqId === reqSeqRef.current) {
              fetchingRef.current = false;
              setListLoading(false);
            }
          }
        }}
      />
    </div>
    </FacetQueryCtx.Provider>
  );
};

export default TraceList;
