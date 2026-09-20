import { App, Button, Dropdown, Progress, Table, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { MenuProps } from 'antd';
import { ListTodo, MoreHorizontal, Pencil, RotateCcw, Trash2 } from 'lucide-react';

import { EmptyState } from '@/components';
import { EnginePullProgress } from '@/services/engineImages';
import { copyToClipboard, formatDisplayTime } from '@/utils';
import { humanizeEngineImageError } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';

import { engineTagLabel, engineTagTone, formatBytes } from '../utils';
import StatusTag from './StatusTag';
import type { StatusTagTone } from './StatusTag';

type Props = {
  tasks: EnginePullProgress[];
  onCancel: (taskId: string) => void;
  onRetry?: (taskId: string) => void;
  onModify?: (row: EnginePullProgress) => void;
  onDelete?: (taskId: string) => void;
};

const statusTone = (status?: string): StatusTagTone => {
  switch (status) {
    case 'succeeded':
      return 'success';
    case 'running':
    case 'pending':
      return 'info';
    case 'failed':
      return 'error';
    default:
      return 'neutral';
  }
};

const panelShell =
  'rounded-lg bg-card border border-[color:var(--c-border-light)] shadow-card overflow-hidden';

const kindLabel = (kind?: string) => {
  if (kind === 'migrate') return l('models.engines.taskKindMigrate');
  if (kind === 'delete') return l('models.engines.taskKindDelete');
  return l('models.engines.taskKindPull');
};

/** 任务进度标签页：下载 / 迁移统一进度表 */
const TaskPanel: React.FC<Props> = ({
  tasks,
  onCancel,
  onRetry,
  onModify,
  onDelete,
}) => {
  const { message } = App.useApp();

  const columns: ColumnsType<EnginePullProgress> = [
    {
      title: l('models.engines.taskKind'),
      dataIndex: 'kind',
      width: 88,
      render: (kind: string) => (
        <span className="text-sm text-default">{kindLabel(kind)}</span>
      ),
    },
    {
      title: l('models.engines.engine'),
      dataIndex: 'engine',
      width: 100,
      render: (engine: string) =>
        engine ? (
          <StatusTag tone={engineTagTone(engine)} dot={false}>
            {engineTagLabel(engine)}
          </StatusTag>
        ) : (
          '—'
        ),
    },
    {
      title: l('models.engines.version'),
      dataIndex: 'version',
      width: 140,
      ellipsis: true,
      render: (v: string) => <span className="text-default">{v || '—'}</span>,
    },
    {
      title: l('models.engines.taskRoute'),
      key: 'route',
      width: 180,
      ellipsis: true,
      render: (_, row) => {
        if (row.kind !== 'migrate') return '—';
        const src = row.source || '—';
        const dest = row.target || '—';
        return (
          <span className="font-mono text-[12px] text-default">
            {src} → {dest}
          </span>
        );
      },
    },
    {
      title: l('models.engines.taskStartedAt'),
      dataIndex: 'created_at',
      width: 170,
      render: (v?: number | null) => (
        <span className="text-xs tabular-nums text-muted">
          {v ? formatDisplayTime(v) : '—'}
        </span>
      ),
    },
    {
      title: l('models.engines.status'),
      dataIndex: 'status',
      width: 110,
      render: (status: string) => (
        <StatusTag tone={statusTone(status)}>
          {lGet(`models.engines.pullStatus.${status}`) || status}
        </StatusTag>
      ),
    },
    {
      title: l('models.engines.progress'),
      dataIndex: 'progress',
      width: 220,
      render: (v: number, row) => {
        const done = Number(row.bytes_done || 0);
        const total = Number(row.bytes_total || 0);
        const sizeText =
          total > 0
            ? `${formatBytes(done)} / ${formatBytes(total)}`
            : done > 0
              ? `${formatBytes(done)} / —`
              : '';
        return (
          <div className="flex flex-col gap-0.5 min-w-0">
            <Progress
              percent={Math.round((v || 0) * 100)}
              size="small"
              status={
                row.status === 'failed'
                  ? 'exception'
                  : row.status === 'succeeded'
                    ? 'success'
                    : 'active'
              }
            />
            {sizeText ? (
              <span className="text-[11px] text-muted tabular-nums">
                {sizeText}
              </span>
            ) : null}
          </div>
        );
      },
    },
    {
      title: l('models.engines.message'),
      dataIndex: 'message',
      ellipsis: true,
      render: (_, row) => {
        const raw = (row.error || row.message || '—').trim() || '—';
        const text =
          raw === '—'
            ? raw
            : humanizeEngineImageError(raw) || raw;
        const isFail = row.status === 'failed' || row.status === 'cancelled';
        const copyable = text !== '—';
        return (
          <Tooltip
            title={
              <div className="max-w-[480px] font-mono text-[12px] break-all whitespace-pre-wrap">
                {text}
                {raw !== text ? (
                  <div className="mt-2 opacity-70 whitespace-pre-wrap">{raw}</div>
                ) : null}
                {copyable ? (
                  <div className="mt-1 text-[11px] opacity-80">
                    {l('models.engines.messageCopyHint')}
                  </div>
                ) : null}
              </div>
            }
          >
            <button
              type="button"
              className={`max-w-full text-left text-xs font-mono truncate bg-transparent border-0 p-0 cursor-pointer ${
                isFail ? 'text-[var(--c-error)]' : 'text-muted'
              } hover:underline`}
              onClick={() => {
                if (!copyable) return;
                copyToClipboard(raw);
                message.success(
                  String(lGet('models.engines.messageCopied')),
                );
              }}
            >
              {text}
            </button>
          </Tooltip>
        );
      },
    },
    {
      title: l('models.engines.actions'),
      key: 'actions',
      width: 72,
      fixed: 'right',
      render: (_, row) => {
        const items: MenuProps['items'] = [];
        if (row.status === 'pending' || row.status === 'running') {
          items.push({
            key: 'cancel',
            danger: true,
            label: l('models.engines.cancel'),
            onClick: () => onCancel(row.task_id),
          });
        }
        if (
          onRetry &&
          (row.status === 'failed' || row.status === 'cancelled') &&
          row.kind !== 'delete'
        ) {
          items.push({
            key: 'retry',
            icon: <RotateCcw size={14} />,
            label:
              row.kind === 'migrate'
                ? l('models.engines.continue')
                : l('models.engines.retry'),
            onClick: () => onRetry(row.task_id),
          });
        }
        if (onModify && row.image) {
          items.push({
            key: 'modify',
            icon: <Pencil size={14} />,
            label: l('models.engines.modify'),
            onClick: () => onModify(row),
          });
        }
        if (onDelete && row.status !== 'pending' && row.status !== 'running') {
          items.push({
            key: 'delete',
            danger: true,
            icon: <Trash2 size={14} />,
            label: l('models.engines.deleteTask'),
            onClick: () => onDelete(row.task_id),
          });
        }
        if (!items.length) return '—';
        return (
          <Dropdown menu={{ items }} trigger={['click']} placement="bottomRight">
            <Button
              type="text"
              size="small"
              className="!px-1.5"
              aria-label={String(lGet('models.engines.moreActions'))}
              icon={<MoreHorizontal size={16} />}
            />
          </Dropdown>
        );
      },
    },
  ];

  if (!tasks.length) {
    return (
      <EmptyState
        customIcon={<ListTodo size={48} className="text-muted" />}
        title={l('models.engines.taskEmpty')}
      />
    );
  }

  return (
    <div className={panelShell}>
      <Table
        rowKey="task_id"
        columns={columns}
        dataSource={tasks}
        pagination={{
          pageSize: 10,
          showSizeChanger: false,
          hideOnSinglePage: false,
          showTotal: (total) =>
            l('models.engines.taskPageTotal', { total }),
        }}
        scroll={{ x: 860 }}
      />
    </div>
  );
};

export default TaskPanel;
