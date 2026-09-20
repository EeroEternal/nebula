import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { get } from 'lodash';

import { PageContainer, PermissionDenied, SectionLoading, StatusTag } from '@/components';
import { canManageSystemSettings } from '@/utils/permissions';
import { l } from '@/utils/intl';
import request from '@/utils/request';

type FlagRow = {
  key: string;
  item: string;
  value: string;
  tone?: 'success' | 'error' | 'neutral';
};

const FlagsPage = () => {
  const { initialState } = useModel('@@initialState');
  const isAdmin = canManageSystemSettings(initialState?.currentUser);

  const { data, loading } = useRequest(
    async () => {
      const [globalRes, statusRes] = await Promise.all([
        request('/setting/global'),
        request('/setting/monitoring/status').catch(() => undefined),
      ]);
      return {
        enableLangfuse: Boolean(get(globalRes, ['data', 'data', 'enable_langfuse'])),
        langfuseUrl: String(get(globalRes, ['data', 'data', 'langfuse_url'], '') || ''),
        xtraceConnected: Boolean(get(statusRes, ['data', 'data', 'xtrace', 'connected'])),
      };
    },
    { ready: isAdmin },
  );

  if (!isAdmin) {
    return <PermissionDenied scene="page" />;
  }

  const enableLangfuse = Boolean(data?.enableLangfuse);
  const rows: FlagRow[] = [
    {
      key: 'enable',
      item: l('admin.flags.langfuse.enable'),
      value: enableLangfuse
        ? l('admin.flags.on')
        : l('admin.flags.off'),
      tone: enableLangfuse ? 'success' : 'error',
    },
    {
      key: 'env',
      item: 'POWERLLM_DISABLE_LANGFUSE',
      value: enableLangfuse ? '0' : '1',
      tone: 'neutral',
    },
    {
      key: 'host',
      item: l('admin.flags.langfuse.host'),
      value: data?.langfuseUrl || l('admin.flags.empty'),
      tone: 'neutral',
    },
    {
      key: 'connected',
      item: l('admin.flags.langfuse.connected'),
      value: data?.xtraceConnected
        ? l('admin.flags.connected')
        : l('admin.flags.disconnected'),
      tone: data?.xtraceConnected ? 'success' : 'error',
    },
  ];

  const columns: ColumnsType<FlagRow> = [
    {
      title: l('admin.flags.col.item'),
      dataIndex: 'item',
      width: 280,
      render: (text: string) => (
        <span className="text-sm text-default break-all">{text}</span>
      ),
    },
    {
      title: l('admin.flags.col.value'),
      dataIndex: 'value',
      render: (text: string, record) =>
        record.tone === 'neutral' ? (
          <span className="text-sm text-default break-all">{text}</span>
        ) : (
          <StatusTag tone={record.tone}>{text}</StatusTag>
        ),
    },
  ];

  return (
    <PageContainer
      title={l('admin.flags.title')}
      subTitle={l('admin.flags.subTitle',
      )}
    >
      {loading && !data ? (
        <SectionLoading />
      ) : (
        <div className="flex flex-col gap-3">
          <Table<FlagRow>
            rowKey="key"
            size="middle"
            pagination={false}
            columns={columns}
            dataSource={rows}
            title={() => (
              <span className="text-sm font-semibold text-default">
                {l('admin.flags.langfuse.section')}
              </span>
            )}
          />
          {!enableLangfuse && data?.xtraceConnected ? (
            <p className="text-xs text-muted mb-0">
              {l('admin.flags.langfuse.hint',
              )}
            </p>
          ) : null}
        </div>
      )}
    </PageContainer>
  );
};

export default FlagsPage;
