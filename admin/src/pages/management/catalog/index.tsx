import { useRequest } from 'ahooks';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Progress,
  Select,
  Space,
  Table,
  Tag,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { history, useLocation } from '@umijs/max';
import { debounce } from 'lodash';
import { Package, Plus, RefreshCw, Rocket, Search, Thermometer } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { ActionWithTips, EmptyState, PageContainer, SectionLoading } from '@/components';
import {
  CatalogEntry,
  createCatalogEntry,
  deleteCatalogEntry,
  listCatalogEntries,
  warmupCatalogEntry,
} from '@/services/catalog';
import { copyToClipboard } from '@/utils';
import { readResponseDetail } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';

const WARMUP_FILTER_ALL = 'all';

const warmupTagColor = (status?: string) => {
  switch (status) {
    case 'ready':
      return 'success';
    case 'running':
      return 'processing';
    case 'failed':
      return 'error';
    default:
      return 'default';
  }
};

const CatalogPage: React.FC = () => {
  const { message, modal } = App.useApp();
  const location = useLocation();
  const [createOpen, setCreateOpen] = useState(false);
  const [form] = Form.useForm();
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const query = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const [keyword, setKeyword] = useState(query.get('q') || '');
  const [warmupFilter, setWarmupFilter] = useState(
    query.get('warmup') || WARMUP_FILTER_ALL,
  );

  const syncUrl = useCallback(
    (nextKeyword: string, nextWarmup: string) => {
      const params = new URLSearchParams();
      if (nextKeyword.trim()) params.set('q', nextKeyword.trim());
      if (nextWarmup && nextWarmup !== WARMUP_FILTER_ALL) {
        params.set('warmup', nextWarmup);
      }
      const search = params.toString();
      history.replace({
        pathname: '/models/catalog',
        search: search || undefined,
      });
    },
    [],
  );

  const { data, loading, error, refresh } = useRequest(async () => {
    const res = await listCatalogEntries();
    if (!res?.success) {
      throw new Error(
        readResponseDetail(res) || String(lGet('models.catalog.loadFailed')),
      );
    }
    return (res.data || []) as CatalogEntry[];
  }, { cacheKey: 'catalog-entries', staleTime: 30_000 });

  const entries = data || [];

  const filtered = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    return entries.filter((e) => {
      if (warmupFilter !== WARMUP_FILTER_ALL) {
        const st = e.warmup_status || 'idle';
        if (st !== warmupFilter) return false;
      }
      if (!q) return true;
      const hay = [
        e.model_name,
        e.version,
        e.source?.type,
        ...(e.hardware_tags || []),
        e.local_path || '',
      ]
        .join(' ')
        .toLowerCase();
      return hay.includes(q);
    });
  }, [entries, keyword, warmupFilter]);

  const running = useMemo(
    () => entries.some((e) => e.warmup_status === 'running'),
    [entries],
  );

  useEffect(() => {
    if (running) {
      if (!pollRef.current) {
        pollRef.current = setInterval(() => refresh(), 2000);
      }
    } else if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [running, refresh]);

  const debounceSyncUrl = useMemo(
    () =>
      debounce((kw: string, wf: string) => {
        syncUrl(kw, wf);
      }, 300),
    [syncUrl],
  );
  useEffect(() => () => debounceSyncUrl.cancel(), [debounceSyncUrl]);

  const apiErrorText = (res: unknown, fallbackKey: string) =>
    readResponseDetail(res) || lGet(fallbackKey);

  const handleWarmup = async (entry: CatalogEntry) => {
    const res = await warmupCatalogEntry(entry.id);
    if (!res?.success) {
      message.error(apiErrorText(res, 'models.catalog.warmupFailed'));
      return;
    }
    message.success(lGet('models.catalog.warmupStarted'));
    refresh();
  };

  const handleLaunch = async (entry: CatalogEntry) => {
    if (!entry.local_path) {
      message.warning(lGet('models.catalog.needWarmup'));
      return;
    }
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(entry.local_path);
      } else {
        copyToClipboard(entry.local_path);
      }
      message.success(lGet('models.catalog.pathCopied'));
    } catch {
      message.warning(entry.local_path);
    }
    history.push(
      `/models/repository/LLM?model_path=${encodeURIComponent(entry.local_path)}&model_name=${encodeURIComponent(entry.model_name)}`,
    );
  };

  const handleCreate = async () => {
    const values = await form.validateFields();
    const tags = (values.hardware_tags || '')
      .split(',')
      .map((s: string) => s.trim())
      .filter(Boolean);
    const res = await createCatalogEntry({
      model_name: values.model_name,
      version: values.version,
      hardware_tags: tags,
      source: {
        type: values.source_type || 's3',
        bucket: values.bucket || undefined,
        prefix: values.prefix || undefined,
        endpoint: values.endpoint || undefined,
      },
    });
    if (!res?.success) {
      message.error(apiErrorText(res, 'models.catalog.createFailed'));
      return;
    }
    message.success(lGet('models.catalog.createOk'));
    setCreateOpen(false);
    form.resetFields();
    refresh();
  };

  const handleDelete = (entry: CatalogEntry) => {
    modal.confirm({
      title: l('models.catalog.deleteConfirm'),
      content: l('models.catalog.deleteImpact'),
      okButtonProps: { danger: true },
      onOk: async () => {
        const res = await deleteCatalogEntry(entry.id);
        if (!res?.success) {
          message.error(apiErrorText(res, 'models.catalog.deleteFailed'));
          return;
        }
        message.success(lGet('models.catalog.deleteOk'));
        refresh();
      },
    });
  };

  const columns: ColumnsType<CatalogEntry> = [
    {
      title: l('models.catalog.modelName'),
      dataIndex: 'model_name',
      key: 'model_name',
      ellipsis: true,
    },
    {
      title: l('models.catalog.version'),
      dataIndex: 'version',
      key: 'version',
      width: 120,
    },
    {
      title: l('models.catalog.source'),
      key: 'source',
      width: 110,
      render: (_: unknown, row) => (
        <Tag className="!m-0 rounded-full">{row.source?.type || '-'}</Tag>
      ),
    },
    {
      title: l('models.catalog.hardware'),
      key: 'hardware_tags',
      ellipsis: true,
      render: (_: unknown, row) =>
        (row.hardware_tags || []).length
          ? (row.hardware_tags || []).join(', ')
          : '-',
    },
    {
      title: l('models.catalog.warmup'),
      key: 'warmup',
      width: 240,
      render: (_: unknown, row) => {
        const status = row.warmup_status || 'idle';
        return (
          <div className="flex flex-col gap-1 min-w-0">
            <Tag color={warmupTagColor(status)} className="!m-0 w-fit">
              {lGet(`models.catalog.warmupStatus.${status}`, status)}
            </Tag>
            {status === 'running' ? (
              <Progress
                percent={row.warmup_progress || 0}
                size="small"
                status="active"
              />
            ) : null}
            {status === 'failed' && row.warmup_error ? (
              <span className="text-xs text-danger truncate" title={row.warmup_error}>
                {row.warmup_error}
              </span>
            ) : null}
            {status === 'ready' && row.local_path ? (
              <span className="text-xs text-muted truncate" title={row.local_path}>
                {row.local_path}
              </span>
            ) : null}
            {status === 'running' && row.warmup_message ? (
              <span className="text-xs text-muted truncate">{row.warmup_message}</span>
            ) : null}
          </div>
        );
      },
    },
    {
      title: l('models.catalog.actions'),
      key: 'actions',
      width: 200,
      fixed: 'right',
      render: (_: unknown, row) => (
        <Space size="small">
          <ActionWithTips title={l('models.catalog.warmupBtn')}>
            <Button
              size="small"
              type="text"
              icon={<Thermometer size={14} />}
              loading={row.warmup_status === 'running'}
              aria-label={l('models.catalog.warmupBtn')}
              onClick={() => handleWarmup(row)}
            />
          </ActionWithTips>
          <ActionWithTips title={l('models.catalog.launchBtn')}>
            <Button
              size="small"
              type="text"
              icon={<Rocket size={14} />}
              disabled={row.warmup_status !== 'ready'}
              aria-label={l('models.catalog.launchBtn')}
              onClick={() => handleLaunch(row)}
            />
          </ActionWithTips>
          <ActionWithTips title={l('models.catalog.deleteBtn')}>
            <Button
              size="small"
              type="text"
              danger
              aria-label={l('models.catalog.deleteBtn')}
              onClick={() => handleDelete(row)}
            >
              {l('global.actions.delete')}
            </Button>
          </ActionWithTips>
        </Space>
      ),
    },
  ];

  const toolbar = (
    <div className="flex flex-col gap-3 mb-4 md:flex-row md:items-center md:justify-between">
      <div className="flex flex-wrap items-center gap-2 min-w-0">
        <Input
          allowClear
          className="w-full max-w-xs"
          prefix={<Search size={14} className="text-muted" />}
          placeholder={l('models.catalog.searchPlaceholder')}
          value={keyword}
          onChange={(e) => {
            const v = e.target.value;
            setKeyword(v);
            debounceSyncUrl(v, warmupFilter);
          }}
        />
        <Select
          className="w-36"
          value={warmupFilter}
          onChange={(v) => {
            setWarmupFilter(v);
            syncUrl(keyword, v);
          }}
          options={[
            { value: WARMUP_FILTER_ALL, label: l('models.catalog.filterAll') },
            { value: 'idle', label: l('models.catalog.warmupStatus.idle') },
            { value: 'running', label: l('models.catalog.warmupStatus.running') },
            { value: 'ready', label: l('models.catalog.warmupStatus.ready') },
            { value: 'failed', label: l('models.catalog.warmupStatus.failed') },
          ]}
        />
      </div>
      <Space wrap>
        <Button icon={<RefreshCw size={14} />} onClick={() => refresh()}>
          {l('models.catalog.refresh')}
        </Button>
        <Button
          type="primary"
          icon={<Plus size={14} />}
          onClick={() => setCreateOpen(true)}
        >
          {l('models.catalog.register')}
        </Button>
      </Space>
    </div>
  );

  let body: React.ReactNode;
  if (loading && !data) {
    body = <SectionLoading />;
  } else if (error) {
    body = (
      <EmptyState
        title={l('models.catalog.loadFailed')}
        description={error.message}
        action={
          <Button type="primary" onClick={() => refresh()}>
            {l('models.catalog.refresh')}
          </Button>
        }
      />
    );
  } else if (!entries.length) {
    body = (
      <EmptyState
        customIcon={<Package size={40} className="text-muted" />}
        title={l('models.catalog.empty')}
        description={l('models.catalog.emptyHint')}
        action={
          <Button type="primary" icon={<Plus size={14} />} onClick={() => setCreateOpen(true)}>
            {l('models.catalog.register')}
          </Button>
        }
      />
    );
  } else if (!filtered.length) {
    body = (
      <EmptyState
        title={l('models.catalog.noMatch')}
        description={l('models.catalog.noMatchHint')}
        action={
          <Button
            onClick={() => {
              setKeyword('');
              setWarmupFilter(WARMUP_FILTER_ALL);
              syncUrl('', WARMUP_FILTER_ALL);
            }}
          >
            {l('models.catalog.clearFilters')}
          </Button>
        }
      />
    );
  } else {
    body = (
      <Table
        rowKey="id"
        columns={columns}
        dataSource={filtered}
        pagination={{ pageSize: 20, showSizeChanger: true }}
        scroll={{ x: 960 }}
      />
    );
  }

  return (
    <PageContainer
      title={l('models.catalog.title')}
      subTitle={l('models.catalog.subTitle')}
      extraContent={
        running ? (
          <span className="text-sm text-muted">{l('models.catalog.pollingHint')}</span>
        ) : null
      }
    >
      {toolbar}
      {body}

      <Modal
        title={l('models.catalog.register')}
        open={createOpen}
        onCancel={() => setCreateOpen(false)}
        onOk={handleCreate}
        destroyOnClose
      >
        <Form form={form} layout="vertical" initialValues={{ source_type: 's3' }}>
          <Form.Item
            name="model_name"
            label={l('models.catalog.modelName')}
            rules={[{ required: true }]}
          >
            <Input />
          </Form.Item>
          <Form.Item
            name="version"
            label={l('models.catalog.version')}
            rules={[{ required: true }]}
          >
            <Input placeholder="v1" />
          </Form.Item>
          <Form.Item name="hardware_tags" label={l('models.catalog.hardware')}>
            <Input placeholder="H200,A100" />
          </Form.Item>
          <Form.Item
            name="source_type"
            label={l('models.catalog.source')}
            rules={[{ required: true }]}
          >
            <Select
              options={[
                { value: 's3', label: 's3' },
                { value: 'local', label: 'local' },
                { value: 'harbor', label: 'harbor' },
                { value: 'modelscope', label: 'modelscope' },
              ]}
            />
          </Form.Item>
          <Form.Item name="bucket" label="bucket" extra={l('models.catalog.bucketHint')}>
            <Input />
          </Form.Item>
          <Form.Item name="prefix" label="prefix">
            <Input placeholder="catalog/models/name/version/" />
          </Form.Item>
          <Form.Item name="endpoint" label="endpoint">
            <Input placeholder="http://minio:9000" />
          </Form.Item>
        </Form>
      </Modal>
    </PageContainer>
  );
};

export default CatalogPage;
