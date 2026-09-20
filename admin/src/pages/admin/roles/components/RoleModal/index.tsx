import { FC, PropsWithChildren, useState } from 'react';
import { Tree, Form, App } from 'antd';
import type { TreeProps } from 'antd';
import { ModalForm, ProFormText, ProFormField } from '@ant-design/pro-components';
import { useRequest } from 'ahooks';
import { size, isEmpty, forEach } from 'lodash';
import { Menu, Code2 } from 'lucide-react';

import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';

interface RoleModalProps {
  type?: 'add' | 'edit';
  initValues?: {
    role?: string;
    permissions?: {
      action: Record<string, string[]>;
      page: Record<string, string[]>;
    };
  };
  onSubmitBack: () => void;
}
const RoleModal: FC<PropsWithChildren<RoleModalProps>> = ({
  initValues,
  type = 'add',
  children,
  onSubmitBack,
}) => {
  const isEdit = type === 'edit';
  const [form] = Form.useForm();
  const { message } = App.useApp();
  const [open, setOpen] = useState<boolean>(false);
  const [pageParentKeys, setPageParentKeys] = useState<string[]>([]);
  const [pageExpandedKeys, setPageExpandedKeys] = useState<string[]>([]);
  const [pageTreeData, setPageTreeData] = useState<TreeProps['treeData']>([]);
  const [actionParentKeys, setActionParentKeys] = useState<string[]>([]);
  const [actionExpandedKeys, setActionExpandedKeys] = useState<string[]>([]);
  const [actionTreeData, setActionTreeData] = useState<TreeProps['treeData']>([]);
  /**
   *
   * @param obj { models: ['read'], ... }
   * @param currentPermissions { models: ['read', 'list'], ... }
   * @returns ['models-read', ...]
   */
  const transformObjectToTreeCheckKeys = (
    obj: Record<string, string[]>,
    currentPermissions: Record<string, string[]>,
  ) => {
    const checkKeys: string[] = [];
    forEach(obj, (value, key) => {
      /** 控制没有children的情况 */
      if (!size(currentPermissions[key])) {
        checkKeys.push(key);
      } else {
        checkKeys.push(...(value || []).map((item) => `${key}-${item}`));
      }
    });
    return checkKeys;
  };

  const showModal = () => setOpen(true);
  const closeModal = () => setOpen(false);

  const { loading: initLoading, run: init } = useRequest(() => request('/roles/permissions'), {
    ready: open,
    onSuccess: (res) => {
      if (res.success) {
        const { page = {}, action = {} } = res.data?.data || {};
        const pageParentKeys = Object.keys(page);
        const actionParentKeys = Object.keys(action);
        setPageParentKeys(pageParentKeys);
        setPageExpandedKeys(pageParentKeys);
        setPageTreeData(
          pageParentKeys.map((item) => ({
            key: item,
            title: lGet(`menu.${item}`),
            children: size(page[item])
              ? page[item].map((sub: string) => ({
                  key: `${item}-${sub}`,
                  title: lGet(`menu.${item}.${sub}`),
                }))
              : [],
            disabled: item === 'dashboard',
            checkable: true,
          })),
        );
        setActionParentKeys(actionParentKeys);
        setActionExpandedKeys(actionParentKeys);
        setActionTreeData(
          actionParentKeys.map((item) => ({
            key: item,
            // defaultMessage avoids raw i18n ids when a new action group is added
            title: lGet(`admin.roles.permissions.action.${item}`, item),
            children: size(action[item])
              ? action[item].map((sub: string) => ({
                  key: `${item}-${sub}`,
                  title: lGet(`admin.roles.permissions.action.${sub}`, sub),
                }))
              : [],
          })),
        );
        if (!isEmpty(initValues)) {
          form.setFieldsValue({
            role: initValues?.role,
            permissions: {
              page: isEmpty(initValues?.permissions?.page)
                ? []
                : transformObjectToTreeCheckKeys(initValues?.permissions?.page || {}, page),
              action: isEmpty(initValues?.permissions?.action)
                ? []
                : transformObjectToTreeCheckKeys(initValues?.permissions?.action || {}, action),
            },
          });
        }
      }
    },
  });
  const { run, loading } = useRequest(
    (data) => request('/roles', { method: isEdit ? 'put' : 'post', data }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          closeModal();
          message.success(
            isEdit ? lGet('global.message.editSuccess') : lGet('global.message.addSuccess'),
          );
          onSubmitBack();
        }
      },
    },
  );
  const onPageExpand: TreeProps['onExpand'] = (keys) => {
    setPageExpandedKeys(keys as string[]);
  };
  const onActionExpand: TreeProps['onExpand'] = (keys) => {
    setActionExpandedKeys(keys as string[]);
  };
  /**
   * @param checkKeys ['models', 'models-list', ...]
   * @param parentKeys ['models', ....]
   * @returns // { models: ['list'], instances: [], ... }
   */
  const transformPermissionsValues = (checkKeys: string[], parentKeys: string[]) => {
    return Object.fromEntries(
      parentKeys.map((parent) => [
        parent,
        checkKeys.filter((k) => k.startsWith(`${parent}-`)).map((k) => k.slice(parent.length + 1)),
      ]),
    );
  };
  const handleSubmit = async (values: {
    role?: string;
    permissions?: { page?: string[]; action?: string[] };
  }) => {
    const { page = [], action = [] } = values.permissions || {};
    const data = {
      role: values.role,
      permissions: {
        page: transformPermissionsValues(page, pageParentKeys),
        action: transformPermissionsValues(action, actionParentKeys),
      },
    };
    run(data);
  };
  return (
    <ModalForm
      onLoad={init}
      loading={initLoading || loading}
      trigger={<span onClick={showModal}>{children}</span>}
      open={open}
      form={form}
      title={isEdit ? l('admin.roles.edit') : l('admin.roles.create')}
      onFinish={handleSubmit}
      grid
      layout="horizontal"
      initialValues={{
        permissions: {
          page: ['dashboard'],
          action: [],
        },
      }}
      modalProps={{ onCancel: closeModal }}
    >
      <div className="mt-2 w-full pr-2 overflow-y-auto max-h-[calc(100vh-314px)]">
        <ProFormText
          label={l('admin.roles.name')}
          name="role"
          rules={[{ required: true, message: l('admin.roles.name.rule') }]}
          placeholder={l('admin.roles.name.rule')}
          disabled={isEdit}
          width={270}
        />

        <div className="border rounded-md overflow-hidden w-full mt-4">
          <div className="flex border-b">
            <div className="w-1/2 p-2 bg-background flex items-center justify-center gap-2">
              <Menu size={16} />
              {l('admin.roles.permissions.page')}
            </div>
            <div className="w-1/2 p-2 bg-background flex items-center justify-center gap-2 border-l">
              <Code2 size={16} />
              {l('admin.roles.permissions.action')}
            </div>
          </div>
          <div className="flex">
            <div className="w-1/2 p-2 ">
              <ProFormField
                name={['permissions', 'page']}
                valuePropName="checkedKeys"
                trigger="onCheck"
              >
                <Tree
                  checkable
                  treeData={pageTreeData}
                  expandedKeys={pageExpandedKeys}
                  onExpand={onPageExpand}
                ></Tree>
              </ProFormField>
            </div>
            <div className="w-1/2 p-2 border-l">
              <ProFormField
                name={['permissions', 'action']}
                valuePropName="checkedKeys"
                trigger="onCheck"
              >
                <Tree
                  checkable
                  treeData={actionTreeData}
                  expandedKeys={actionExpandedKeys}
                  onExpand={onActionExpand}
                ></Tree>
              </ProFormField>
            </div>
          </div>
        </div>
      </div>
    </ModalForm>
  );
};
export default RoleModal;
