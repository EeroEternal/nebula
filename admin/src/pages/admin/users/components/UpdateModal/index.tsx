import { l, lGet } from '@/utils/intl';
import { ModalForm, ProFormSelect, ProFormText } from '@ant-design/pro-components';
import { message } from 'antd';
import { omit } from 'lodash';
import { useEffect, useState } from 'react';
import type { TableListItem, UpdateUserParams } from '../../data';
import { getRole, updateUsers } from '../../service';

const formLayout = {
  labelCol: {
    span: 5,
  },
  wrapperCol: {
    span: 19,
  },
};

type UpdateProps = {
  visible: boolean;
  onCancel: () => void;
  onSubmit: () => void;
  values: TableListItem | undefined;
};

const UpdateForm = (props: UpdateProps) => {
  const { visible, onCancel, onSubmit, values } = props;
  const [rolesList, setRolesList] = useState([]);

  useEffect(() => {
    getRole().then((data) => {
      setRolesList((data?.data?.data?.results || []).map((item: API.roleItem) => item.role));
    });
  }, []);

  const handleSubmitValues = async (values: UpdateUserParams) => {
    const res = await updateUsers(omit(values, 'duplicatePassword'));

    if (res.success) {
      onSubmit();
      message.success(lGet('admin.users.updateUserSuccess'));
      onCancel();
    }
  };
  return (
    <ModalForm
      title={l('admin.users.edit')}
      visible={visible}
      onFinish={handleSubmitValues}
      initialValues={values}
      modalProps={{
        destroyOnClose: true,
        onCancel: () => onCancel(),
      }}
      layout="horizontal"
      width={480}
      {...formLayout}
      submitter={{
        searchConfig: {
          submitText: l('global.actions.save'),
          resetText: l('global.actions.cancel'),
        },
      }}
    >
      <ProFormText label={l('admin.users.account')} name="account" disabled />
      <ProFormText label={l('admin.users.name')} name="username" disabled />
      <ProFormSelect
        label={l('admin.users.role')}
        name="role"
        placeholder={l('admin.users.role.rule')}
        options={rolesList}
        rules={[{ required: true, message: l('admin.users.role.rule') }]}
      />
      <ProFormSelect
        label={l('admin.users.accountStatus')}
        name="status"
        placeholder={l('admin.users.accountStatus.rule')}
        valueEnum={{
          enabled: l('admin.users.accountStatus.enabling'),
          disabled: l('admin.users.accountStatus.disabled'),
        }}
        rules={[{ required: true, message: l('admin.users.accountStatus.rule') }]}
      />
    </ModalForm>
  );
};
export default UpdateForm;
