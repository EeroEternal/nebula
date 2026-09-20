import { l } from '@/utils/intl';
import { Modal, Select, Form, message } from 'antd';
import { ProTable } from '@ant-design/pro-components';
import { FC, PropsWithChildren, useState, useMemo } from 'react';
import { useRequest } from 'ahooks';
import request from '@/utils/request';
import { SyncWorkerSource } from '@/types/Public/data';

const SyncWorkModal: FC<
  PropsWithChildren<{ currentIpAddress: string; submitCallback: () => void }>
> = ({ children, currentIpAddress, submitCallback }) => {
  const [open, setOpen] = useState(false);
  const [form] = Form.useForm();
  const selectValue = Form.useWatch('sync_worker_address', form);
  const { data } = useRequest(
    () => request<{ data: { data: SyncWorkerSource[] } }>('/workers', { params: { detail: true } }),
    {
      ready: open,
    },
  );
  const workSource = useMemo(() => {
    return (data?.data?.data || []).map((item) => ({
      label: item.worker_address,
      value: item.worker_address,
      models: item.models,
      disabled: item.worker_address === currentIpAddress,
    }));
  }, [data?.data?.data]);
  const modelsForSelectWorker = useMemo(() => {
    return workSource.find((item) => item.value === selectValue)?.models || [];
  }, [selectValue, workSource]);
  const { loading, run } = useRequest((data) => request('/worker/sync', { method: 'put', data }), {
    manual: true,
    onSuccess: (res) => {
      if (res.success) {
        message.success('Sync Success! ');
        setOpen(false);
        submitCallback();
      }
    },
  });

  const onCancel = () => {
    setOpen(false);
    form.resetFields();
  };
  const submit = () => {
    form.validateFields().then((values) => {
      run({
        ...values,
        worker_address: currentIpAddress,
      });
    });
  };
  return (
    <>
      <span onClick={() => setOpen(true)}>{children}</span>
      <Modal
        width={750}
        open={open}
        title={l('monitor.deviceInfo.syncWorker')}
        onCancel={onCancel}
        onOk={submit}
        okText={l('monitor.deviceInfo.sync')}
        okButtonProps={{ loading }}
        destroyOnHidden
      >
        <Form className="my-[20px]" form={form}>
          <Form.Item name="sync_worker_address" label="Worker" rules={[{ required: true }]}>
            <Select
              style={{ width: 300 }}
              options={workSource}
              placeholder="Please select the Worker you want to sync"
            />
          </Form.Item>
          {modelsForSelectWorker && (
            <ProTable
              size="small"
              className="mt-[20px]"
              search={false}
              toolBarRender={false}
              dataSource={modelsForSelectWorker}
              pagination={false}
              columns={[
                {
                  title: l('model.running.instanceName'),
                  dataIndex: 'model_uid',
                },
                {
                  title: l('model.running.modelName'),
                  dataIndex: 'model_name',
                },
                {
                  title: l('model.running.modelType'),
                  dataIndex: 'model_type',
                  width: 120,
                },
                {
                  title: l('model.running.version'),
                  dataIndex: 'model_version',
                },
              ]}
            />
          )}
        </Form>
      </Modal>
    </>
  );
};
export default SyncWorkModal;
