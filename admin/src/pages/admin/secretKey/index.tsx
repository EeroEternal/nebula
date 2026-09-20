import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { ProTable, ProCard, ModalForm, ProFormText } from '@ant-design/pro-components';
import { Button, App } from 'antd';
import { useRef, useState } from 'react';
import { Plus, Eye, EyeOff, Copy, Trash2 } from 'lucide-react';

import { PageContainer, IconButton } from '@/components';
import { copyToClipboard, sleep } from '@/utils';
import { formPageParams, formatTime } from '@/utils/fomatData';
import { l, lGet } from '@/utils/intl';
import type { CreateParams, TableListItem } from './data';
import { create, deleteSerects, getList } from './service';

const SecretKey = () => {
  const { message, modal } = App.useApp();
  const actionRef = useRef<ActionType>();
  const [createModalVisible, setCreateModalVisible] = useState<boolean>(false);
  const [visibleMap, setVisibleMap] = useState<Record<string, boolean>>({});

  const handleSubmit = async (params: CreateParams) => {
    const res = await create(params);
    if (res.success) {
      setCreateModalVisible(false);
      message.success(lGet('global.message.addSuccess'));
      actionRef.current?.reload();
    }
  };

  const handleDelete = async (record: TableListItem) => {
    modal.confirm({
      title: lGet('admin.secretKey.delete.tips', undefined, { key: record.name }),
      onOk: async () => {
        const res = await deleteSerects(record);
        if (res.success) {
          message.success(lGet('global.message.deleteSuccess'));
          setVisibleMap((prev) => {
            const newState = { ...prev };
            delete newState[record.secrets];
            return newState;
          });
          actionRef.current?.reload();
        }
      },
    });
  };
  const toggleVisible = async (key: string) => {
    setVisibleMap((prev) => ({ ...prev, [key]: !prev[key] }));
    await sleep(3000);
    setVisibleMap((prev) => ({ ...prev, [key]: false }));
  };
  const columns: ProColumns<TableListItem>[] = [
    {
      title: l('admin.secretKey.name'),
      dataIndex: 'name',
      width: '120px',
    },
    {
      title: l('admin.secretKey.key'),
      dataIndex: 'secrets',
      render: (_, record) => {
        const isVisible = visibleMap[record.secrets] || false;
        return (
          <div className="flex items-center gap-2">
            <code className="text-sm bg-background-muted px-2 py-1 rounded font-mono">
              {isVisible
                ? record.secrets
                : record.secrets.substring(0, 5) + '*'.repeat(record.secrets.length - 5)}
            </code>
            {isVisible ? (
              <IconButton
                className="!w-7 !h-7 hover:text-primary hover:bg-primary/15"
                onClick={() => toggleVisible(record.secrets)}
              >
                <EyeOff size={16} />
              </IconButton>
            ) : (
              <IconButton
                className="!w-7 !h-7 hover:text-primary hover:bg-primary/15"
                onClick={() => toggleVisible(record.secrets)}
              >
                <Eye size={16} />
              </IconButton>
            )}
            <IconButton
              className="!w-7 !h-7 hover:text-primary hover:bg-primary/15"
              onClick={() => copyToClipboard(record.secrets)}
            >
              <Copy size={14} />
            </IconButton>
          </div>
        );
      },
    },
    {
      title: l('admin.secretKey.createTime'),
      dataIndex: 'created_ts',
      hideInSearch: true,
      render: (txt) => (txt !== null ? formatTime(Number(txt)) : '-'),
    },
    {
      title: l('global.actions.action'),
      dataIndex: 'option',
      valueType: 'option',
      render: (_, record) => [
        <IconButton key="delete" className="!w-7 !h-7 !text-danger hover:bg-danger/10">
          <Trash2 size={16} onClick={() => handleDelete(record)} />
        </IconButton>,
      ],
    },
  ];

  return (
    <PageContainer
      title={l('menu.admin.secretKey')}
      subTitle={l('admin.secretKey.subTitle')}
      extraContent={
        <Button
          size="large"
          type="primary"
          onClick={() => setCreateModalVisible(true)}
          icon={<Plus size={16} />}
        >
          {l('admin.secretKey.create')}
        </Button>
      }
    >
      <ProCard bodyStyle={{ padding: 0 }}>
        <ProTable
          actionRef={actionRef}
          rowKey="secrets"
          search={false}
          toolBarRender={false}
          request={async (params: { pageSize: number; current: number }) => {
            const res = await getList(formPageParams(params));
            return {
              data: res.data?.results || [],
              success: true,
              total: res.data?.count || 0,
            };
          }}
          columns={columns}
        />
      </ProCard>
      <ModalForm
        width={480}
        title={l('admin.secretKey.create')}
        className="mt-4"
        visible={createModalVisible}
        onFinish={handleSubmit}
        modalProps={{
          destroyOnClose: true,
          onCancel: () => setCreateModalVisible(false),
        }}
      >
        <ProFormText
          label={l('admin.secretKey.name')}
          name="name"
          placeholder={l('admin.secretKey.name.rule')}
          rules={[{ required: true, message: l('admin.secretKey.name.rule') }]}
        />
      </ModalForm>
    </PageContainer>
  );
};

export default SecretKey;
