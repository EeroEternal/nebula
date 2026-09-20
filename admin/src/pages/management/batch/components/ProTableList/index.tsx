import { EyeOutlined, PauseCircleOutlined, DeleteOutlined } from '@ant-design/icons';
import { ProTable } from '@ant-design/pro-components';
import type { ActionType } from '@ant-design/pro-components';
import { Button, Tag, Popconfirm } from 'antd';
import React, { useRef, useState } from 'react';
import { ActionWithTips } from '@/components';
import { PROTABLE_OPTIONS_PUBLIC } from '@/constants';
import { batchStatus } from '@/constants/batch';
import { l } from '@/utils/intl';
import type { TableListItem } from '../../data';
import { cancelBatch, deleteBatch, getBatchList as getTableList } from '../../service';
import DetailComponent from '../Detail';
import CreateBatchModal from '../CreateBatchModal';

const ProTableList: React.FC = () => {
  /**
   * delete = 0
    validating = 1
    waiting = 2
    running = 3
    completed = 4
    cancelling = 5
    cancelled = 6
    failed = 7
   */
  let statusValueEnum = {};
  batchStatus.forEach((item, index) => {
    statusValueEnum[index] = {
      text: l(`model.batch.status.${item.value}`),
      status: item.value,
      // text: l(`model.batch.status.${item.value}`),
      // status: item.value,
    };
  });
  /**
   * status
   */
  const [modelListState, setModelListState] = useState<TableListItem>({
    loading: false,
    value: {},
    createOpen: false,
    editOpen: false,
  });
  const actionRef = useRef<ActionType>();
  const executeAndCallbackRefresh = async (callback: () => void) => {
    setModelListState((prevState) => ({ ...prevState, loading: true }));
    await callback();
    setModelListState((prevState) => ({ ...prevState, loading: false }));
    actionRef.current?.reload?.();
  };

  /**
   * delete model by id
   * @param id model id
   */
  const handleCancelSubmit = async (id: number) => {
    await executeAndCallbackRefresh(async () => cancelBatch({ id }));
    // await executeAndCallbackRefresh(async () => removeById(`${API_CONSTANTS.LLM_MODEL}${id}/`, {}));
  };
  /**
   * delete model by id
   * @param id model id
   */
  const handleDeleteSubmit = async (id: number) => {
    await executeAndCallbackRefresh(async () => deleteBatch({ id }));
    // await executeAndCallbackRefresh(async () => removeById(`${API_CONSTANTS.LLM_MODEL}${id}/`, {}));
  };

  /**
   * 显示模型详情
   */
  const handleClickViewModelList = (record: TableListItem & { id?: number }) => {
    setModelListState((prevState) => ({
      ...prevState,
      value: record,
      viewDetailOpen: true,
    }));
  };

  /**
   * columns
   */
  const columns = [
    {
      title: 'ID',
      dataIndex: 'id',
      width: '80px',
      hideInSearch: true,
      ellipsis: {
        showTitle: false,
      },
    },
    {
      title: l('model.batch.endpoint'),
      dataIndex: 'endpoint',
      width: '120px',
      ellipsis: {
        showTitle: false,
      },
      hideInSearch: true,
    },
    {
      title: l('model.batch.inputFileId'),
      dataIndex: 'input_file_id',
      width: '120px',
      hideInSearch: true,
      ellipsis: {
        showTitle: true,
      },
    },
    {
      title: l('model.batch.outputFileId'),
      dataIndex: 'output_file_id',
      hideInSearch: true,
      width: '120px',
      ellipsis: {
        showTitle: true,
      },
    },
    {
      title: l('model.batch.completionWindow'),
      dataIndex: 'completion_window',
      hideInSearch: true,
      width: '120px',
      ellipsis: {
        showTitle: true,
      },
    },
    {
      title: l('model.tuning.status'),
      dataIndex: 'status',
      width: '100px',
      align: 'center',
      valueEnum: {
        '': { text: l('model.tuning.all'), status: '' },
        ...statusValueEnum,
      },
      render: (_, record) => {
        const color = {
          deleted: 'red',
          validating: 'blue',
          waiting: 'orange',
          running: 'blue',
          completed: 'green',
          cancelling: 'orange',
          cancelled: 'red',
          failed: 'red',
        };
        return (
          <Tag color={color[statusValueEnum[record.status].status] || 'default'}>
            {statusValueEnum[record.status].text}
          </Tag>
        );
      },
    },
    {
      title: l('model.tuning.createTime'),
      dataIndex: 'created_at',
      hideInSearch: true,
      align: 'center',
      width: '180px',
      render: (txt) => txt.split('T').join(' '),
    },
    /**
     * 创建任务：创建一个新的微调任务，指定模型、数据集和微调参数等
     * 启动任务：启动一个处于pending状态的任务，进入running状态
     * 暂停任务：暂停一个正在执行的任务，将任务状态从running变为paused
     * 恢复任务：恢复一个已暂停的任务，将任务状态从paused变为running。
     * 取消任务：取消一个正在执行或待执行的任务，将任务状态变为cancelled。
     * 重试任务：重试一个执行失败的任务，将任务状态从failed变为retrying或重新启动任务
     * 删除任务：删除一个已完成、已失败或已取消的任务，从任务列表中移除
     * 查看日志：查看任务执行过程中的详细日志信息，帮助诊断问题
     */
    {
      title: l('model.tuning.actions'),
      valueType: 'option',
      fixed: 'right',
      width: 120,
      render: (_: unknown, record) => [
        <ActionWithTips title={l('global.actions.detail')} key="read">
          <EyeOutlined onClick={() => handleClickViewModelList(record)} />
        </ActionWithTips>,
        // record.status === 'pending' && (
        //   <CreateBatchModal submitBack={() => actionRef.current?.reload()} key="edit" initValues={record}>
        //     <ActionWithTips title={l('global.actions.edit')} key="edit">
        //       <EditOutlined />
        //     </ActionWithTips>
        //   </CreateBatchModal>
        // ),
        [1, 2, 3].includes(record.status) && (
          <Popconfirm
            key="cancel"
            title={l('model.batch.cancelTips')}
            okText={l('model.running.ok')}
            cancelText={l('model.running.cancel')}
            onConfirm={() => handleCancelSubmit(record.id)}
          >
            <PauseCircleOutlined className="text-orange-400" />
          </Popconfirm>
        ),
        <Popconfirm
          key="delete"
          title={l('model.batch.deleteTips')}
          okText={l('model.running.ok')}
          cancelText={l('model.running.cancel')}
          onConfirm={() => handleDeleteSubmit(record.id)}
        >
          <DeleteOutlined className="text-red-500" />
        </Popconfirm>,
      ],
    },
  ];

  /**
   * render
   */
  return (
    <>
      <ProTable
        {...PROTABLE_OPTIONS_PUBLIC}
        headerTitle={l('model.batch.taskList')}
        actionRef={actionRef}
        toolBarRender={() => [
          <CreateBatchModal submitBack={() => actionRef.current?.reload()}>
            <Button type="primary" key="primary">
              {l('pages.searchTable.create')}
            </Button>
          </CreateBatchModal>,
        ]}
        request={async (params, sorter, filter: Record<string, unknown>) => {
          const filteredParams = {
            ...sorter,
            ...filter,
            ...params,
          };
          if (filteredParams.status === '') {
            delete filteredParams.status;
          }
          const res = await getTableList({
            curPageNum: params.current,
            numPerPage: params.pageSize || 10,
            ...filteredParams,
          });
          const resData = res?.data?.data || {};
          return {
            data: resData.result || [],
            total: resData.count,
            success: true,
          };
        }}
        columns={columns}
        pagination={{
          pageSize: 10,
          // showTotal: (total) => `总共 ${total} 条`, // 显示总数
        }}
        search={{ defaultCollapsed: false }} //展开搜索栏
        form={{
          initialValues: {
            status: '',
          },
        }}
      />
      {/* read */}
      <DetailComponent
        values={modelListState.value}
        open={modelListState.viewDetailOpen}
        loading={modelListState.loading}
        onClose={() => setModelListState((prevState) => ({ ...prevState, viewDetailOpen: false }))}
      />
    </>
  );
};
export default ProTableList;
