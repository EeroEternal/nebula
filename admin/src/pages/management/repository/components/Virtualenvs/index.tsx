import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { ProTable, ProCard } from '@ant-design/pro-components';
import { App } from 'antd';
import { FC, useRef } from 'react';
import { useParams } from '@umijs/max';
import { Copy, Trash2 } from 'lucide-react';

import { l, lGet } from '@/utils/intl';
import { ModelVirtualEnvsItem } from '@/types/Public/data';
import { IconButton } from '@/components';
import request from '@/utils/request';
import { copyToClipboard } from '@/utils';

const Virtualenvs: FC = () => {
  const { modal, message } = App.useApp();
  const params = useParams();
  const { modelName } = params || {};
  const actionRef = useRef<ActionType>();

  const handleDelete = async (record: ModelVirtualEnvsItem) => {
    modal.confirm({
      title: lGet('models.repository.detail.virtualenvs.actions.delete.tips'),
      onOk: async () => {
        const res = await request(
          `/virtualenvs?model_name=${modelName}&model_engine=${record.model_engine}&python_version=${record.python_version}&worker_ip=${record.actor_ip_address}`,
          {
            method: 'delete',
          },
        );
        if (res.success) {
          message.success(lGet('global.message.deleteSuccess'));
          actionRef.current?.reload();
        }
      },
    });
  };

  const columns: ProColumns<ModelVirtualEnvsItem>[] = [
    {
      title: l('models.repository.detail.virtualenvs.ip'),
      dataIndex: 'actor_ip_address',
    },
    {
      title: l('models.repository.detail.virtualenvs.modelEngine'),
      dataIndex: 'model_engine',
    },
    {
      title: l('models.repository.detail.virtualenvs.path'),
      dataIndex: 'path',
      render: (dom) => (
        <div className="flex items-center gap-1">
          <div className="truncate">{dom}</div>
          <IconButton
            className="!w-6 !h-6 hover:text-primary hover:bg-primary/15"
            onClick={() => copyToClipboard(dom as string)}
          >
            <Copy size={14} />
          </IconButton>
        </div>
      ),
    },
    {
      title: l('models.repository.detail.virtualenvs.pythonVersion'),
      dataIndex: 'python_version',
      width: '160px',
    },
    {
      title: l('global.actions.action'),
      dataIndex: 'option',
      valueType: 'option',
      fixed: 'right',
      width: '110px',
      render: (_, record) => [
        <IconButton
          key="delete"
          className="!w-7 !h-7 !text-danger hover:bg-danger/10"
          onClick={() => handleDelete(record)}
        >
          <Trash2 size={16} />
        </IconButton>,
      ],
    },
  ];
  return (
    <ProCard bodyStyle={{ padding: 0 }}>
      <ProTable<ModelVirtualEnvsItem>
        toolBarRender={false}
        rowKey={(record) =>
          `${record.actor_ip_address}-${record.model_engine}-${record.python_version}`
        }
        actionRef={actionRef}
        columns={columns}
        search={false}
        scroll={{ x: 700 }}
        request={async () => {
          const res = await request(`/virtualenvs?model_name=${modelName}`);
          return {
            data: res?.data?.list || [],
            success: true,
          };
        }}
      />
    </ProCard>
  );
};

export default Virtualenvs;
