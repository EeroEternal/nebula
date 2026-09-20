import { ModalForm, ProFormText, ProFormSelect } from '@ant-design/pro-components';
import { Sparkles } from 'lucide-react';
import { Button } from 'antd';
import { useRequest } from 'ahooks';
import { useMemo, FC } from 'react';

import { l } from '@/utils/intl';
import request from '@/utils/request';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import type { DeviceInfo } from '@/types/Public/data';
import { DeviceStatus } from '@/components';

interface AutoFillProps {
  submitCallBack: (values: Record<string, unknown>) => void;
}
const AutoFill: FC<AutoFillProps> = ({ submitCallBack }) => {
  const { data: familyResponse, run: getModelFamily } = useRequest(
    () => request<{ data: Record<string, string[]>; success: boolean }>('/models/families'),
    { manual: true },
  );
  const { data: deviceResult, run: getDevices } = useRequest(
    () =>
      request<{ data: { results: DeviceInfo[] } }>('/device/info', {
        params: ALL_LIST_PAGES_PARAMS,
      }),
    { manual: true },
  );
  const workerIpOptions = (deviceResult?.data?.results || []).map((item) => ({
    label: item.worker_address,
    value: item.worker_address,
    name: item.name,
    status: item.status,
  }));
  const modelFamilyOptions = useMemo(() => {
    if (!familyResponse?.success) return [];
    return [
      ...new Set(
        Object.values(familyResponse?.data || {}).reduce((acc, cur) => acc.concat(cur || []), []),
      ),
    ];
  }, [familyResponse]);
  const { runAsync: submit, loading } = useRequest(
    (data) => request('/models/llm/auto-register', { method: 'post', data }),
    { manual: true },
  );

  const onFinish = async (values: {
    model_path?: string;
    model_family?: string;
    worker_ip?: string;
  }) => {
    const res = await submit({
      ...values,
      model_path: values.model_path?.trim(),
      worker_ip: values.worker_ip || undefined,
    });
    if (!res.success) return false;
    submitCallBack(res.data || {});
    return true;
  };
  return (
    <ModalForm
      width={600}
      title={l('models.register.autoFill')}
      trigger={
        <Button type="primary" className="h-[36px]" icon={<Sparkles size={14} />}>
          {l('models.register.autoFill')}
        </Button>
      }
      onOpenChange={(open) => {
        if (open) {
          getModelFamily();
          getDevices();
        }
      }}
      onFinish={onFinish}
      loading={loading}
      className="mt-4"
    >
      <ProFormText
        label={l('models.register.modelPath')}
        name="model_path"
        rules={[{ required: true }]}
      />
      <ProFormSelect
        label={l('models.register.modelFamily')}
        name="model_family"
        rules={[{ required: true }]}
        showSearch
        options={modelFamilyOptions}
        fieldProps={{
          filterOption: (input, option) =>
            (option?.label ?? '').toLowerCase().includes(input.toLowerCase()),
        }}
      />
      <ProFormSelect
        name="worker_ip"
        label={l('models.register.workerIp')}
        extra={l('models.register.workerIpAutoFillTips')}
        options={workerIpOptions}
        allowClear
        fieldProps={{
          optionRender: (option) => (
            <div
              className="flex justify-between items-center gap-x-[8px] mr-2"
              key={option.value}
            >
              <span>{`${option.label} (${option.data.name})`}</span>
              {option.data.status && <DeviceStatus status={option.data.status} />}
            </div>
          ),
        }}
      />
    </ModalForm>
  );
};
export default AutoFill;
