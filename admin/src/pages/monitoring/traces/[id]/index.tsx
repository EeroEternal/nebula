import { history, useParams } from '@umijs/max';
import { useRequest } from 'ahooks';
import { useMemo } from 'react';
import { Button, Card, Tabs, Tag } from 'antd';
import { ArrowLeft, Network, TextAlignStart } from 'lucide-react';
import dayjs from 'dayjs';
import { PageContainer } from '@/components';
import request from '@/utils/request';
import { l } from '@/utils/intl';
import TabsForTree from './components/TabsForTree';
import TabsForTimeLine from './components/TabsForTimeLine';

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'image_url'; image_url: string | { url: string } }
  | { type: 'video_url'; video_url: string | { url: string } }
  | { type: 'audio_url'; audio_url: string | { url: string } }
  | { type: 'input_audio'; input_audio: { data?: string; format?: string } };

export type InputContent = string | null | ContentBlock[] | Record<string, unknown>;
export interface ObservationsItem {
  name: string;
  type: string;
  createdAt: string;
  latency: number;
  model: string;
  output: string | Record<string, unknown>;
  metadata: unknown;
  id: string;
  input:
    | string
    | {
        role: string;
        content: InputContent;
        name?: string;
        tool_calls?: unknown[];
        reasoning_content?: string;
      }[];
  usage: {
    input: number;
    output: number;
    total: number;
  };
  timeToFirstToken: number;
  startTime?: string;
  completionStartTime?: string;
}
interface TracesDetailResponse {
  tags: string[];
  observations: ObservationsItem[];
  userId?: string;
  metadata?: Record<string, unknown>;
  latency?: number;
}

const metaText = (meta: Record<string, unknown> | undefined, ...keys: string[]) => {
  if (!meta) return undefined;
  for (const k of keys) {
    const v = meta[k];
    if (v != null && v !== '') return String(v);
  }
  return undefined;
};

const metaNumber = (meta: Record<string, unknown> | undefined, ...keys: string[]) => {
  if (!meta) return undefined;
  for (const k of keys) {
    const v = meta[k];
    if (v == null || v === '') continue;
    const n = Number(v);
    if (!Number.isNaN(n)) return n;
  }
  return undefined;
};

const TracesDetail = () => {
  const params = useParams();
  const { data } = useRequest(() =>
    request<{ data: { data: TracesDetailResponse } }>(`/l/traces/${params.id}`),
  );
  const {
    observations = [],
    tags = [],
    userId,
    metadata: traceMeta,
    latency: traceLatency,
  } = data?.data?.data || {};
  const sourceIp = metaText(traceMeta, 'sourceIp', 'source_ip', 'clientIp');
  const status = metaText(traceMeta, 'status', 'level');
  const requestId = metaText(traceMeta, 'request_id', 'requestId');
  const ttftMs = metaNumber(traceMeta, 'timeToFirstToken', 'time_to_first_token', 'ttftMs');
  const { inputTotal, outputTotal, totalCost } = useMemo(() => {
    return (observations || []).reduce(
      (acc, item) => {
        acc.inputTotal += item?.usage?.input || 0;
        acc.outputTotal += item?.usage?.output || 0;
        acc.totalCost += item?.usage?.total || 0;
        return acc;
      },
      { inputTotal: 0, outputTotal: 0, totalCost: 0 },
    );
  }, [observations]);
  const newObservations = (observations || []).map((item) => {
    const latencySec = Number(item.latency) || 0;
    let ttft = item.timeToFirstToken;
    // xtrace 常只写 completionStartTime，timeToFirstToken 为空；按差值推导（秒→沿用原字段语义）
    if (ttft == null && item.completionStartTime && item.startTime) {
      const start = dayjs(item.startTime);
      const cst = dayjs(item.completionStartTime);
      let deltaSec = cst.diff(start, 'millisecond') / 1000;
      // 历史 bug：本地时间被标成 Z，差值约 +8h；用 latency 纠正
      if (latencySec > 0 && deltaSec > latencySec + 0.5) {
        const offsets = [8, 9, 7, 5.5].map((h) => h * 3600);
        for (const off of offsets) {
          const cand = deltaSec - off;
          if (cand >= 0 && cand <= latencySec + 0.5) {
            deltaSec = cand;
            break;
          }
        }
      }
      if (deltaSec >= 0 && (latencySec <= 0 || deltaSec <= latencySec + 0.5)) {
        ttft = deltaSec;
      }
    }
    return {
      ...item,
      // Langfuse/xtrace latency 已是秒，勿再 /1000
      latency: latencySec,
      timeToFirstToken: ttft,
    };
  });
  const tabs = [
    {
      key: 'tree',
      label: (
        <span className="flex items-center gap-1">
          <Network size={16} />
          Tree
        </span>
      ),
      children: <TabsForTree observations={newObservations} />,
    },
    {
      key: 'timeline',
      label: (
        <span className="flex items-center gap-1">
          <TextAlignStart size={16} />
          Timeline
        </span>
      ),
      children: <TabsForTimeLine observations={newObservations} />,
    },
  ];
  const goBack = () => {
    // 优先浏览器历史（保留列表筛选）；无历史则回对话监控·链路
    if (window.history.length > 1) {
      history.back();
      return;
    }
    history.push('/monitor/traffic?tab=traces');
  };

  return (
    <PageContainer showBreadcrumb showPageHeader={false}>
      <div className="mb-3">
        <Button icon={<ArrowLeft size={14} />} onClick={goBack}>
          {l('monitor.traces.back')}
        </Button>
      </div>
      <Card title={params.id}>
        <span className="border rounded-[20px] px-[12px] py-[4px] font-semibold">
          {inputTotal} → {outputTotal} (∑ {totalCost})
        </span>
        <div className="mt-[10px] mb-[10px] flex flex-wrap gap-2 text-xs">
          {userId ? (
            <span className="rounded-full border px-[8px] py-[2px] font-semibold">
              用户: {userId}
            </span>
          ) : null}
          {sourceIp ? (
            <span className="rounded-full border px-[8px] py-[2px] font-semibold">
              来源 IP: {sourceIp}
            </span>
          ) : null}
          {status ? (
            <span className="rounded-full border px-[8px] py-[2px] font-semibold">
              状态: {status}
            </span>
          ) : null}
          {ttftMs != null ? (
            <span className="rounded-full border px-[8px] py-[2px] font-semibold">
              首 Token: {Number(ttftMs).toFixed(0)}ms
            </span>
          ) : null}
          {traceLatency != null ? (
            <span className="rounded-full border px-[8px] py-[2px] font-semibold">
              总耗时: {Number(traceLatency).toFixed(2)}s
            </span>
          ) : null}
        </div>
        <div className="mb-[30px] border inline-block rounded-[6px] mt-[10px] px-[12px] py-[4px] flex gap-x-[8px]">
          Tags
          <div>
            {tags.map((item) => (
              <Tag key={item}>{item}</Tag>
            ))}
          </div>
        </div>
        <Tabs items={tabs} />
        {requestId ? (
          <div className="mt-4">
            <Button
              type="link"
              className="px-0"
              onClick={() =>
                history.push(
                  `/monitor/logs?tab=service&request_id=${encodeURIComponent(requestId)}`,
                )
              }
            >
              {l('monitor.traces.openServiceLogs')}
              <span className="text-muted ml-1 font-mono text-[12px]">{requestId}</span>
            </Button>
          </div>
        ) : null}
      </Card>
    </PageContainer>
  );
};
export default TracesDetail;
