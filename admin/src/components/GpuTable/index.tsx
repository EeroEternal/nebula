import { Table, Progress, Tag, Tooltip } from 'antd';
import { QuestionCircleOutlined } from '@ant-design/icons';
import { FC, useMemo } from 'react';
import { isEmpty } from 'lodash';
import { l } from '@/utils/intl';
import { calculatePercentage } from '@/utils';
import { DeviceInfo } from '@/types/Public/data';

export function transformGpusToTableData(data: DeviceInfo['gpus']) {
  if (data === null || isEmpty(data) || !data) return [];
  return Object.keys(data).map((objKey) => {
    const gpu = data[objKey];
    return {
      name: gpu.name,
      percent: calculatePercentage(gpu.mem_used, gpu.mem_total),
      type: objKey,
    };
  });
}
const GpuTable: FC<{ data: DeviceInfo['gpus'] }> = ({ data }) => {
  const dataSource = useMemo(() => {
    if (data === null || isEmpty(data) || !data) return [];
    return Object.keys(data).map((objKey) => {
      const gpu = data[objKey];
      return {
        name: gpu.name,
        percent: calculatePercentage(gpu.mem_used, gpu.mem_total),
        type: objKey,
        status: gpu.status,
      };
    });
  }, [data]);
  return (
    <Table
      rowKey="type"
      pagination={false}
      size="small"
      dataSource={dataSource}
      columns={[
        {
          key: 'type',
          dataIndex: 'type',
          title: l('monitoring.deviceInfo.gpu.name'),
        },
        {
          key: 'name',
          dataIndex: 'name',
          title: l('monitoring.deviceInfo.gpu.type'),
        },
        {
          key: 'status',
          dataIndex: 'status',
          title: l('monitoring.deviceInfo.gpu.status'),
          render: (_, record) =>
            record.status === 'expired' ? (
              <Tooltip title={l('global.license.disabledTips')}>
                <Tag className="mr-0" color="warning">
                  {l('monitoring.deviceInfo.gpu.status.unauthorized')} <QuestionCircleOutlined />
                </Tag>
              </Tooltip>
            ) : (
              <Tag className="mr-0" color="success">
                {l('monitoring.deviceInfo.gpu.status.authorized')}
              </Tag>
            ),
        },
        {
          key: 'percent',
          dataIndex: 'percent',
          title: l('monitoring.deviceInfo.gpu.rate'),
          align: 'center',
          render: (val) => (
            <Progress
              steps={4}
              strokeColor={
                val > 90 ? 'var(--c-error)' : 'var(--c-primary)'
              }
              percent={val}
              success={{ percent: 99 }}
            />
          ),
        },
      ]}
    />
  );
};

export default GpuTable;
