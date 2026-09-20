import { Button, AutoComplete } from 'antd';
import { ProFormList, ProFormText } from '@ant-design/pro-components';
import { FC, useRef } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import type { FormListActionType } from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { l } from '@/utils/intl';
import { IconButton } from '@/components';
import { MODEL_KWARGS_OPTIONS } from '@/constants/modelData';

const ExtraParams: FC = () => {
  const { selectModelAbility } = useModel('management.instanceDetail.model');
  const extraConfigRef = useRef<FormListActionType>();
  return (
    <ProFormList
      initialValue={[{}]}
      actionRef={extraConfigRef}
      name="extraConfig"
      copyIconProps={false}
      creatorButtonProps={false}
      actionRender={() => []}
      label={
        <div className="flex items-center gap-2">
          {l('models.instances.detail.extendParams')}
          <Button
            className="h-7"
            type="primary"
            onClick={() => extraConfigRef.current?.add?.({})}
            icon={<Plus size={14} />}
          >
            {l('global.actions.add')}
          </Button>
        </div>
      }
    >
      {(_, index) => {
        return (
          <div className="flex">
            <div className="flex-1 flex">
              <ProFormText
                name="key"
                placeholder="key"
                colProps={{ span: 12 }}
                fieldProps={{ size: 'large' }}
              >
                <AutoComplete
                  options={MODEL_KWARGS_OPTIONS?.[selectModelAbility] || []}
                  placeholder="key"
                  allowClear
                  size="large"
                />
              </ProFormText>
              <ProFormText
                name="value"
                placeholder="value"
                colProps={{ span: 12 }}
                fieldProps={{ size: 'large' }}
              />
            </div>
            <IconButton
              onClick={() => extraConfigRef.current?.remove(index)}
              className="hover:bg-background rounded-md group mt-1.5 !h-7 !w-7"
            >
              <Trash2 size={18} className="group-hover:text-danger text-muted" />
            </IconButton>
          </div>
        );
      }}
    </ProFormList>
  );
};
export default ExtraParams;
