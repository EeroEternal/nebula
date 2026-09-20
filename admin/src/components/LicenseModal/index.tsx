import { useRef } from 'react';
import { CloseCircleTwoTone } from '@ant-design/icons';
import { ModalForm, ProFormText, ProFormList } from '@ant-design/pro-components';
import type { ProFormInstance, FormListActionType } from '@ant-design/pro-components';
import { App, Button } from 'antd';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { size } from 'lodash';
import { Plus, Trash2, Undo2, RefreshCw } from 'lucide-react';
import cn from 'classnames';

import request from '@/utils/request';
import { l, lGet } from '@/utils/intl';
import { IconButton } from '@/components';
import type { ModelInitialState } from '@/types/global';
import { setLocal } from '@/utils';

const LicenseModal = () => {
  const { initialState, setInitialState } = useModel('@@initialState');
  const formRef = useRef<ProFormInstance>();
  const licenseFormListRef = useRef<FormListActionType>();
  const { modal, message } = App.useApp();
  const { data } = useRequest(() => request('/license'));
  const licenseDetail = data?.data?.data || {};

  const successCallback = async () => {
    // 更新菜单权限
    const res = await request('/user/me');
    const newUser = {
      ...(initialState?.currentUser || {}),
      ...(res?.data?.data || {}),
    };
    setLocal('user', newUser);
    setInitialState(
      (prev) =>
        ({
          ...prev,
          licenseModalVisible: false,
          currentUser: newUser,
        } as ModelInitialState),
    );
    window.location.reload();
  };
  const reset = () => {
    formRef.current?.resetFields();
  };
  const { run, loading } = useRequest((data) => request('/license', { method: 'post', data }), {
    manual: true,
    onSuccess: (res) => {
      if (res.success) {
        if (size(res?.data?.data?.invalid_licenses)) {
          modal.confirm({
            icon: <CloseCircleTwoTone twoToneColor="#ff4d4f" />,
            title: lGet('global.license.updateErrorTitle'),
            content: (
              <div>
                {res.data.data.invalid_licenses.map(
                  (item: { license_key: string; prompt: string }) => (
                    <div key={item.license_key}>
                      <div>{item.license_key}</div>
                      <div className="text-danger">{item.prompt}</div>
                    </div>
                  ),
                )}
              </div>
            ),
            cancelText: lGet('global.license.updateErrorCancel'),
            cancelButtonProps: {
              icon: <Undo2 size={14} />,
            },
            onCancel: reset,
            okText: lGet('global.license.updateErrorOk'),
            okButtonProps: {
              icon: <RefreshCw size={14} />,
            },
            onOk: successCallback,
          });
          return;
        }
        successCallback();
      }
    },
  });

  const submit = () => {
    formRef.current?.validateFields().then((values) => {
      const license_keys = values.license_keys
        .map((item: { value: string }) => item.value && item.value.trim())
        .filter(Boolean);
      if (!license_keys.length) {
        return message.error(lGet('global.license.licenseValidator'));
      }
      run({ license_keys });
    });
  };
  return (
    <ModalForm
      width={535}
      formRef={formRef}
      title={l('global.license.update')}
      open={true}
      modalProps={{ closable: false }}
      layout="horizontal"
      submitter={{
        render: () => {
          return (
            <div className="w-full flex justify-center gap-x-[10px]">
              <Button onClick={reset}>{l('global.actions.reset')}</Button>
              <Button type="primary" loading={loading} onClick={submit}>
                {l('global.actions.submit')}
              </Button>
            </div>
          );
        },
      }}
    >
      <div>
        {l('global.license.mac')}：{licenseDetail.mac_address}
      </div>
      <div>
        {l('global.license.expireTime')}：{licenseDetail?.licenses?.[0]?.expire_time || '-'}
      </div>
      <div className="text-danger">The license is invalid.</div>
      <div className="mt-[20px]" />
      <ProFormList
        label={l('global.license.updateLicense')}
        initialValue={[{}]}
        actionRef={licenseFormListRef}
        name="license_keys"
        copyIconProps={false}
        creatorButtonProps={false}
        actionRender={() => []}
        min={1}
        className="w-full-pro-form-list mt-[20px]"
        rules={[
          {
            required: true,
            validator: () => Promise.resolve(),
          },
        ]}
      >
        {(_, index) => (
          <div className="flex items-center w-full gap-x-[10px]">
            <ProFormText name="value" fieldProps={{ className: 'w-[340px]' }} />
            <div className={cn('!-mt-3.5')}>
              {index ? (
                <IconButton
                  onClick={() => licenseFormListRef.current?.remove?.(index)}
                  className="!w-6 !h-6 rounded-md group"
                >
                  <Trash2 size={16} className="group-hover:text-danger text-muted" />
                </IconButton>
              ) : (
                <Button
                  type="primary"
                  shape="circle"
                  size="small"
                  icon={<Plus size={14} />}
                  onClick={() => licenseFormListRef.current?.add?.()}
                />
              )}
            </div>
          </div>
        )}
      </ProFormList>
    </ModalForm>
  );
};
export default LicenseModal;
