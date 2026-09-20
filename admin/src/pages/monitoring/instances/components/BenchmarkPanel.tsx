import { useState } from 'react';
import { Button, Form, InputNumber, Table, Switch, Tag, message } from 'antd';
import { useRequest } from 'ahooks';
import dayjs from 'dayjs';
import { FlaskConical } from 'lucide-react';
import { l, lGet } from '@/utils/intl';
import { readRequestFailure, readResponseDetail } from '@/utils/formatApiError';
import request from '@/utils/request';
import MetricLabel from '../../components/MetricLabel';

type MetricBlock = {
  mean?: number;
  p50?: number;
  p95?: number;
  p99?: number;
};

type BenchTask = {
  id: number;
  model_uid: string;
  status: string;
  concurrency?: number;
  num_prompts?: number;
  created_at?: number;
  finished_at?: number | null;
  report?: {
    throughput_rps?: number;
    p50_latency_ms?: number;
    p95_latency_ms?: number;
    p99_latency_ms?: number;
    ttft_ms?: MetricBlock;
    tpot_ms?: MetricBlock;
    itl_ms?: MetricBlock;
    e2el_ms?: MetricBlock;
    output_token_throughput?: number;
    request_throughput?: number;
    per_user_throughput?: number;
    max_effective_concurrency?: number | null;
    success?: number;
    failed?: number;
  } | null;
};

type Props = {
  /** 一个或多个实例；多个时分散创建压测任务 */
  modelUids: string[];
  /** 抽屉内嵌时隐藏外层边框与标题 */
  embedded?: boolean;
};

const fmt = (n?: number | null, d = 1) =>
  n == null || Number.isNaN(Number(n)) || Number(n) <= 0
    ? '-'
    : Number(n).toFixed(d);

const BenchmarkPanel = ({ modelUids, embedded }: Props) => {
  const [form] = Form.useForm();
  const [tasks, setTasks] = useState<BenchTask[]>([]);
  const multi = modelUids.length > 1;
  const uidKey = modelUids.slice().sort().join('|');

  const { loading: listLoading, refresh } = useRequest(
    async () => {
      if (!modelUids.length) return [];
      if (modelUids.length === 1) {
        const res = await request('/benchmarks', {
          params: { model_uid: modelUids[0] },
        });
        const raw = res?.data?.data ?? res?.data ?? [];
        return (Array.isArray(raw) ? raw : []) as BenchTask[];
      }
      const res = await request('/benchmarks');
      const raw = res?.data?.data ?? res?.data ?? [];
      const all = (Array.isArray(raw) ? raw : []) as BenchTask[];
      const allow = new Set(modelUids);
      return all.filter((t) => allow.has(t.model_uid));
    },
    {
      ready: modelUids.length > 0,
      refreshDeps: [uidKey],
      onSuccess: (rows) => setTasks(rows || []),
      onError: () => setTasks([]),
    },
  );

  const { loading: creating, run: createTask } = useRequest(
    async (values: {
      concurrency: number;
      num_prompts: number;
      max_tokens: number;
      scan_sla: boolean;
    }) => {
      const targets = modelUids.filter(Boolean);
      if (!targets.length) {
        throw new Error(String(lGet('monitor.instances.overview.noInstance')));
      }
      const results = await Promise.allSettled(
        targets.map((uid) =>
          request('/benchmarks', {
            method: 'POST',
            // Aggregated toast below — avoid interceptor + success double flash.
            skipNotification: true,
            data: {
              model_uid: uid,
              concurrency: values.concurrency,
              num_prompts: values.num_prompts,
              max_tokens: values.max_tokens,
              stream: true,
              scan_sla: values.scan_sla,
              sla_ttft_p95_ms: 2000,
              sla_tpot_ms: 100,
              concurrency_max: 16,
            },
          }),
        ),
      );
      const isOk = (r: PromiseSettledResult<unknown>) =>
        r.status === 'fulfilled' && (r.value as { success?: boolean })?.success !== false;
      const ok = results.filter(isOk).length;
      const fail = results.length - ok;
      const firstFail = results.find((r) => !isOk(r));
      let failDetail = '';
      if (firstFail && firstFail.status === 'fulfilled') {
        const d = readResponseDetail(firstFail.value);
        failDetail = typeof d === 'string' ? d : d ? JSON.stringify(d) : '';
      } else if (firstFail && firstFail.status === 'rejected') {
        failDetail = String(firstFail.reason?.message || firstFail.reason || '');
      }
      return { ok, fail, total: results.length, failDetail };
    },
    {
      manual: true,
      onSuccess: ({ ok, fail, total, failDetail }) => {
        if (fail === 0) {
          message.success(
            total > 1
              ? String(
                  lGet('monitor.instances.bench.createdMulti',
                  ),
                ).replace('{n}', String(ok))
              : String(
                  lGet('monitor.instances.bench.created',
                  ),
                ),
          );
        } else if (ok === 0) {
          message.error(
            failDetail ||
              String(
                lGet('monitor.instances.bench.createdNone',
                ),
              ),
          );
        } else {
          message.warning(
            String(
              lGet('monitor.instances.bench.createdPartial',
              ),
            )
              .replace('{ok}', String(ok))
              .replace('{fail}', String(fail)) +
              (failDetail ? `；${failDetail}` : ''),
          );
        }
        refresh();
      },
      onError: (e: unknown) => {
        message.error(readRequestFailure(e) || 'failed');
      },
    },
  );

  return (
    <div
      className={
        embedded
          ? 'space-y-3'
          : 'rounded-lg border border-[color:var(--c-border-light)] p-3 space-y-3'
      }
    >
      {embedded ? (
        <div className="text-xs text-muted">
          {multi
            ? l('monitor.instances.bench.hintMulti',
              )
            : l('monitor.instances.bench.hint',
              )}
        </div>
      ) : (
        <div className="text-sm font-medium flex items-center gap-1.5">
          <FlaskConical size={14} />
          {l('monitor.instances.bench')}
          <span className="text-xs text-muted font-normal ml-2">
            {multi
              ? l('monitor.instances.bench.hintMulti',
                )
              : l('monitor.instances.bench.hint',
                )}
          </span>
        </div>
      )}
      <Form
        form={form}
        layout="inline"
        size="small"
        initialValues={{ concurrency: 4, num_prompts: 20, max_tokens: 64, scan_sla: true }}
        onFinish={(v) => createTask(v)}
      >
        <Form.Item
          name="concurrency"
          label={l('monitor.instances.bench.concurrency')}
          rules={[{ required: true }]}
        >
          <InputNumber min={1} max={10000} />
        </Form.Item>
        <Form.Item
          name="num_prompts"
          label={l('monitor.instances.bench.prompts')}
          rules={[{ required: true }]}
        >
          <InputNumber min={1} max={10000} />
        </Form.Item>
        <Form.Item name="max_tokens" label="max_tokens" rules={[{ required: true }]}>
          <InputNumber min={1} max={16000} />
        </Form.Item>
        <Form.Item
          name="scan_sla"
          label={l('monitor.instances.bench.scanSla')}
          tooltip={l('monitor.instances.bench.scanSla.tip',
          )}
          valuePropName="checked"
        >
          <Switch />
        </Form.Item>
        <Form.Item>
          <Button
            type="primary"
            htmlType="submit"
            loading={creating}
            disabled={!modelUids.length}
          >
            {multi
              ? l('monitor.instances.bench.startMulti')
              : l('monitor.instances.bench.start')}
          </Button>
        </Form.Item>
        <Form.Item>
          <Button size="small" onClick={() => refresh()} loading={listLoading}>
            {l('global.actions.refresh')}
          </Button>
        </Form.Item>
      </Form>
      <Table<BenchTask>
        size="small"
        rowKey="id"
        loading={listLoading}
        dataSource={tasks}
        pagination={{ pageSize: 5 }}
        scroll={{ x: multi ? 860 : 720 }}
        columns={[
          { title: 'ID', dataIndex: 'id', width: 60 },
          ...(multi
            ? [
                {
                  title: l('models.instances.modelUid'),
                  dataIndex: 'model_uid',
                  width: 140,
                  ellipsis: true,
                } as const,
              ]
            : []),
          {
            title: l('models.instances.status'),
            dataIndex: 'status',
            width: 140,
            render: (s: string, r: BenchTask) => {
              const color =
                s === 'succeeded'
                  ? 'success'
                  : s === 'failed'
                    ? 'error'
                    : s === 'running'
                      ? 'processing'
                      : 'default';
              const ok = r.report?.success;
              const fail = r.report?.failed;
              const suffix =
                (s === 'succeeded' || s === 'failed') &&
                (ok != null || fail != null)
                  ? ` ${ok ?? 0}/${fail ?? 0}`
                  : '';
              return (
                <Tag color={color}>
                  {s}
                  {suffix}
                </Tag>
              );
            },
          },
          {
            title: l('monitor.instances.bench.createdAt'),
            dataIndex: 'created_at',
            width: 110,
            render: (v) => (v ? dayjs(v).format('MM-DD HH:mm') : '-'),
          },
          {
            title: <MetricLabel zh="首Token时延 P95" en="TTFT P95" />,
            key: 'ttft',
            width: 110,
            render: (_: unknown, r: BenchTask) => fmt(r.report?.ttft_ms?.p95, 0),
          },
          {
            title: <MetricLabel zh="每Token时延 P95" en="TPOT P95" />,
            key: 'tpot',
            width: 110,
            render: (_: unknown, r: BenchTask) => fmt(r.report?.tpot_ms?.p95, 0),
          },
          {
            title: <MetricLabel zh="输出Token吞吐" en="tok/s" />,
            key: 'toks',
            width: 100,
            render: (_: unknown, r: BenchTask) => fmt(r.report?.output_token_throughput, 1),
          },
          {
            title: <MetricLabel zh="每秒请求数" en="RPS" />,
            key: 'rps',
            width: 90,
            render: (_: unknown, r: BenchTask) =>
              fmt(r.report?.request_throughput ?? r.report?.throughput_rps, 2),
          },
          {
            title: <MetricLabel zh="最大有效并发" en="MaxConc" />,
            key: 'mc',
            width: 100,
            render: (_: unknown, r: BenchTask) =>
              r.report?.max_effective_concurrency ?? '-',
          },
        ]}
        expandable={{
          expandedRowRender: (r) => (
            <pre className="text-xs whitespace-pre-wrap m-0">
              {JSON.stringify(r.report || {}, null, 2)}
            </pre>
          ),
        }}
      />
    </div>
  );
};

export default BenchmarkPanel;
