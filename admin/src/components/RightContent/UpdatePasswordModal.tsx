import { DigitsReg, LetterReg, PunctuationReg } from '@/constants/user';
import { updatePassword } from '@/services/user';
import { l, lGet } from '@/utils/intl';
import { ModalForm, ProFormText } from '@ant-design/pro-components';
import { Form, message } from 'antd';

export type UpdatePasswordProps = {
  visible?: boolean;
  handleCancel: () => void;
};

const UpdatePasswordModal: React.FC<UpdatePasswordProps> = ({ visible, handleCancel }) => {
  const [form] = Form.useForm<{
    oldPassword: string;
    newPassword: string;
    confirmPassword: string;
  }>();
  const validatePassword = (_rule: unknown, value: string) => {
    const lengthValid = value.length >= 8 && value.length <= 30;
    const containsLetters = LetterReg.test(value);
    const containsDigits = DigitsReg.test(value);
    const containsPunctuation = PunctuationReg.test(value);

    if (!lengthValid || !containsLetters || !containsDigits || !containsPunctuation) {
      return Promise.reject(new Error(lGet('management.user.passwordMessage') as string));
    }

    return Promise.resolve();
  };

  const validateDuplicatePassword = (_rule: unknown, value: string) => {
    if (!value) {
      return Promise.reject(new Error(lGet('global.login.duplicatePasswordTips') as string));
    }
    if (value !== form.getFieldsValue().newPassword) {
      return Promise.reject(new Error(lGet('global.login.passwordMismatch') as string));
    }
    return Promise.resolve();
  };
  const handleSubmit = async (values: { oldPassword: string; newPassword: string}) => {
    const params = {
      old_password: values.oldPassword,
      new_password: values.newPassword,
    };
    const res = await updatePassword(params);
    if (res.success) {
      message.success(lGet('global.login.changePasswordSuccess'));
      handleCancel();
    }
  };
  return (
    <ModalForm
      title={l('global.login.changePassword')}
      visible={visible}
      form={form}
      width={480}
      autoFocusFirstInput
      modalProps={{
        destroyOnClose: true,
        onCancel: handleCancel,
      }}
      onFinish={handleSubmit}
    >
      <ProFormText.Password
        label={l('global.login.oldPassword')}
        name="oldPassword"
        rules={[{ required: true, message: lGet('global.login.oldPasswordTips') }]}
      />
      <ProFormText.Password
        label={l('global.login.newPassword')}
        name="newPassword"
        rules={[{ required: true, message: lGet('global.login.passwordTips') }, { validator: validatePassword }]}
        validateFirst
      />
      <ProFormText.Password
        label={l('global.login.confirmPassword')}
        name="confirmPassword"
        rules={[{ required: true }, { validator: validateDuplicatePassword }]}
        validateFirst
      />
    </ModalForm>
  );
};

export default UpdatePasswordModal;
