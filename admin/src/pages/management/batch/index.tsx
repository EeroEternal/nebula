import { ProTable } from '@ant-design/pro-components';
import type { ProColumns, ActionType } from '@ant-design/pro-components';
import { Button, Tag, Dropdown, Progress, App } from 'antd';
import { RefreshCw, Plus, Eye, Ban, Trash2, MoreHorizontal } from 'lucide-react';
import { useRef, useState } from 'react';
import { mapValues, isNumber } from 'lodash';
import { useRequest } from 'ahooks';

import { PageContainer } from '@/components';
import { calculatePercentage } from '@/utils';
import { formPageParams } from '@/utils/fomatData';
import { UNLIMITED, ALL_LIST_PAGES_PARAMS } from '@/constants';
import { BATCH_STATUS_COLORS, BatchStatus } from '@/constants/batch';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import type { BatchListItem } from '@/types/Public/data';
import CreateBatchModal from './components/CreateBatchModal';
import DetailDrawer from './components/Detail';

const Batch = () => {
  const { modal } = App.useApp();
  const proTableRef = useRef<ActionType>();
  const [currentRecordDetail, setCurrentRecordDetail] = useState<BatchListItem | undefined>(
    undefined,
  );
  const [taskBatchCounts, setBatchStatusCounts] = useState({
    runningCount: 0,
    canceledCount: 0,
    completedCount: 0,
    failedCount: 0,
  });
  const { run: refreshBatchStatusCounts, loading } = useRequest(
    () => request('/batches', { params: ALL_LIST_PAGES_PARAMS }),
    {
      onSuccess: (res) => {
        if (res.success) {
          const results = (res?.data?.data?.result || []) as BatchListItem[];
          const counts = results.reduce(
            (acc, item) => {
              switch (item.status) {
                case BatchStatus.running:
                  acc.runningCount++;
                  break;
                case BatchStatus.canceled:
                  acc.canceledCount++;
                  break;
                case BatchStatus.completed:
                  acc.completedCount++;
                  break;
                case BatchStatus.failed:
                  acc.failedCount++;
                  break;
                default:
                  break;
              }
              return acc;
            },
            { runningCount: 0, canceledCount: 0, completedCount: 0, failedCount: 0 },
          );
          setBatchStatusCounts(counts);
        }
      },
    },
  );

  const refresh = () => {
    proTableRef.current?.reset?.();
    proTableRef.current?.reload();
    refreshBatchStatusCounts();
  };
  const { runAsync: cancelBatch } = useRequest(
    (id) => request(`/batches/${id}`, { method: 'post' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          refresh();
        }
      },
    },
  );

  const handleCancel = (id: number) => {
    modal.confirm({
      title: lGet('tasks.batch.actions.cancel.tips'),
      onOk: () => cancelBatch(id),
    });
  };
  const { runAsync: deleteBatch } = useRequest(
    (id) => request(`/batches/${id}`, { method: 'delete' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          refresh();
        }
      },
    },
  );
  const handleDelete = (id: number) => {
    modal.confirm({
      title: lGet('tasks.batch.actions.delete.tips'),
      onOk: () => deleteBatch(id),
    });
  };

  const columns: ProColumns<BatchListItem>[] = [
    {
      title: 'ID',
      dataIndex: 'id',
      hideInSearch: true,
      width: '80px',
    },
    {
      title: l('tasks.batch.endpoint'),
      dataIndex: 'endpoint',
      width: '170px',
      hideInSearch: true,
    },
    {
      title: l('tasks.batch.status'),
      dataIndex: 'status',
      width: '110px',
      valueEnum: {
        [UNLIMITED]: l('global.data.unlimited'),
        ...mapValues(BATCH_STATUS_COLORS, (_, key: number) =>
          l(`tasks.batch.status.${BatchStatus[key]}`),
        ),
      },
      initialValue: UNLIMITED,
      render: (_, record) => {
        if (!isNumber(record.status)) return '-';
        return (
          <Tag
            bordered={false}
            color={
              BATCH_STATUS_COLORS[record.status as keyof typeof BATCH_STATUS_COLORS] || 'default'
            }
          >
            {l(`tasks.batch.status.${BatchStatus[record.status]}`)}
          </Tag>
        );
      },
    },
    {
      title: l('tasks.batch.progress'),
      dataIndex: 'request_completed',
      width: '140px',
      hideInSearch: true,
      render: (_, record) => {
        if (isNumber(record.request_total) && isNumber(record.request_completed)) {
          return (
            <Progress
              status="normal"
              percent={calculatePercentage(record.request_completed, record.request_total)}
              strokeColor={window.THEME_PRIMARY_COLOR}
              className="!leading-[0]"
            />
          );
        }
        return '-';
      },
    },
    {
      title: l('tasks.batch.requestTotal'),
      dataIndex: 'request_total',
      width: '140px',
      hideInSearch: true,
      render: (_, record) => {
        if (isNumber(record.request_total) && isNumber(record.request_completed)) {
          return `${record.request_completed} / ${record.request_total}`;
        }
        return '-';
      },
    },
    {
      title: l('tasks.batch.requestFailed'),
      dataIndex: 'request_failed',
      width: '100px',
      hideInSearch: true,
    },
    {
      title: l('tasks.batch.inputFileUrl'),
      dataIndex: 'input_file_url',
      hideInSearch: true,
      width: '140px',
      copyable: true,
      ellipsis: {
        showTitle: true,
      },
    },
    {
      title: l('tasks.batch.outputFileUrl'),
      dataIndex: 'output_file_url',
      hideInSearch: true,
      width: '140px',
      copyable: true,
      ellipsis: {
        showTitle: true,
      },
    },
    {
      title: l('tasks.batch.completionWindow'),
      dataIndex: 'completion_window',
      hideInSearch: true,
      width: '100px',
    },
    {
      title: l('tasks.batch.createAt'),
      dataIndex: 'created_at',
      hideInSearch: true,
      width: '180px',
    },
    {
      title: l('tasks.batch.completedAt'),
      dataIndex: 'completed_at',
      hideInSearch: true,
      width: '180px',
    },
    {
      title: l('tasks.batch.cancelledAt'),
      dataIndex: 'cancelled_at',
      hideInSearch: true,
      width: '180px',
    },
    {
      title: l('global.actions.action'),
      valueType: 'option',
      fixed: 'right',
      width: '110px',
      render: (_, record) => {
        const items = [
          {
            key: 'detail',
            show: true,
            icon: <Eye size={16} />,
            label: l('global.actions.detail'),
            onClick: () => setCurrentRecordDetail(record),
          },
          {
            key: 'cancel',
            show: [1, 2, 3].includes(record.status),
            icon: <Ban size={16} />,
            label: l('global.actions.cancel'),
            onClick: () => handleCancel(record.id),
          },

          {
            key: 'delete',
            show: true,
            icon: <Trash2 size={16} />,
            label: l('global.actions.delete'),
            className: '!text-danger',
            onClick: () => handleDelete(record.id),
          },
        ];
        return (
          <Dropdown
            key="actions"
            menu={{
              items: items.filter((item) => item.show),
            }}
          >
            <MoreHorizontal size={16} />
          </Dropdown>
        );
      },
    },
  ];
  return (
    <PageContainer
      title={l('menu.tasks.batch')}
      subTitle={l('tasks.batch.subTitle')}
      extraContent={
        <div className="flex gap-3">
          <Button size="large" icon={<RefreshCw size={14} />} loading={loading} onClick={refresh}>
            {l('global.actions.refresh')}
          </Button>
          <CreateBatchModal submitBack={refresh}>
            <Button type="primary" size="large" icon={<Plus size={14} />}>
              {l('tasks.finetune.create')}
            </Button>
          </CreateBatchModal>
        </div>
      }
    >
      <div className="flex flex-col gap-6">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 ">
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.batch.status.running')}</div>
            <div className="text-3xl font-bold text-blue-500">{taskBatchCounts.runningCount}</div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.batch.status.completed')}</div>
            <div className="text-3xl font-bold text-green-500">
              {taskBatchCounts.completedCount}
            </div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.batch.status.failed')}</div>
            <div className="text-3xl font-bold text-danger">{taskBatchCounts.failedCount}</div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.batch.status.canceled')}</div>
            <div className="text-3xl font-bold text-muted">{taskBatchCounts.canceledCount}</div>
          </div>
        </div>
        <ProTable
          toolBarRender={false}
          rowKey="id"
          actionRef={proTableRef}
          columns={columns}
          search={{
            className: 'pro-table-filter',
            span: 6,
            labelWidth: 'auto',
            collapseRender: () => null,
          }}
          scroll={{ x: 1100 }}
          pagination={{
            pageSize: 10,
          }}
          request={async (params: { pageSize: number; current: number; status: string }) => {
            const pageParams = formPageParams(params);
            const res = await request('/batches', {
              params: {
                ...pageParams,
                status: params?.status !== UNLIMITED ? params?.status : undefined,
              },
            });
            return {
              data: res?.data?.data?.result || [],
              total: res.data?.data?.count || 0,
              success: true,
            };
          }}
        />
      </div>
      {!!currentRecordDetail && (
        <DetailDrawer
          open
          values={currentRecordDetail}
          onClose={() => setCurrentRecordDetail(undefined)}
        />
      )}
    </PageContainer>
  );
};
export default Batch;
