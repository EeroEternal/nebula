import React, { useCallback, useEffect, useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Popconfirm,
  Select,
  Space,
  Table,
  Tag,
} from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';

import {
  createConsoleUser,
  deleteConsoleUser,
  fetchConsoleRoles,
  fetchConsoleUsers,
  syncConsoleUser,
  updateConsoleUser,
} from '@/utils/consoleApi';
import { useConsoleReachable } from '@/hooks/useConsoleReachable';
import { l } from '@/utils/intl';
import ConsoleUnavailable from '../ConsoleUnavailable';

type ConsoleUserRow = {
  username: string;
  email?: string;
  account?: string;
  role?: string;
  status?: string;
  locale?: string;
};
type ConsoleRoleRow = { name: string };
type SyncResultRow = { name: string; success?: boolean; detail?: string };

const ConsoleUsersPage: React.FC = () => {
  const { reachable: consoleReachable, probed: consoleProbed } = useConsoleReachable();
  const [users, setUsers] = useState<ConsoleUserRow[]>([]);
  const [roles, setRoles] = useState<ConsoleRoleRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<ConsoleUserRow | 'new' | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm();
  const [keyword, setKeyword] = useState('');
  const { message, modal } = App.useApp();
  const filteredUsers = users.filter((u) =>
    String(u?.username || '')
      .toLowerCase()
      .includes(keyword.trim().toLowerCase()),
  );

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [u, r] = await Promise.all([fetchConsoleUsers(), fetchConsoleRoles()]);
      setUsers(u);
      setRoles(r);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => {
    if (consoleReachable) {
      load();
    }
  }, [load, consoleReachable]);

  const showSync = (sync: SyncResultRow[]) => {
    modal.info({
      title: '同步结果',
      width: 640,
      content: (
        <Table
          size="small"
          pagination={false}
          rowKey="name"
          dataSource={sync || []}
          columns={[
            { title: 'Region', dataIndex: 'name' },
            {
              title: '结果',
              dataIndex: 'success',
              render: (ok: boolean) =>
                ok ? <Tag color="success">{l('model.batch.request.completed')}</Tag> : <Tag color="error">{l('model.running.model.replica.status.FAILED')}</Tag>,
            },
            { title: '详情', dataIndex: 'detail', ellipsis: true },
          ]}
        />
      ),
    });
  };

  if (!consoleProbed) {
    return null;
  }
  if (!consoleReachable) {
    return <ConsoleUnavailable />;
  }

  return (
    <div className="p-[16px]">
      <Space className="mb-[16px]">
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing('new');
            form.resetFields();
            form.setFieldsValue({ status: 'enabled', locale: 'zh-CN' });
          }}
        >
          新建全局用户
        </Button>
        <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
          刷新
        </Button>
        <Input
          allowClear
          className="w-[220px]"
          placeholder={l('global.console.users.search')}
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
        />
      </Space>
      <Table
        rowKey="username"
        loading={loading}
        dataSource={filteredUsers}
        columns={[
          { title: l('models.engines.registryUsername'), dataIndex: 'username' },
          { title: l('admin.users.email'), dataIndex: 'email' },
          { title: l('admin.users.role'), dataIndex: 'role' },
          {
            title: l('admin.users.accountStatus'),
            dataIndex: 'status',
            render: (s: string) =>
              s === 'enabled' ? (
                <Tag color="success">{l('global.actions.enable')}</Tag>
              ) : (
                <Tag>{l('admin.users.accountStatus.disabled')}</Tag>
              ),
          },
          {
            title: l('global.actions.action'),
            key: 'action',
            render: (_: unknown, record: ConsoleUserRow) => (
              <Space>
                <Button
                  size="small"
                  onClick={() => {
                    setEditing(record);
                    form.setFieldsValue({
                      username: record.username,
                      email: record.email,
                      account: record.account,
                      role: record.role,
                      status: record.status,
                      locale: record.locale,
                      password: '',
                    });
                  }}
                >
                  编辑
                </Button>
                <Button
                  size="small"
                  onClick={async () => {
                    try {
                      const data = await syncConsoleUser(record.username);
                      showSync(data.sync);
                      message.success('已重试同步');
                    } catch (e) {
                      message.error((e as Error).message);
                    }
                  }}
                >
                  重试同步
                </Button>
                <Popconfirm
                  title={`确认删除全局用户 ${record.username}？各站将先禁用。`}
                  onConfirm={async () => {
                    try {
                      const data = await deleteConsoleUser(record.username);
                      showSync(data.sync);
                      message.success('已删除');
                      load();
                    } catch (e) {
                      message.error((e as Error).message);
                    }
                  }}
                >
                  <Button size="small" danger>
                    删除
                  </Button>
                </Popconfirm>
              </Space>
            ),
          },
        ]}
      />
      <Modal
        title={editing === 'new' ? '新建全局用户' : '编辑全局用户'}
        open={editing !== null}
        confirmLoading={saving}
        onCancel={() => setEditing(null)}
        onOk={async () => {
          const values = await form.validateFields();
          setSaving(true);
          try {
            let data;
            if (editing === 'new') {
              data = await createConsoleUser(values);
            } else {
              const body: Record<string, unknown> = {
                email: values.email,
                account: values.account || values.email,
                role: values.role,
                status: values.status,
                locale: values.locale,
              };
              if (values.password) {
                body.password = values.password;
              }
              data = await updateConsoleUser(values.username, body);
            }
            showSync(data.sync);
            message.success('已保存');
            setEditing(null);
            load();
          } catch (e) {
            message.error((e as Error).message);
          } finally {
            setSaving(false);
          }
        }}
      >
        <Form form={form} layout="vertical">
          <Form.Item name="username" label={l('models.engines.registryUsername')} rules={[{ required: true }]}>
            <Input disabled={editing !== 'new'} />
          </Form.Item>
          <Form.Item name="email" label={l('admin.users.email')} rules={[{ required: true, type: 'email' }]}>
            <Input />
          </Form.Item>
          <Form.Item name="account" label="账号（可空，默认邮箱）">
            <Input />
          </Form.Item>
          <Form.Item
            name="password"
            label={editing === 'new' ? '密码' : '新密码（留空不改）'}
            rules={editing === 'new' ? [{ required: true }] : []}
          >
            <Input.Password />
          </Form.Item>
          <Form.Item name="role" label={l('admin.users.role')} rules={[{ required: true }]}>
            <Select
              options={roles.map((r) => ({ value: r.name, label: r.name }))}
              showSearch
            />
          </Form.Item>
          <Form.Item name="status" label={l('management.cluster.clusterStatus')} rules={[{ required: true }]}>
            <Select
              options={[
                { value: 'enabled', label: '启用' },
                { value: 'disabled', label: '禁用' },
              ]}
            />
          </Form.Item>
          <Form.Item name="locale" label={l('model.repository.language')}>
            <Input />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
};

export default ConsoleUsersPage;
