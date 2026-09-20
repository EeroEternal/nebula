import React, { useCallback, useEffect, useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Popconfirm,
  Space,
  Table,
  Tag,
} from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';

import {
  createConsoleRole,
  deleteConsoleRole,
  fetchConsoleRoles,
  syncConsoleRole,
  updateConsoleRole,
} from '@/utils/consoleApi';
import { useConsoleReachable } from '@/hooks/useConsoleReachable';
import ConsoleUnavailable from '../ConsoleUnavailable';

const DEFAULT_PERMS = JSON.stringify(
  {
    console: ['*'],
    site: {
      action: {
        models: ['list', 'read'],
        instances: ['list', 'read'],
        users: ['list'],
        roles: ['list'],
        secrets: ['list'],
        tasks: ['list', 'read'],
        caches: ['list'],
        virtualenv: ['list'],
      },
      page: {
        dashboard: [],
        models: ['repository', 'instances'],
        tasks: [],
        monitor: [],
        admin: [],
      },
    },
  },
  null,
  2,
);

const ConsoleRolesPage: React.FC = () => {
  const { reachable: consoleReachable, probed: consoleProbed } = useConsoleReachable();
  type RoleRow = { name: string; permissions?: unknown };
  const [roles, setRoles] = useState<RoleRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<RoleRow | 'new' | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm();
  const { message, modal } = App.useApp();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setRoles(await fetchConsoleRoles());
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

  const showSync = (sync: { name?: string; success?: boolean; detail?: string }[]) => {
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
                ok ? <Tag color="success">成功</Tag> : <Tag color="error">失败</Tag>,
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
            form.setFieldsValue({ name: '', permissions: DEFAULT_PERMS });
          }}
        >
          新建全局角色
        </Button>
        <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
          刷新
        </Button>
      </Space>
      <Table
        rowKey="name"
        loading={loading}
        dataSource={roles}
        columns={[
          { title: '角色名', dataIndex: 'name' },
          {
            title: '权限',
            dataIndex: 'permissions',
            render: (p: unknown) => (
              <code className="text-[12px]">{JSON.stringify(p)}</code>
            ),
          },
          {
            title: '操作',
            key: 'action',
            render: (_: unknown, record: RoleRow) => (
              <Space>
                <Button
                  size="small"
                  onClick={() => {
                    setEditing(record);
                    form.setFieldsValue({
                      name: record.name,
                      permissions: JSON.stringify(record.permissions, null, 2),
                    });
                  }}
                >
                  编辑
                </Button>
                <Button
                  size="small"
                  onClick={async () => {
                    try {
                      const data = await syncConsoleRole(record.name);
                      showSync(data.sync);
                    } catch (e) {
                      message.error((e as Error).message);
                    }
                  }}
                >
                  重试同步
                </Button>
                <Popconfirm
                  title={`确认删除角色 ${record.name}？将同步删除各站角色。`}
                  onConfirm={async () => {
                    try {
                      const data = await deleteConsoleRole(record.name);
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
        title={editing === 'new' ? '新建全局角色' : '编辑全局角色'}
        open={editing !== null}
        confirmLoading={saving}
        onCancel={() => setEditing(null)}
        onOk={async () => {
          const values = await form.validateFields();
          let permissions: unknown;
          try {
            permissions = JSON.parse(values.permissions);
          } catch {
            message.error('permissions 必须是合法 JSON');
            return;
          }
          setSaving(true);
          try {
            let data;
            if (editing === 'new') {
              data = await createConsoleRole({ name: values.name, permissions });
            } else {
              data = await updateConsoleRole(values.name, { permissions });
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
          <Form.Item name="name" label="角色名" rules={[{ required: true }]}>
            <Input disabled={editing !== 'new'} />
          </Form.Item>
          <Form.Item
            name="permissions"
            label="权限 JSON（console 字段控制 Console RBAC）"
            rules={[{ required: true }]}
            extra='console 控制 Console RBAC；site 下发到各站。示例含 console+site。'
          >
            <Input.TextArea rows={8} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
};

export default ConsoleRolesPage;
