import { Button } from 'antd';
import type { ValidatorRule } from 'rc-field-form/es/interface';
import { Plus, Trash2 } from 'lucide-react';
import { ProFormList, ProFormText, ProFormDigit } from '@ant-design/pro-components';
import type { FormListActionType } from '@ant-design/pro-components';
import { useRef } from 'react';
import { IconButton } from '@/components';
import { l } from '@/utils/intl';

const ModelStop = () => {
  const modelControlnetRef = useRef<FormListActionType>();
  const handleAdd = () => {
    modelControlnetRef.current?.add({
      stop_token_id: '',
      stop: '',
    });
  };

  return (
    <ProFormList
      name="stopController"
      className="w-full-pro-form-list"
      actionRef={modelControlnetRef}
      copyIconProps={false}
      creatorButtonProps={false}
      initialValue={[{ stop_token_id: '', stop: '' }]}
      min={1}
      actionRender={() => []}
      rules={
        [{ required: true }] as unknown as (ValidatorRule & {
          required?: boolean;
        })[]
      }
      label={
        <div className="flex items-center gap-2">
          {l('models.register.stop')}
          <Button className="h-7" size="small" onClick={handleAdd} icon={<Plus size={16} />}>
            {l('global.actions.add')}
          </Button>
        </div>
      }
    >
      {(meta, index, action, count) => {
        return (
          <div className="flex">
            <div className="flex-1 flex">
              <ProFormDigit
                name="stop_token_id"
                placeholder={l('models.register.stopTokenIDTips')}
                rules={[
                  { required: true },
                  { pattern: /^[0-9]\d*$/, message: l('models.register.stopTokenIDRuleTips') },
                ]}
                fieldProps={{ size: 'large' }}
                className="flex-1"
                colProps={{ span: 12 }}
              />
              <ProFormText
                name="stop"
                placeholder={l('models.register.stopTips')}
                rules={[{ required: true }]}
                fieldProps={{ size: 'large' }}
                className="flex-1"
                colProps={{ span: 12 }}
              />
            </div>

            {count > 1 && (
              <IconButton
                onClick={() => modelControlnetRef.current?.remove(index)}
                className="hover:bg-background rounded-md group mt-[1px]"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>
            )}
          </div>
        );
      }}
    </ProFormList>
  );
};
export default ModelStop;
