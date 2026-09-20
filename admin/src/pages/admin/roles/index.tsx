import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { ProTable, ProCard } from '@ant-design/pro-components';
import { Button } from 'antd';
import { useRef } from 'react';
import { omit } from 'lodash';
import { Plus, SquarePen } from 'lucide-react';
import { useIntl } from '@umijs/max';

import { PageContainer, IconButton } from '@/components';
import request from '@/utils/request';
import { formPageParams, formatTime } from '@/utils/fomatData';
import { l } from '@/utils/intl';
import type { RoleListItem } from '@/types/Public/data';
import RoleModal from './components/RoleModal';

const Roles = () => {
  const actionRef = useRef<ActionType>();
  const { locale } = useIntl();
  const onSubmitBack = () => {
    actionRef.current?.reload();
  };
  const columns: ProColumns<RoleListItem>[] = [
    {
      title: l('admin.roles.name'),
      dataIndex: 'role',
      hideInSearch: true,
    },
    {
      title: l('admin.roles.updateTime'),
      dataIndex: 'update_ts',
      hideInSearch: true,
      render: (txt) => (txt !== null ? formatTime(Number(txt)) : '-'),
    },
    {
      title: l('global.actions.action'),
      dataIndex: 'option',
      valueType: 'option',
      width: locale === 'ja-JP' ? 110 : 80,
      render: (_, record) => [
        <RoleModal
          key="edit"
          initValues={omit(record, 'update_ts')}
          type="edit"
          onSubmitBack={onSubmitBack}
        >
          <IconButton className="!w-7 !h-7 hover:text-primary hover:bg-primary/15">
            <SquarePen size={16} />
          </IconButton>
        </RoleModal>,
      ],
    },
  ];

  return (
    <PageContainer
      title={l('menu.admin.roles')}
      subTitle={l('admin.roles.subTitle')}
      extraContent={
        <RoleModal key="primary" onSubmitBack={onSubmitBack}>
          <Button type="primary" size="large" icon={<Plus size={16} />}>
            {l('admin.roles.create')}
          </Button>
        </RoleModal>
      }
    >
      <ProCard bodyStyle={{ padding: 0 }}>
        <ProTable
          actionRef={actionRef}
          rowKey="role"
          search={false}
          toolBarRender={false}
          columns={columns}
          request={async (params: { pageSize: number; current: number }) => {
            const res = await request('/roles', { params: formPageParams(params) });
            return {
              data: res.data.data?.results || [],
              success: true,
              total: res.data.data?.count,
            };
          }}
        />
      </ProCard>
    </PageContainer>
  );
};

export default Roles;
