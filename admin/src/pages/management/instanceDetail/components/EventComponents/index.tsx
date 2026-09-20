import { formPageParams, formatTime } from '@/utils/fomatData';
import { ProTable } from '@ant-design/pro-components';
import React from 'react';
import { useLocation } from '@umijs/max';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { ModelsEventItem } from '@/types/Public/data';
import { asServerPageRows } from '@/utils/tablePagination';

const TableList: React.FC = () => {
  const location = useLocation();
  const queryParams = new URLSearchParams(location.search);
  const modelUid = queryParams.get('id');
  const columns = [
    {
      title: l('models.instances.detail.event.type'),
      dataIndex: 'event_type',
    },
    {
      title: l('models.instances.detail.event.time'),
      dataIndex: 'event_ts',
      sorter: true,
      render: (txt: unknown) => (txt !== null ? formatTime(Number(txt)) : '-'),
    },
    {
      title: l('models.instances.detail.event.message'),
      dataIndex: 'event_content',
    },
  ];

  return (
    <ProTable
      toolBarRender={false}
      rowKey="id"
      search={false}
      columns={columns}
      cardBordered={false}
      request={async (params: { pageSize: number; current: number }) => {
        const pageParams = formPageParams(params);
        const res = await request(`/models/${modelUid}/events`, { params: pageParams });
        // API: { count, results }；兼容历史直接数组
        const body = res?.data ?? res;
        const raw: ModelsEventItem[] = Array.isArray(body?.results)
          ? body.results
          : Array.isArray(body)
            ? body
            : Array.isArray(res?.results)
              ? res.results
              : [];
        const data = raw.map((item, index) => ({
          ...item,
          id: index,
        }));
        const total =
          typeof body?.count === 'number'
            ? body.count
            : typeof res?.count === 'number'
              ? res.count
              : data.length;
        return {
          data: asServerPageRows(data, params.pageSize),
          success: true,
          total,
        };
      }}
    />
  );
};

export default TableList;
