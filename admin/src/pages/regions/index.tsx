import React, { useCallback, useEffect, useState } from 'react';
import {
  App,
  Badge,
  Button,
  Card,
  Col,
  Form,
  Input,
  Modal,
  Popconfirm,
  Row,
  Statistic,
  Table,
  Tag,
  Typography,
} from 'antd';
import { GlobalOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { formatDisplayTime } from '@/utils';
import {
  createRegion,
  deleteRegion,
  getCurrentRegion,
  setCurrentRegion,
  updateRegion,
  type RegionInfo,
} from '@/utils/region';

interface OverviewData {
  regions: RegionInfo[];
  total: number;
  online: number;
  gpu_count?: number | null;
  model_replicas?: number | null;
}

function consoleHeaders(): Record<string, string> {
  const headers: Record<string, string> = { Accept: 'application/json' };
  try {
    const fromWindow = (window as Window & { CONSOLE_TOKEN?: string }).CONSOLE_TOKEN;
    const consoleToken = fromWindow || localStorage.getItem('powerllm_console_token') || '';
    if (consoleToken) {
      headers.Authorization = `Bearer ${consoleToken}`;
      return headers;
    }
    const user = JSON.parse(localStorage.getItem('user') || '{}');
    if (user?.token && user?.token_type) {
      headers.Authorization = `${user.token_type} ${user.token}`;
    }
  } catch {
    // ignore
  }
  return headers;
}

function formatUsageRatio(ratio?: number | null): string {
  if (ratio == null || Number.isNaN(ratio)) {
    return '-';
  }
  return `${Math.round(ratio * 100)}%`;
}

/** "全部地区"总览页：展示各 region 健康状态并支持一键切换（数据来自 Console /api/console/overview） */
const RegionsOverview: React.FC = () => {
  const [data, setData] = useState<OverviewData | null>(null);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<RegionInfo | null | 'new'>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm();
  const { message } = App.useApp();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await fetch(`${window.location.origin}/api/console/overview`, {
        headers: consoleHeaders(),
      });
      if (resp.ok) {
        setData(await resp.json());
      }
    } catch {
      // 忽略：Console 不可达时保留上次数据
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
    const timer = setInterval(load, 30000);
    return () => clearInterval(timer);
  }, [load]);

  const current = getCurrentRegion();

  const switchTo = (region: string) => {
    setCurrentRegion(region);
    window.location.reload();
  };

  const columns = [
    {
      title: '地区',
      dataIndex: 'name',
      render: (name: string, record: RegionInfo) => (
        <span>
          <GlobalOutlined className="mr-[6px]" />
          {record.display_name || name}
          {name === current && (
            <Tag color="processing" className="ml-[8px]">
              当前
            </Tag>
          )}
        </span>
      ),
    },
    {
      title: '接入点',
      dataIndex: 'endpoint',
      render: (v: string) => <Typography.Text copyable>{v}</Typography.Text>,
    },
    {
      title: '状态',
      dataIndex: ['health', 'status'],
      render: (status?: string) =>
        status === 'online' ? (
          <Badge status="success" text="在线" />
        ) : status === 'offline' ? (
          <Badge status="error" text="不可达" />
        ) : (
          <Badge status="default" text="未知" />
        ),
    },
    {
      title: 'GPU',
      key: 'gpu',
      render: (_: unknown, record: RegionInfo) => {
        const summary = record.health?.summary;
        if (!summary?.available) {
          return <Typography.Text type="secondary">摘要不可用</Typography.Text>;
        }
        return `${summary.gpu_count ?? '-'}（占用 ${formatUsageRatio(
          summary.gpu_usage_ratio,
        )}）`;
      },
    },
    {
      title: '模型副本',
      dataIndex: ['health', 'summary', 'model_replicas'],
      render: (v?: number | null, record?: RegionInfo) =>
        record?.health?.summary?.available ? v ?? '-' : '-',
    },
    {
      title: '最近探测',
      dataIndex: ['health', 'checked_at'],
      render: (ts?: number | null) =>
        formatDisplayTime(ts ? ts * 1000 : undefined),
    },
    {
      title: '操作',
      key: 'action',
      render: (_: unknown, record: RegionInfo) => (
        <>
          <Button
            size="small"
            className="mr-[8px]"
            disabled={record.name === current || record.health?.status !== 'online'}
            onClick={() => switchTo(record.name)}
          >
            切换到该地区
          </Button>
          <Button
            size="small"
            className="mr-[8px]"
            onClick={() => {
              setEditing(record);
              form.setFieldsValue({
                name: record.name,
                display_name: record.display_name,
                endpoint: record.endpoint,
                service_token: '',
              });
            }}
          >
            编辑
          </Button>
          <Popconfirm
            title={`确认删除地区 ${record.display_name || record.name}？`}
            disabled={record.name === current}
            onConfirm={async () => {
              try {
                await deleteRegion(record.name);
                message.success('已删除');
                load();
              } catch (e) {
                message.error((e as Error).message);
              }
            }}
          >
            <Button size="small" danger disabled={record.name === current}>
              删除
            </Button>
          </Popconfirm>
        </>
      ),
    },
  ];

  return (
    <div className="p-[16px]">
      <Row gutter={16} className="mb-[16px]">
        <Col span={6}>
          <Card>
            <Statistic title="地区总数" value={data?.total ?? '-'} />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="在线地区"
              value={data?.online ?? '-'}
              valueStyle={{
                color:
                  data && data.online < data.total ? '#cf1322' : '#3f8600',
              }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic title="GPU 总量" value={data?.gpu_count ?? '-'} />
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic title="模型副本" value={data?.model_replicas ?? '-'} />
          </Card>
        </Col>
      </Row>
      <Card
        title="全部地区"
        extra={
          <>
            <Button
              type="primary"
              icon={<PlusOutlined />}
              size="small"
              className="mr-[8px]"
              onClick={() => {
                setEditing('new');
                form.resetFields();
              }}
            >
              新增地区
            </Button>
            <Button icon={<ReloadOutlined />} size="small" onClick={load} loading={loading}>
              刷新
            </Button>
          </>
        }
      >
        <Table
          rowKey="name"
          columns={columns}
          dataSource={data?.regions || []}
          loading={loading && !data}
          pagination={false}
        />
      </Card>
      <Modal
        title={editing === 'new' ? '新增地区' : '编辑地区'}
        open={editing !== null}
        confirmLoading={saving}
        onCancel={() => setEditing(null)}
        onOk={async () => {
          const values = await form.validateFields();
          setSaving(true);
          try {
            if (editing === 'new') {
              await createRegion(values);
            } else if (editing) {
              const body: Record<string, string> = {
                display_name: values.display_name || '',
                endpoint: values.endpoint,
              };
              if (values.service_token) {
                body.service_token = values.service_token;
              }
              await updateRegion(editing.name, body);
            }
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
          <Form.Item
            name="name"
            label="地区标识（小写字母/数字/中划线）"
            rules={[
              { required: true, message: '请输入地区标识' },
              { pattern: /^[a-z0-9][a-z0-9-]{0,62}$/, message: '仅支持小写字母、数字、中划线' },
            ]}
          >
            <Input placeholder="beijing" disabled={editing !== 'new'} />
          </Form.Item>
          <Form.Item name="display_name" label="显示名称">
            <Input placeholder="华北-北京" />
          </Form.Item>
          <Form.Item
            name="endpoint"
            label="站点接入点（http/https）"
            rules={[
              { required: true, message: '请输入站点接入点' },
              { pattern: /^https?:\/\//, message: '需以 http:// 或 https:// 开头' },
            ]}
          >
            <Input placeholder="https://bj.powerllm.internal:9997" />
          </Form.Item>
          <Form.Item
            name="service_token"
            label="Service Token（可选；留空则透传用户登录态）"
          >
            <Input.Password placeholder="不修改请留空" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
};

export default RegionsOverview;
