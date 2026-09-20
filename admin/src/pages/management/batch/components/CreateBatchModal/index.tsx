import { ModalForm, ProFormUploadButton, ProFormText } from '@ant-design/pro-components';
import type { FormInstance } from '@ant-design/pro-components';
import { useState, useRef, FC, PropsWithChildren, useEffect } from 'react';
import { l } from '@/utils/intl';
import { useRequest } from 'ahooks';
import request from '@/utils/request';

interface CreateBatchModalProps {
  initValues?: {
    task_id?: string;
    id: string;
  };
  submitBack: () => void;
}
const CreateBatchModal: FC<PropsWithChildren & CreateBatchModalProps> = ({
  initValues,
  children,
  submitBack,
}) => {
  const [visible, setVisible] = useState(false);
  const formRef = useRef<FormInstance>();
  const { run, loading } = useRequest((data) => request('/batches', { data, method: 'post' }), {
    manual: true,
    onSuccess: (res) => {
      if (res.success) {
        setVisible(false);
        submitBack?.();
      }
    },
  });
  const handleSubmit = async (values: {
    file: { originFileObj: Blob }[];
    endpoint: string;
    completion_window: string;
  }) => {
    const formData = new FormData();
    formData.append('file', values.file[0].originFileObj);
    formData.append('endpoint', values.endpoint);
    formData.append('completion_window', values.completion_window);
    run(formData);
  };
  useEffect(() => {
    if (visible) formRef.current?.resetFields();
  }, [visible]);
  return (
    <>
      <span onClick={() => setVisible(true)}>{children}</span>
      <ModalForm
        width={600}
        open={visible}
        formRef={formRef}
        onFinish={handleSubmit}
        modalProps={{ onCancel: () => setVisible(false) }}
        title={initValues?.id ? l('tasks.batch.edit') : l('tasks.batch.create')}
        loading={loading}
        className="mt-4"
      >
        <ProFormUploadButton
          // extra="支持扩展名：.jpg .zip .doc .wps"
          label={l('tasks.batch.inputFile')}
          tooltip={l('tasks.batch.inputFile.tips')}
          name="file"
          max={1}
          rules={[{ required: true, message: l('tasks.batch.inputFile.rule') }]}
          colProps={{ span: 24 }}
          fieldProps={{
            customRequest: ({ onSuccess }) => onSuccess?.('ok'),
          }}
        />

        <ProFormText
          name="endpoint"
          label={l('tasks.batch.endpoint')}
          tooltip={l('tasks.batch.endpoint.tips')}
          rules={[{ required: true, message: l('tasks.batch.endpoint.rule') }]}
          placeholder={l('tasks.batch.endpoint.rule')}
          colProps={{ span: 24 }}
          disabled={!!initValues?.task_id}
        />
        <ProFormText
          name="completion_window"
          label={l('tasks.batch.completionWindow')}
          rules={[{ required: true, message: l('tasks.batch.completionWindow') }]}
          colProps={{ span: 24 }}
          initialValue="24h"
        />
      </ModalForm>
    </>
  );
};
export default CreateBatchModal;
