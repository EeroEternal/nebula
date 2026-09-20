import { PASSWORD_REGEX } from '@/constants/user';
import { l, lGet } from '@/utils/intl';
import { ModalForm, ProFormSelect, ProFormText } from '@ant-design/pro-components';
import { history } from '@umijs/max';
import { Form, message } from 'antd';
import { useEffect, useState } from 'react';
import { omit } from 'lodash';
import { DataEmpty } from '@/components';
import type { RoleListItem } from '@/types/Public/data';
import type { CreateUserParams } from '../../data';
import { createUsers, getRole } from '../../service';

const formLayout = {
  labelCol: {
    span: 5,
  },
  wrapperCol: {
    span: 19,
  },
};

type CreateProps = {
  visible: boolean;
  onCancel: () => void;
  onSubmit: () => void;
};

const CreateForm = (props: CreateProps) => {
  const { visible, onCancel, onSubmit } = props;
  const [rolesList, setRolesList] = useState<string[]>([]);
  const [form] = Form.useForm();
  useEffect(() => {
    getRole().then((res) => {
      setRolesList(((res.data?.data?.results || []) as RoleListItem[]).map((item) => item.role));
    });
  }, []);

  const validateDuplicatePassword = (_rule: unknown, value: string) => {
    if (value && value !== form.getFieldsValue().password) {
      return Promise.reject(new Error(lGet('admin.users.duplicatePassword.invalid') as string));
    }
    return Promise.resolve();
  };

  const handleSubmitValues = async (values: CreateUserParams) => {
    const res = await createUsers(omit(values, 'duplicatePassword'));
    if (res.success) {
      onSubmit();
      message.success(lGet('admin.users.createUserSuccess'));
      onCancel();
    }
  };
  return (
    <ModalForm
      title={l('admin.users.create')}
      visible={visible}
      onFinish={handleSubmitValues}
      modalProps={{
        destroyOnClose: true,
        onCancel: () => onCancel(),
      }}
      layout="horizontal"
      width={540}
      {...formLayout}
      submitter={{
        searchConfig: {
          submitText: l('global.actions.create'),
          resetText: l('global.actions.cancel'),
        },
      }}
      form={form}
    >
      <ProFormText
        label={l('admin.users.name')}
        name="username"
        placeholder={l('admin.users.name.rule')}
        fieldProps={{
          autoComplete: 'new-password',
        }}
        rules={[{ required: true, message: l('admin.users.name.rule') }]}
        id={Math.random().toString(36).substr(2)}
      />
      <ProFormText
        label={l('admin.users.email')}
        name="email"
        placeholder={l('admin.users.email.rule')}
        rules={[
          { required: true, message: l('admin.users.email.rule') },
          { type: 'email', message: l('admin.users.email.rule.invalid') },
        ]}
      />

      <ProFormText.Password
        label={l('admin.users.password')}
        name="password"
        placeholder={l('admin.users.password.rule')}
        fieldProps={{
          autoComplete: 'new-password',
        }}
        rules={[
          { required: true, message: l('admin.users.password.rule') },
          { pattern: PASSWORD_REGEX, message: l('admin.users.password.rule.invalid') },
        ]}
      />
      <ProFormText.Password
        label={l('admin.users.duplicatePassword')}
        name="duplicatePassword"
        placeholder={l('admin.users.duplicatePassword.rule')}
        fieldProps={{
          autoComplete: 'new-password',
        }}
        rules={[
          { required: true, message: l('admin.users.duplicatePassword.rule') },
          { validator: validateDuplicatePassword },
        ]}
      />
      <ProFormSelect
        label={l('admin.users.role')}
        name="role"
        placeholder={l('admin.users.role.rule')}
        options={rolesList}
        rules={[{ required: true, message: l('admin.users.role.rule') }]}
        fieldProps={{
          notFoundContent: (
            <DataEmpty
              emptyText={
                <div>
                  {l('global.data.empty')},
                  <span
                    onClick={() => history.push('/admin/roles')}
                    className="cursor-pointer text-primary"
                  >
                    {l('admin.users.role.create')}
                  </span>
                </div>
              }
            />
          ),
        }}
      />
    </ModalForm>
  );
};
export default CreateForm;
