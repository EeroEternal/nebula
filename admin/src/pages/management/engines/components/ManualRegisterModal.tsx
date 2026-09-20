import { App, Form, Input, Modal } from 'antd';
import { useEffect, useState } from 'react';

import {
  EnginePullProgress,
  manualRegisterEngineImage,
} from '@/services/engineImages';
import {
  humanizeEngineImageError,
  stringifyDetail,
} from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';

type Props = {
  open: boolean;
  onClose: () => void;
  onStarted: (progress: EnginePullProgress) => void;
};

const ManualRegisterModal: React.FC<Props> = ({
  open,
  onClose,
  onStarted,
}) => {
  const { message } = App.useApp();
  const [form] = Form.useForm<{
    image: string;
    username?: string;
    password?: string;
  }>();
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!open) {
      form.resetFields();
      setSubmitting(false);
    }
  }, [open, form]);

  const handleOk = async () => {
    try {
      const values = await form.validateFields();
      const image = (values.image || '').trim();
      if (!image) {
        message.warning(lGet('models.engines.manualRegisterImageRequired'));
        return;
      }
      setSubmitting(true);
      // 全局 notification 已 humanize；此处不再重复 toast
      const res = await manualRegisterEngineImage({
        image,
        username: (values.username || '').trim() || undefined,
        password: values.password || undefined,
      });
      if (!res?.success) {
        setSubmitting(false);
        return;
      }
      const progress = res.data as EnginePullProgress;
      message.success(
        String(
          lGet('models.engines.manualRegisterStarted', undefined, {
            engine: progress.engine || '—',
          }),
        ),
      );
      onStarted(progress);
      onClose();
    } catch (e) {
      if ((e as { errorFields?: unknown })?.errorFields) return;
      message.error(
        humanizeEngineImageError(
          stringifyDetail((e as { data?: { detail?: unknown } })?.data?.detail),
        ) || String(lGet('models.engines.manualRegisterFailed')),
      );
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={open}
      title={l('models.engines.manualRegister')}
      okText={l('models.engines.manualRegisterSubmit')}
      cancelText={l('models.engines.cancel')}
      confirmLoading={submitting}
      destroyOnClose
      closable
      maskClosable={!submitting}
      onCancel={onClose}
      onOk={() => void handleOk()}
      width={560}
    >
      <p className="text-[12px] text-muted m-0 mb-3 leading-relaxed">
        {l('models.engines.manualRegisterHint')}
      </p>
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item
          name="image"
          label={l('models.engines.manualRegisterImage')}
          rules={[
            {
              required: true,
              message: String(lGet('models.engines.manualRegisterImageRequired')),
            },
          ]}
        >
          <Input.TextArea
            rows={2}
            className="font-mono text-[12px]"
            placeholder="vllm/vllm-openai:latest"
            autoFocus
          />
        </Form.Item>
        <Form.Item
          name="username"
          label={l('models.engines.registryUsername')}
        >
          <Input
            placeholder={lGet('models.engines.registryUsernamePh')}
            autoComplete="username"
          />
        </Form.Item>
        <Form.Item
          name="password"
          label={l('models.engines.registryPassword')}
        >
          <Input.Password
            placeholder={lGet('models.engines.registryPasswordPh')}
            autoComplete="new-password"
          />
        </Form.Item>
      </Form>
    </Modal>
  );
};

export default ManualRegisterModal;
