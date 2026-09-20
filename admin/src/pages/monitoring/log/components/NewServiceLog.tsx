import {
  useState,
  useEffect,
  useLayoutEffect,
  useRef,
  useCallback,
  memo,
  useMemo,
} from 'react';
import { VariableSizeList } from 'react-window';
import {
  QueryFilter,
  ProFormSelect,
  ProFormText,
  ProCard,
} from '@ant-design/pro-components';
import type { QueryFilterProps } from '@ant-design/pro-components';
import dayjs from 'dayjs';
import { useRequest } from 'ahooks';
import { history, useLocation } from '@umijs/max';
import { Alert, Button, DatePicker, Empty, Form, Spin, message } from 'antd';
import { debounce } from 'lodash';
import { Pause, Play } from 'lucide-react';
import { l } from '@/utils/intl';
import { DATE_FORMAT, NODE_TYPE_OPTIONS } from '@/constants';
import request from '@/utils/request';
import {
  EventStreamController,
  getEventStreamFetcher,
} from '@/utils/eventStream';
import type { NodeInfoSource } from '@/types/Public/data';
import { highlightLogLine } from '@/components/LogText';

const { RangePicker } = DatePicker;
type DateRangeValue = [dayjs.Dayjs, dayjs.Dayjs];

const toLogTimeParams = (range: DateRangeValue) => ({
  start_time: dayjs(range[0]).startOf('day').format('YYYY-MM-DDTHH:mm:ss'),
  end_time: dayjs(range[1]).endOf('day').format('YYYY-MM-DDTHH:mm:ss'),
});

/** 与实例日志一致：行高 / 内边距 */
const lineHeight = 20;
const prePadding = 8;

const LEVEL_OPTIONS = [
  { label: 'ALL', value: 'ALL' },
  { label: 'DEBUG', value: 'DEBUG' },
  { label: 'INFO', value: 'INFO' },
  { label: 'WARNING', value: 'WARNING' },
  { label: 'ERROR', value: 'ERROR' },
  { label: 'CRITICAL', value: 'CRITICAL' },
];

const LINES_OPTIONS = [200, 500, 1000, 2000].map((n) => ({
  label: `${n} 行`,
  value: n,
}));

const lineMatchesLevel = (line: string, level: string) => {
  if (!level || level === 'ALL') return true;
  const upper = line.toUpperCase();
  if (level === 'WARNING') {
    return upper.includes('WARNING') || /\bWARN\b/.test(upper);
  }
  return upper.includes(level);
};

const estimateHeight = (text: string) => {
  const lineCount = Math.max(1, text.split('\n').length);
  return lineCount * lineHeight + prePadding * 2;
};

const LogRow = memo(
  ({
    index,
    style,
    data,
    setRowHeight,
  }: {
    index: number;
    style: React.CSSProperties;
    data: {
      logs: string[];
      rowHeights: React.MutableRefObject<number[]>;
    };
    setRowHeight: (index: number, height: number) => void;
  }) => {
    const rowRef = useRef<HTMLDivElement>(null);
    const log = data.logs[index];

    useEffect(() => {
      if (!rowRef.current) return;
      const observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const realHeight = entry.contentRect.height;
          const estimatedHeight = estimateHeight(log);
          if (Math.abs(realHeight - estimatedHeight) > 5) {
            setRowHeight(index, realHeight);
          }
        }
      });
      if (rowRef.current) observer.observe(rowRef.current);
      return () => observer.disconnect();
    }, [index, log, setRowHeight]);

    return (
      <div style={style}>
        <div ref={rowRef} className="border-b border-[color:var(--c-border-light)]">
          <pre className="m-0 px-3 py-2 text-xs font-mono whitespace-pre-wrap break-all leading-5">
            {highlightLogLine(log, String(index))}
          </pre>
        </div>
      </div>
    );
  },
);

const STICK_BOTTOM_PX = 80;

const VirtualLogViewer = ({ logs }: { logs: string[] }) => {
  const listRef = useRef<VariableSizeList>(null);
  const outerRef = useRef<HTMLDivElement>(null);
  const rowHeights = useRef<number[]>([]);
  const stickBottomRef = useRef(true);
  const programmaticScrollRef = useRef(false);
  /** 首次贴底完成前忽略 onScroll，避免 scrollTop=0 误关 stick */
  const scrollReadyRef = useRef(false);
  const countRef = useRef(0);
  const [verticalHeight, setVerticalHeight] = useState(0);
  const lastTailRef = useRef('');
  countRef.current = logs.length;

  const scrollToBottom = useCallback(() => {
    if (!stickBottomRef.current) return;
    programmaticScrollRef.current = true;
    const run = () => {
      const el = outerRef.current;
      if (el) {
        el.scrollTop = el.scrollHeight;
      }
      const n = countRef.current;
      if (n > 0) {
        listRef.current?.scrollToItem(n - 1, 'end');
      }
    };
    run();
    requestAnimationFrame(() => {
      run();
      requestAnimationFrame(() => {
        run();
        window.setTimeout(() => {
          run();
          programmaticScrollRef.current = false;
          scrollReadyRef.current = true;
        }, 160);
      });
    });
  }, []);

  const getRowHeight = useCallback(
    (index: number) => {
      return rowHeights.current[index] || estimateHeight(logs[index]);
    },
    [logs],
  );

  const setRowHeight = useCallback(
    (index: number, height: number) => {
      if (rowHeights.current[index] !== height) {
        rowHeights.current[index] = height;
        listRef.current?.resetAfterIndex(index);
        // 行高重算会把「视觉底」顶上去，贴底时跟一次
        scrollToBottom();
      }
    },
    [scrollToBottom],
  );

  useEffect(() => {
    if (logs.length < rowHeights.current.length) {
      rowHeights.current = [];
      listRef.current?.resetAfterIndex(0);
    }
  }, [logs]);

  // 新日志出现（尾行变化）时跟随到底；用户上翻后 stick=false 则不再强拉
  useLayoutEffect(() => {
    if (!logs.length) {
      lastTailRef.current = '';
      stickBottomRef.current = true;
      scrollReadyRef.current = false;
      return;
    }
    const tail = logs[logs.length - 1] || '';
    if (tail === lastTailRef.current) return;
    lastTailRef.current = tail;
    scrollToBottom();
  }, [logs, scrollToBottom]);

  useEffect(() => {
    const handleResize = debounce(() => {
      setVerticalHeight(window.innerHeight - 420);
    }, 100);
    handleResize();
    window.addEventListener('resize', handleResize);
    return () => {
      window.removeEventListener('resize', handleResize);
      handleResize.cancel();
    };
  }, []);

  return (
    <div
      data-testid="service-log-virtual"
      className="overflow-hidden rounded-lg border border-[color:var(--c-border-light)] bg-[#F5F5F5] shadow-card"
    >
      <VariableSizeList
        ref={listRef}
        outerRef={outerRef}
        height={Math.max(verticalHeight, 320)}
        width="100%"
        itemCount={logs.length}
        itemSize={getRowHeight}
        overscanCount={8}
        itemData={{ logs, rowHeights }}
        className="bg-[#F5F5F5]"
        onScroll={({ scrollOffset, scrollUpdateWasRequested }) => {
          if (
            !scrollReadyRef.current ||
            scrollUpdateWasRequested ||
            programmaticScrollRef.current
          ) {
            return;
          }
          const el = outerRef.current;
          if (!el) return;
          stickBottomRef.current =
            el.scrollHeight - scrollOffset - el.clientHeight <= STICK_BOTTOM_PX;
        }}
        style={{
          scrollbarWidth: 'thin',
          scrollbarColor: '#b3b3b3 #e8e8e8',
        }}
      >
        {(props) => <LogRow {...props} setRowHeight={setRowHeight} />}
      </VariableSizeList>
    </div>
  );
};

const utc_offset = -new Date().getTimezoneOffset() / 60;
const initRoleValue = NODE_TYPE_OPTIONS[0].value;
const DEFAULT_LINES = 500;
const MAX_LOG_LINES = 8000;

type LogStreamEvent = {
  line?: string;
  last_offset?: number;
  error?: string;
};

const ServiceLogs = () => {
  const location = useLocation();
  /** 深链仍可带 request_id 做内容过滤，表单不再展示该字段 */
  const urlRequestId = useMemo(
    () => new URLSearchParams(location.search).get('request_id') || '',
    [location.search],
  );
  const [form] = Form.useForm();
  const [timeStamp, setTimeStamp] = useState<DateRangeValue>([
    dayjs().subtract(1, 'month'),
    dayjs(),
  ]);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [paused, setPaused] = useState(false);

  const streamCtrlRef = useRef<EventStreamController | null>(null);
  const streamActiveRef = useRef(false);
  const followOffsetRef = useRef(0);
  const timeStampRef = useRef(timeStamp);
  timeStampRef.current = timeStamp;

  const roleValue = Form.useWatch('role', form);
  const levelFilter = Form.useWatch('level', form) || 'ALL';
  const keywordFilter = Form.useWatch('keyword', form) || '';
  const linesWatch = Form.useWatch('lines', form);
  const lines = Number(linesWatch) > 0 ? Number(linesWatch) : DEFAULT_LINES;

  const stopStream = useCallback(() => {
    streamCtrlRef.current?.terminate();
    streamCtrlRef.current = null;
  }, []);

  /** 与实例日志一致：GET(from_end) 灌最新 N 行，再 SSE 续跟 */
  const startStream = useCallback(
    (opts: {
      role: string;
      node?: string;
      lines: number;
      range?: DateRangeValue;
    }) => {
      const node = opts.node;
      if (!node) return;
      const pageLines = Math.max(1, Math.min(opts.lines || DEFAULT_LINES, 5000));
      stopStream();
      streamActiveRef.current = false;
      followOffsetRef.current = 0;
      setLoading(true);
      setLogLines([]);

      const range = opts.range || timeStampRef.current;
      const timeParams = toLogTimeParams(range);

      const openFollow = (fromOffset: number) => {
        const ctrl = new EventStreamController();
        streamCtrlRef.current = ctrl;
        void getEventStreamFetcher<LogStreamEvent>(
          {
            url: '/api/logs/stream',
            params: {
              role: opts.role,
              node,
              lines: pageLines,
              utc_offset,
              last_offset: fromOffset,
              from_end: fromOffset <= 0 ? true : undefined,
              ...timeParams,
            },
            options: {
              onData: (data) => {
                setLoading(false);
                if (typeof data.last_offset === 'number') {
                  followOffsetRef.current = data.last_offset;
                }
                if (data.line != null) {
                  const line = String(data.line);
                  streamActiveRef.current = true;
                  setLogLines((prev) => {
                    const next = prev.length ? [...prev, line] : [line];
                    return next.length > MAX_LOG_LINES
                      ? next.slice(-MAX_LOG_LINES)
                      : next;
                  });
                }
              },
              onError: (msg) => {
                setLoading(false);
                message.error(msg);
              },
              onEnd: () => {
                setLoading(false);
                if (streamCtrlRef.current === ctrl) {
                  streamCtrlRef.current = null;
                }
              },
            },
          },
          ctrl,
        );
      };

      void request('/api/logs', {
        params: {
          role: opts.role,
          node,
          lines: pageLines,
          utc_offset,
          last_offset: 0,
          from_end: true,
          ...timeParams,
        },
        skipNotification: true,
      })
        .then((res) => {
          if (streamActiveRef.current) return;
          const payload = res?.data ?? res;
          const logs = payload?.logs ?? payload?.data?.logs;
          const list = Array.isArray(logs)
            ? logs.map((x: unknown) => String(x))
            : [];
          const nextOff =
            typeof payload?.last_offset === 'number'
              ? payload.last_offset
              : typeof payload?.data?.last_offset === 'number'
                ? payload.data.last_offset
                : list.length;
          if (list.length) {
            setLogLines(list);
            setLoading(false);
            followOffsetRef.current = nextOff;
          }
          openFollow(nextOff);
        })
        .catch(() => {
          openFollow(0);
        });
    },
    [stopStream],
  );

  const { data: nodeRes } = useRequest(
    () => request<{ data: { results: NodeInfoSource[] } }>('/cluster/info'),
    {
      onFinally: (_, res) => {
        const { ip_address } =
          (res?.data?.results || []).find(
            (item) => item.node_type.toLocaleLowerCase() === initRoleValue,
          ) || {};
        form.setFieldValue('node', ip_address);
        if (ip_address) {
          setPaused(false);
          startStream({
            role: initRoleValue,
            node: ip_address,
            lines: Number(form.getFieldValue('lines')) || DEFAULT_LINES,
            range: timeStampRef.current,
          });
        }
      },
    },
  );

  useEffect(() => () => stopStream(), [stopStream]);

  const nodeList = nodeRes?.data?.results || [];
  const ipList = useMemo(() => {
    return nodeList
      .filter((item) => item.node_type.toLocaleLowerCase() === roleValue)
      .map((item) => item.ip_address);
  }, [roleValue, nodeList]);

  const filteredLogs = useMemo(() => {
    const kw = String(keywordFilter || '').trim().toLowerCase();
    const rid = String(urlRequestId || '').trim().toLowerCase();
    return (logLines || []).filter((line) => {
      if (!lineMatchesLevel(line, levelFilter)) return false;
      if (kw && !line.toLowerCase().includes(kw)) return false;
      if (rid && !line.toLowerCase().includes(rid)) return false;
      return true;
    });
  }, [logLines, levelFilter, keywordFilter, urlRequestId]);

  const handleQuery: QueryFilterProps['onFinish'] = async (values) => {
    const v = values || {};
    setPaused(false);
    startStream({
      role: String(v.role || initRoleValue),
      node: v.node as string | undefined,
      lines: Number(v.lines) || DEFAULT_LINES,
      range: timeStamp,
    });
    // 保留 tab；清除已废弃的 request_id 表单同步
    const params = new URLSearchParams(location.search);
    params.set('tab', 'service');
    if (!urlRequestId) params.delete('request_id');
    history.replace({
      pathname: '/monitor/logs',
      search: params.toString() || undefined,
    });
  };

  const handleTimeChange = (data: DateRangeValue | null) => {
    if (!data || data.length !== 2) return;
    setTimeStamp(data);
    if (paused) return;
    const values = form.getFieldsValue();
    if (values?.node) {
      startStream({
        role: String(values.role || initRoleValue),
        node: values.node,
        lines: Number(values.lines) || DEFAULT_LINES,
        range: data,
      });
    }
  };

  const togglePause = () => {
    if (paused) {
      setPaused(false);
      const values = form.getFieldsValue();
      startStream({
        role: String(values.role || initRoleValue),
        node: values.node,
        lines: Number(values.lines) || DEFAULT_LINES,
        range: timeStamp,
      });
    } else {
      setPaused(true);
      stopStream();
    }
  };

  return (
    <>
      <div className="mb-3 flex flex-wrap items-start justify-between gap-2">
        <Alert
          className="mb-0 min-w-0 flex-1 text-xs"
          type="info"
          showIcon
          message={l('monitor.logs.service.hint',
          )}
        />
        <div className="flex shrink-0 items-center gap-2 justify-end">
          <RangePicker
            size="small"
            value={timeStamp}
            format={DATE_FORMAT}
            allowClear={false}
            onChange={(data) => handleTimeChange(data as DateRangeValue)}
          />
          <Button
            size="small"
            icon={paused ? <Play size={14} /> : <Pause size={14} />}
            onClick={togglePause}
          >
            {paused
              ? l('models.instances.viewLogs.resumeLive')
              : l('models.instances.viewLogs.pauseLive')}
          </Button>
        </div>
      </div>
      <ProCard bodyStyle={{ padding: 0 }}>
        <QueryFilter
          className="service-log-filter px-3 pt-3 !pb-0 text-xs [&_.ant-form-item]:!mb-2 [&_.ant-form-item-label>label]:!text-xs [&_.ant-select]:!text-xs [&_.ant-input]:!text-xs"
          labelWidth="auto"
          size="small"
          showHiddenNum={false}
          defaultCollapsed={false}
          collapseRender={false}
          onFinish={handleQuery}
          span={6}
          form={form}
          searchGutter={[16, 4]}
        >
          <ProFormSelect
            options={NODE_TYPE_OPTIONS}
            name="role"
            initialValue={initRoleValue}
            placeholder={l('monitor.logs.service.role')}
            label={l('monitor.logs.service.role')}
            onChange={() => form.setFieldValue('node', undefined)}
            allowClear={false}
            fieldProps={{ size: 'small' }}
          />
          <ProFormSelect
            options={ipList}
            initialValue={ipList[0]}
            name="node"
            placeholder={l('monitor.logs.service.node')}
            label={l('monitor.logs.service.node')}
            allowClear={false}
            fieldProps={{ size: 'small' }}
          />
          <ProFormSelect
            options={LEVEL_OPTIONS}
            name="level"
            initialValue="ALL"
            label={l('monitor.logs.service.level')}
            allowClear={false}
            fieldProps={{ size: 'small' }}
          />
          <ProFormSelect
            options={LINES_OPTIONS}
            name="lines"
            initialValue={DEFAULT_LINES}
            label={l('monitor.logs.service.lines')}
            allowClear={false}
            fieldProps={{ size: 'small' }}
          />
          <ProFormText
            name="keyword"
            label={l('monitor.logs.service.keyword')}
            placeholder={l('monitor.logs.service.keyword.placeholder')}
            fieldProps={{ size: 'small' }}
            colSize={2}
          />
        </QueryFilter>
      </ProCard>

      <div className="my-3 flex flex-wrap items-center gap-2 text-xs text-muted">
        <span>
          {l('monitor.logs.service.filteredCount', {
            shown: filteredLogs.length,
            total: logLines.length,
          })}
        </span>
        <span>· {lines} 行窗口</span>
        {paused ? (
          <span>({l('models.instances.viewLogs.pauseLive')})</span>
        ) : null}
        {urlRequestId ? (
          <span className="font-mono">· request_id={urlRequestId}</span>
        ) : null}
      </div>
      <Spin spinning={loading && !logLines.length}>
        {filteredLogs.length ? (
          <VirtualLogViewer logs={filteredLogs} />
        ) : (
          <div className="rounded-lg border border-[color:var(--c-border-light)] bg-[#F5F5F5] py-10">
            <Empty description={l('global.data.empty')} />
          </div>
        )}
      </Spin>
    </>
  );
};

export default ServiceLogs;
