import { l } from '@/utils/intl';
import { ModalForm, ProFormText } from '@ant-design/pro-components';
import { Form } from 'antd';
import type { CreateParams } from '../../data';

type CreateProps = {
  visible: boolean;
  onCancel: () => void;
  onSubmit: (params: CreateParams) => Promise<boolean | void>;
};

const CreateForm = (props: CreateProps) => {
  const { visible, onCancel, onSubmit } = props;
  const [form] = Form.useForm();
  return (
    <ModalForm
      title={l('global.create') + l('management.key')}
      visible={visible}
      onFinish={onSubmit}
      modalProps={{
        destroyOnClose: true,
        onCancel: () => onCancel(),
      }}
      layout="vertical"
      width={480}
      form={form}
    >
      <ProFormText
        label={l('management.key.keyName')}
        name="name"
        placeholder={l('management.key.keyNameTips')}
        rules={[{ required: true, message: l('management.key.keyNameTips') }]}
      />
    </ModalForm>
  );
};

export default CreateForm;
