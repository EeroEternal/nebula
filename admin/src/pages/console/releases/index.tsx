import React, { useCallback, useEffect, useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Table,
  Tag,
} from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { formatDisplayTime } from '@/utils';
import {
  applyConsoleRelease,
  createConsoleRelease,
  fetchConsoleReleases,
} from '@/utils/consoleApi';
import { fetchRegionStatus, type RegionInfo } from '@/utils/region';
import { useConsoleReachable } from '@/hooks/useConsoleReachable';
import ConsoleUnavailable from '../ConsoleUnavailable';

const KIND_OPTIONS = [
  { value: 'site_settings', label: 'site_settings' },
  { value: 'raw', label: 'raw' },
  { value: 'model_lifecycle', label: 'model_lifecycle' },
];

const SAMPLE_PAYLOAD: Record<string, string> = {
  site_settings: JSON.stringify({ global: { show_setting_guide: false } }, null, 2),
  raw: JSON.stringify(
    [{ method: 'PUT', path: 'v1/setting/global', body: { show_setting_guide: true } }],
    null,
    2,
  ),
  model_lifecycle: JSON.stringify(
    {
      steps: [
        {
          action: 'register',
          model_type: 'LLM',
          body: { model: { model_name: 'demo' }, persist: true },
        },
      ],
    },
    null,
    2,
  ),
};

const ConsoleReleasesPage: React.FC = () => {
  const { reachable: consoleReachable, probed: consoleProbed } = useConsoleReachable();
  type ReleaseRow = { id: number; [key: string]: unknown };
  const [releases, setReleases] = useState<ReleaseRow[]>([]);
  const [regions, setRegions] = useState<RegionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [creating, setCreating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm();
  const { message, modal } = App.useApp();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [list, status] = await Promise.all([
        fetchConsoleReleases(),
        fetchRegionStatus(),
      ]);
      setReleases(list);
      setRegions(status || []);
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

  const showResults = (results: Record<string, unknown>[]) => {
    modal.info({
      title: '下发结果',
      width: 720,
      content: (
        <Table
          size="small"
          pagination={false}
          rowKey={(_, i) => String(i)}
          dataSource={results || []}
          columns={[
            { title: 'Region', dataIndex: 'region' },
            {
              title: '结果',
              dataIndex: 'success',
              render: (ok: boolean) =>
                ok ? <Tag color="success">成功</Tag> : <Tag color="error">失败</Tag>,
            },
            { title: 'HTTP', dataIndex: 'status', width: 80 },
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
            setCreating(true);
            form.setFieldsValue({
              name: '',
              kind: 'model_lifecycle',
              payload: SAMPLE_PAYLOAD.model_lifecycle,
              region: undefined,
            });
          }}
        >
          新建 Release
        </Button>
        <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
          刷新
        </Button>
      </Space>
      <Table
        rowKey="id"
        loading={loading}
        dataSource={releases}
        columns={[
          { title: 'ID', dataIndex: 'id', width: 70 },
          { title: '名称', dataIndex: 'name' },
          { title: '类型', dataIndex: 'kind' },
          {
            title: '创建时间',
            dataIndex: 'created_at',
            render: (ts: number) => formatDisplayTime(ts * 1000),
          },
          {
            title: '最近下发',
            dataIndex: 'applied_at',
            render: (ts?: number | null) =>
              formatDisplayTime(ts ? ts * 1000 : undefined),
          },
          {
            title: '操作',
            key: 'action',
            render: (_: unknown, record: ReleaseRow) => (
              <Space>
                <Button
                  size="small"
                  type="primary"
                  onClick={async () => {
                    try {
                      const data = await applyConsoleRelease(record.id);
                      showResults(data.results);
                      load();
                    } catch (e) {
                      message.error((e as Error).message);
                    }
                  }}
                >
                  下发全部
                </Button>
                <Select
                  size="small"
                  className="min-w-[120px]"
                  placeholder="灰度 region"
                  options={regions.map((r) => ({
                    value: r.name,
                    label: r.display_name || r.name,
                  }))}
                  onSelect={async (region: string) => {
                    try {
                      const data = await applyConsoleRelease(record.id, region);
                      showResults(data.results);
                      load();
                    } catch (e) {
                      message.error((e as Error).message);
                    }
                  }}
                />
              </Space>
            ),
          },
        ]}
      />
      <Modal
        title="新建 Release"
        open={creating}
        confirmLoading={saving}
        onCancel={() => setCreating(false)}
        onOk={async () => {
          const values = await form.validateFields();
          let payload: unknown;
          try {
            payload = JSON.parse(values.payload);
          } catch {
            message.error('payload 必须是合法 JSON');
            return;
          }
          setSaving(true);
          try {
            await createConsoleRelease({
              name: values.name,
              kind: values.kind,
              payload,
            });
            message.success('已创建');
            setCreating(false);
            load();
          } catch (e) {
            message.error((e as Error).message);
          } finally {
            setSaving(false);
          }
        }}
      >
        <Form
          form={form}
          layout="vertical"
          onValuesChange={(changed) => {
            if (changed.kind && SAMPLE_PAYLOAD[changed.kind]) {
              form.setFieldsValue({ payload: SAMPLE_PAYLOAD[changed.kind] });
            }
          }}
        >
          <Form.Item name="name" label="名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="kind" label="类型" rules={[{ required: true }]}>
            <Select options={KIND_OPTIONS} />
          </Form.Item>
          <Form.Item name="payload" label="Payload JSON" rules={[{ required: true }]}>
            <Input.TextArea rows={12} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
};

export default ConsoleReleasesPage;
