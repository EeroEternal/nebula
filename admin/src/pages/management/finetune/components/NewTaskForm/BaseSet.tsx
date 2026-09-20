import { ProCard, ProFormGroup, ProFormText, ProFormSelect } from '@ant-design/pro-components';
import { FC, useMemo } from 'react';
import { useRequest } from 'ahooks';
import { Form, Tag } from 'antd';
import { Server, CircleQuestionMark } from 'lucide-react';

import { DeviceStatus, ActionWithTips } from '@/components';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import { l, lGet } from '@/utils/intl';
import { calculatePercentage } from '@/utils';
import type { DeviceInfo } from '@/types/Public/data';
import request from '@/utils/request';
import type { StepFormProps } from './index';

const BaseSet: FC<StepFormProps> = ({ isEdit }) => {
  const form = Form.useFormInstance();
  const workerIp = Form.useWatch(['worker_ip']);
  const { data: deviceResult } = useRequest(() =>
    request<{ data: { results: DeviceInfo[] } }>('/device/info', { params: ALL_LIST_PAGES_PARAMS }),
  );
  const devices = deviceResult?.data?.results || [];

  const workerIpOptions = useMemo(
    () =>
      devices.map((item) => ({
        value: item.worker_address,
        label: item.worker_address,
        name: item.name,
        status: item.status,
        disabled: item.status === 'expired',
      })),
    [devices],
  );
  const GPUIdsOptions = useMemo(() => {
    if (!workerIp) return [];
    const gpus = devices.find((item) => item.worker_address === workerIp)?.gpus;
    return Object.entries(gpus || {}).map(([, info], index) => ({
      label: index,
      value: index,
      disabled: info.status === 'expired',
      vramPercent: calculatePercentage(info.mem_used, info.mem_total),
    }));
  }, [workerIp, devices]);

  const handleWorkerIPChange = () => {
    form.setFieldValue(['train_args', 'gpu_ids'], []);
  };

  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <Server size={18} className="text-primary" />
          {l('tasks.finetune.baseSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">1/6</span>}
    >
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormText
          name="task_name"
          label={l('tasks.finetune.taskName')}
          rules={[
            { required: true, message: lGet('tasks.finetune.taskName.placeholder') },
            { max: 20, message: lGet('tasks.finetune.taskName.rule') },
          ]}
          placeholder={l('tasks.finetune.taskName.placeholder')}
          disabled={isEdit}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <div className="w-1/2" />
        <ProFormSelect
          name="worker_ip"
          label="Worker IP"
          placeholder={l('tasks.finetune.baseSet.workerIP.rule')}
          options={workerIpOptions}
          rules={[{ required: true, message: lGet('tasks.finetune.baseSet.workerIP.rule') }]}
          onChange={handleWorkerIPChange}
          colProps={{ span: 12 }}
          fieldProps={{
            size: 'large',
            optionRender: (option) => (
              <div className="flex justify-between items-center gap-x-[8px]" key={option.value}>
                <span>
                  {option.label} ({option.data.name})
                </span>
                {option.data.status && <DeviceStatus status={option.data.status} />}
              </div>
            ),
          }}
        />
        <ProFormSelect
          name={['train_args', 'gpu_ids']}
          options={GPUIdsOptions}
          label="GPU Index"
          colProps={{ span: 12 }}
          rules={[{ required: true, message: lGet('tasks.finetune.baseSet.gpuIndex.rule') }]}
          mode="multiple"
          fieldProps={{
            size: 'large',
            optionRender: (option) => (
              <div className="flex justify-between items-center gap-x-[8px]" key={option.value}>
                <span>{option.label}</span>
                {option.data.disabled ? (
                  <ActionWithTips title={l('global.license.disabledTips')}>
                    <Tag className="mr-0" color="warning">
                      {l('monitor.deviceInfo.gpu.status.unauthorized')}{' '}
                      <CircleQuestionMark size={14} />
                    </Tag>
                  </ActionWithTips>
                ) : (
                  <span className={option.data.vramPercent > 90 ? 'text-danger' : 'text-success'}>
                    {option.data.vramPercent}%
                  </span>
                )}
              </div>
            ),
          }}
        />
      </ProFormGroup>
    </ProCard>
  );
};
export default BaseSet;
