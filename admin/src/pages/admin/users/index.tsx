import type { ActionType, ProColumns } from '@ant-design/pro-components';
import { ProTable, ProCard } from '@ant-design/pro-components';
import { Button, Input, Tag } from 'antd';
import type { InputProps } from 'antd';
import React, { useRef, useState, useMemo } from 'react';
import { debounce } from 'lodash';
import { Plus, Search, SquarePen } from 'lucide-react';

import { PageContainer, IconButton, FilterBar } from '@/components';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { formPageParams, formatTime } from '@/utils/fomatData';
import CreateModal from './components/CreateModal';
import UpdateModal from './components/UpdateModal';
import type { TableListItem } from './data';

const Users: React.FC = () => {
  const [createModalVisible, setCreateModalVisible] = useState<boolean>(false);
  const [updateModalVisible, setUpdateModalVisible] = useState<boolean>(false);
  const [formValues, setFormValues] = useState<TableListItem>();
  const actionRef = useRef<ActionType>();
  const [searchValue, setSearchValue] = useState<string>('');

  const columns: ProColumns<TableListItem>[] = [
    {
      title: l('admin.users.account'),
      dataIndex: 'account',
    },
    {
      title: l('admin.users.name'),
      dataIndex: 'username',
    },
    {
      title: l('admin.users.role'),
      dataIndex: 'role',
    },
    {
      title: l('admin.users.email'),
      dataIndex: 'email',
    },
    {
      title: l('admin.users.accountStatus'),
      dataIndex: 'status',
      render: (val) => {
        if (val === 'enabled') {
          return (
            <Tag color="success" bordered={false}>
              {l('admin.users.accountStatus.enabling')}
            </Tag>
          );
        }
        return (
          <Tag color="default" bordered={false}>
            {l('admin.users.accountStatus.disabled')}
          </Tag>
        );
      },
    },
    {
      title: l('admin.users.lastLoginTime'),
      dataIndex: 'last_login_ts',
      render: (txt) => (Number(txt) ? formatTime(Number(txt)) : '-'),
    },
    {
      title: l('global.actions.action'),
      dataIndex: 'option',
      valueType: 'option',
      render: (_, record) => [
        <IconButton
          key="edit"
          className="!w-7 !h-7  hover:text-primary hover:bg-primary/15"
          onClick={() => {
            setUpdateModalVisible(true);
            setFormValues(record);
          }}
        >
          <SquarePen size={16} />
        </IconButton>,
      ],
    },
  ];

  const updateTableList = () => {
    setSearchValue('');
    actionRef.current?.reload?.();
  };
  const debounceSearch = useMemo(
    () =>
      debounce(() => {
        actionRef.current?.reload();
      }, 500),
    [],
  );
  const handleInputChange: InputProps['onChange'] = (e) => {
    setSearchValue(e.target.value);
    debounceSearch();
  };
  return (
    <PageContainer
      title={l('menu.admin.users')}
      subTitle={l('admin.users.subTitle')}
      extraContent={
        <Button
          type="primary"
          onClick={() => setCreateModalVisible(true)}
          icon={<Plus size={16} />}
        >
          {l('admin.users.create')}
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        <FilterBar>
          <Input
            className="w-full sm:w-72"
            placeholder={l('admin.users.searchName')}
            prefix={<Search className="text-muted mr-1" size={16} />}
            value={searchValue}
            onChange={handleInputChange}
            allowClear
          />
        </FilterBar>
        <ProCard bodyStyle={{ padding: 0 }}>
          <ProTable
            actionRef={actionRef}
            rowKey="username"
            search={false}
            toolBarRender={false}
            columns={columns}
            request={async (params: { pageSize: number; current: number }) => {
              const newParams = {
                ...formPageParams(params),
                username: searchValue || undefined,
              };
              const res = await request('/users', { params: newParams });
              return {
                data: res.data?.results || [],
                success: true,
                total: res.data?.count || 0,
              };
            }}
          />
        </ProCard>
      </div>

      {createModalVisible && (
        <CreateModal
          visible={createModalVisible}
          onCancel={() => setCreateModalVisible(false)}
          onSubmit={updateTableList}
        />
      )}
      {updateModalVisible && (
        <UpdateModal
          visible={updateModalVisible}
          onCancel={() => setUpdateModalVisible(false)}
          onSubmit={updateTableList}
          values={formValues}
        />
      )}
    </PageContainer>
  );
};

export default Users;
