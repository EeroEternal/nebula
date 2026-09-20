import { useRef, useState } from 'react';
import dayjs from 'dayjs';
import { DatePicker } from 'antd';
import type { ActionType } from '@ant-design/pro-components';
import { ProTable } from '@ant-design/pro-components';
import { useIntl } from '@umijs/max';

import { DATE_FORMAT, LOG_MODULE_TYPE, LOG_OP_TYPE } from '@/constants';
import { formatDisplayTime } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { sliceClientPage } from '@/utils/tablePagination';

const { RangePicker } = DatePicker;
type DateRangeValue = [dayjs.Dayjs, dayjs.Dayjs];

const moduleValueEnum = Object.fromEntries(
  Object.keys(LOG_MODULE_TYPE).map((k) => [
    k,
    { text: LOG_MODULE_TYPE[k as keyof typeof LOG_MODULE_TYPE] },
  ]),
);

const Log = () => {
  const { locale } = useIntl();
  const isChinese = locale === 'zh-CN';
  const actionRef = useRef<ActionType>();
  const [timeStamp, setTimeStamp] = useState<DateRangeValue>([
    dayjs().subtract(1, 'month'),
    dayjs(),
  ]);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex justify-end">
        <RangePicker
          size="small"
          value={timeStamp}
          format={DATE_FORMAT}
          allowClear={false}
          onChange={(data) => {
            if (!data || data.length !== 2) return;
            setTimeStamp(data as DateRangeValue);
            actionRef.current?.reload();
          }}
        />
      </div>
      <ProTable
        actionRef={actionRef}
        options={false}
        size="small"
        className="ops-log-table [&_.ant-pro-table-search]:!mb-0 [&_.ant-pro-query-filter]:!px-3 [&_.ant-pro-query-filter]:!pt-3 [&_.ant-pro-query-filter]:!pb-0 [&_.ant-form-item]:!mb-2 [&_.ant-form-item-label>label]:!text-xs [&_.ant-input]:!text-xs [&_.ant-select]:!text-xs [&_.ant-btn]:!text-xs [&_.ant-pro-query-filter-row]:!min-h-0"
        rowKey={(r) => `${r.opTime}-${r.resourceId}-${r.operator}-${r.opType}`}
        search={{
          labelWidth: 'auto',
          defaultCollapsed: false,
          collapseRender: false,
          span: 8,
          searchGutter: [16, 4],
        }}
        form={{
          size: 'small',
        }}
        request={async (params, _sort, _filter) => {
          const result = await request('/user/operations', {
            params: {
              curPageNum: -1,
              numPerPage: -1,
              resourceId: params.resourceId || undefined,
            },
          });
          let data: {
            module?: string;
            operator?: string;
            opTime?: string | number;
            resourceId?: string;
            opType?: string;
          }[] = result?.data?.results || [];
          if (params.module) {
            data = data.filter((r: { module?: string }) => r.module === params.module);
          }
          if (params.operator) {
            const op = String(params.operator).toLowerCase();
            data = data.filter((r: { operator?: string }) =>
              String(r.operator || '')
                .toLowerCase()
                .includes(op),
            );
          }
          const startSec = dayjs(timeStamp[0]).startOf('day').unix();
          const endSec = dayjs(timeStamp[1]).endOf('day').unix();
          data = data.filter((r: { opTime?: string | number }) => {
            const t = Number(r.opTime) || 0;
            return t >= startSec && t <= endSec;
          });
          const page = sliceClientPage(data, params.current, params.pageSize || 20);
          return {
            data: page.data,
            total: page.total,
          };
        }}
        columns={[
          {
            title: l('monitor.logs.operation.resourceId'),
            dataIndex: 'resourceId',
            render: (val) => val || '-',
          },
          {
            title: l('monitor.logs.operation.fnModule'),
            key: 'module',
            dataIndex: 'module',
            valueType: 'select',
            valueEnum: moduleValueEnum,
            renderText: (val) =>
              isChinese ? LOG_MODULE_TYPE[val as keyof typeof LOG_MODULE_TYPE] : val,
          },
          {
            title: l('monitor.logs.operation.opType'),
            key: 'opType',
            dataIndex: 'opType',
            hideInSearch: true,
            renderText: (val) => (isChinese ? LOG_OP_TYPE[val as keyof typeof LOG_OP_TYPE] : val),
          },
          {
            title: l('monitor.logs.operation.operator'),
            key: 'operator',
            dataIndex: 'operator',
          },
          {
            title: l('monitor.logs.operation.opTime'),
            key: 'opTime',
            dataIndex: 'opTime',
            hideInSearch: true,
            render: (_, r) =>
              formatDisplayTime(r.opTime ? r.opTime * 1000 : undefined),
          },
          {
            title: l('monitor.logs.operation.ipAddress'),
            key: 'ipAddress',
            dataIndex: 'ipAddress',
            hideInSearch: true,
          },
        ]}
      />
    </div>
  );
};
export default Log;
