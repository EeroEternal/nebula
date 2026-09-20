import { ProTable } from '@ant-design/pro-components';
import type { ProColumns, ActionType } from '@ant-design/pro-components';
import { App, Button, Tag, Dropdown } from 'antd';
import { history } from '@umijs/max';
import { useRequest } from 'ahooks';
import {
  RefreshCw,
  Plus,
  MoreHorizontal,
  Eye,
  Play,
  SquarePen,
  Trash2,
  Rocket,
} from 'lucide-react';
import { useRef, useState } from 'react';
import { mapValues, isNumber } from 'lodash';

import { PageContainer } from '@/components';
import DeployModelInstance from '@/components/DeployModelInstance';
import { l, lGet } from '@/utils/intl';
import { ALL_LIST_PAGES_PARAMS, UNLIMITED } from '@/constants';
import { TASK_STATUS_COLORS } from '@/constants/finetune';
import { ModelType } from '@/constants/modelData';
import { formatTime, formPageParams } from '@/utils/fomatData';
import { FinetuneListItem } from '@/types/Public/data';
import request from '@/utils/request';
import TaskDetail from './components/TaskDetail';

const FinetuneTasks = () => {
  const { modal } = App.useApp();
  const proTableRef = useRef<ActionType>();
  const [currentRecordDetail, setCurrentRecordDetail] = useState<FinetuneListItem | undefined>(
    undefined,
  );
  const [taskStatusCounts, setTaskStatusCounts] = useState({
    runningCount: 0,
    pendingCount: 0,
    finishedCount: 0,
    failedCount: 0,
  });
  const { run: refreshTaskStatusCounts, loading } = useRequest(
    () => request('/tasks', { params: ALL_LIST_PAGES_PARAMS }),
    {
      onSuccess: (res) => {
        if (res.success) {
          const results = (res?.data?.results || []) as FinetuneListItem[];
          const counts = results.reduce(
            (acc, item) => {
              switch (item.task_status) {
                case 'running':
                  acc.runningCount++;
                  break;
                case 'pending':
                  acc.pendingCount++;
                  break;
                case 'finished':
                  acc.finishedCount++;
                  break;
                case 'failed':
                  acc.failedCount++;
                  break;
                default:
                  break;
              }
              return acc;
            },
            { runningCount: 0, pendingCount: 0, finishedCount: 0, failedCount: 0 },
          );
          setTaskStatusCounts(counts);
        }
      },
    },
  );
  const refresh = () => {
    proTableRef.current?.reset?.();
    proTableRef.current?.reload();
    refreshTaskStatusCounts();
  };
  const { runAsync: deleteTask } = useRequest(
    (taskId) => request(`/tasks/${taskId}`, { method: 'delete' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          refresh();
        }
      },
    },
  );
  const { run: startTask } = useRequest(
    (taskId) => request(`/tasks/${taskId}/start`, { method: 'post' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          refresh();
        }
      },
    },
  );

  const handleToCreateTask = () => history.push('/tasks/finetune/create');
  const handleToEditTask = (taskId: number) => history.push(`/tasks/finetune/${taskId}`);
  const handleDeleteTask = (taskId: number) => {
    modal.confirm({
      title: lGet('tasks.finetune.delete.tips'),
      onOk: () => deleteTask(taskId),
    });
  };
  const columns: ProColumns<FinetuneListItem>[] = [
    {
      title: l('tasks.finetune.taskId'),
      dataIndex: 'task_id',
      width: '100px',
      hideInSearch: true,
    },
    {
      title: l('tasks.finetune.taskName'),
      dataIndex: 'task_name',
      width: '120px',
    },
    {
      title: l('tasks.finetune.modelName'),
      dataIndex: 'model_name',
      width: '120px',
      render: (_, record) => record?.model_config?.model_name || '-',
    },
    {
      title: l('tasks.finetune.modelVersion'),
      dataIndex: 'model_version',
      width: '180px',
      hideInSearch: true,
      render: (_, record) => record?.model_config?.model_version || '-',
    },
    {
      title: l('tasks.finetune.modelType'),
      dataIndex: 'model_type',
      width: '120px',
      hideInSearch: true,
      render: (_, record) => record?.model_config?.model_type || '-',
    },
    {
      title: l('tasks.finetune.status'),
      dataIndex: 'task_status',
      width: '100px',
      valueEnum: {
        [UNLIMITED]: l('global.data.unlimited'),
        ...mapValues(TASK_STATUS_COLORS, (_, key) => l(`tasks.finetune.status.${key}`)),
      },
      initialValue: UNLIMITED,
      render: (_, record) => {
        return (
          <Tag
            bordered={false}
            color={
              TASK_STATUS_COLORS[record.task_status as keyof typeof TASK_STATUS_COLORS] || 'default'
            }
          >
            {l(`tasks.finetune.status.${record.task_status}`)}
          </Tag>
        );
      },
    },

    {
      title: l('tasks.finetune.createTime'),
      dataIndex: 'create_ts',
      width: '180px',
      hideInSearch: true,
      render: (val) => (isNumber(val) ? formatTime(val) : '-'),
    },
    {
      title: l('tasks.finetune.startTime'),
      dataIndex: 'start_ts',
      width: '180px',
      hideInSearch: true,
      render: (val) => (isNumber(val) ? formatTime(val) : '-'),
    },
    {
      title: l('tasks.finetune.endTime'),
      dataIndex: 'finish_ts',
      width: '180px',
      hideInSearch: true,
      render: (val) => (isNumber(val) ? formatTime(val) : '-'),
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
            key: 'edit',
            show: record.task_status === 'pending',
            icon: <SquarePen size={16} />,
            label: l('global.actions.edit'),
            onClick: () => handleToEditTask(record.task_id),
          },
          {
            key: 'start',
            show: record.task_status === 'pending',
            icon: <Play size={16} />,
            label: l('global.actions.start'),
            onClick: () => startTask(record.task_id),
          },
          {
            key: 'deploy',
            show: record.task_status === 'finished',
            label: (
              <DeployModelInstance
                key="instance"
                type="add"
                initialValues={{
                  model_name: record?.model_config?.model_name,
                  model_version: record?.model_config?.model_version,
                  peft_model_config: {
                    lora_list: (record?.data_config?.dataset || []).map((loraName: string) => ({
                      lora_name: loraName,
                      local_path: record?.output_config?.output_dir,
                    })),
                  },
                }}
                modelType={record?.model_config?.model_type as ModelType}
              >
                <div className="flex items-center gap-2">
                  <Rocket size={16} />
                  {l('global.actions.deploy')}
                </div>
              </DeployModelInstance>
            ),
          },
          {
            key: 'delete',
            show: !['retrying', 'running'].includes(record.task_status),
            icon: <Trash2 size={16} />,
            label: l('global.actions.delete'),
            className: '!text-danger',
            onClick: () => handleDeleteTask(record.task_id),
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
      title={l('menu.tasks.finetune')}
      subTitle={l('tasks.finetune.subTitle')}
      extraContent={
        <div className="flex gap-3">
          <Button size="large" icon={<RefreshCw size={14} />} loading={loading} onClick={refresh}>
            {l('global.actions.refresh')}
          </Button>
          <Button
            type="primary"
            size="large"
            icon={<Plus size={14} />}
            onClick={handleToCreateTask}
          >
            {l('tasks.finetune.create')}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-6">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 ">
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.finetune.status.running')}</div>
            <div className="text-3xl font-bold text-blue-500">{taskStatusCounts.runningCount}</div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.finetune.status.pending')}</div>
            <div className="text-3xl font-bold text-orange-500">
              {taskStatusCounts.pendingCount}
            </div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.finetune.status.finished')}</div>
            <div className="text-3xl font-bold text-green-500">
              {taskStatusCounts.finishedCount}
            </div>
          </div>
          <div className="rounded-lg bg-[var(--c-surface)] text-default shadow-card border border-[color:var(--c-border-light)] p-6">
            <div className="mb-2 text-muted font-medium">{l('tasks.finetune.status.failed')}</div>
            <div className="text-3xl font-bold text-danger">{taskStatusCounts.failedCount}</div>
          </div>
        </div>
        <ProTable
          toolBarRender={false}
          rowKey="task_id"
          actionRef={proTableRef}
          columns={columns}
          search={{
            className: 'pro-table-filter',
            span: 6,
            collapseRender: () => null,
          }}
          scroll={{ x: 1100 }}
          pagination={{
            pageSize: 10,
          }}
          request={async (params: {
            pageSize: number;
            current: number;
            task_status: string;
            task_name?: string;
            model_name: string;
          }) => {
            const pageParams = formPageParams(params);
            const res = await request('/tasks', {
              params: {
                ...pageParams,
                task_status: params?.task_status !== UNLIMITED ? params?.task_status : undefined,
              },
            });
            return {
              data: res?.data?.results || [],
              total: res.data?.count || 0,
              success: true,
            };
          }}
        />
      </div>
      {!!currentRecordDetail && (
        <TaskDetail
          taskDetail={currentRecordDetail}
          onClose={() => setCurrentRecordDetail(undefined)}
        />
      )}
    </PageContainer>
  );
};
export default FinetuneTasks;
