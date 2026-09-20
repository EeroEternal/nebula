import { Button, Form } from 'antd';
import {
  ProFormList,
  ProCard,
  ProFormText,
  ProFormRadio,
  ProFormGroup,
} from '@ant-design/pro-components';
import { Plus, Trash2 } from 'lucide-react';
import type { FormListActionType, FormInstance } from '@ant-design/pro-components';
import { FC, useRef } from 'react';
import classNames from 'classnames';
import { size } from 'lodash';

import { IconButton } from '@/components';
import { l } from '@/utils/intl';
import { MODEL_FORMAT_IMAGE_OPTIONS } from '@/constants/modelData';

interface ModelControlnetProps {
  form: FormInstance;
}
const ModelControlnet: FC<ModelControlnetProps> = ({ form }) => {
  const modelControlnetRef = useRef<FormListActionType>();
  const footerDivRef = useRef<HTMLDivElement>(null);
  const controlnetValue = Form.useWatch('controlnet', form);

  const handleAdd = () => {
    modelControlnetRef.current?.add({
      model_name: 'custom-controlnet',
      model_uri: '/path/to/controlnet-model',
      model_family: 'controlnet',
    });
    setTimeout(
      () =>
        footerDivRef.current?.scrollIntoView({
          behavior: 'smooth',
          block: 'end',
        }),
      10,
    );
  };

  return (
    <>
      <ProFormList
        name="controlnet"
        actionRef={modelControlnetRef}
        copyIconProps={false}
        creatorButtonProps={false}
        className={classNames({ 'h-6': size(controlnetValue) === 0 })}
        label={
          <div className="flex items-center gap-2">
            {l('models.register.controlnet')}
            <Button className="h-7" size="small" onClick={handleAdd} icon={<Plus size={16} />}>
              {l('global.actions.add')}
            </Button>
          </div>
        }
        actionRender={(field) => {
          return [
            <IconButton
              key="delete"
              onClick={() => modelControlnetRef.current?.remove(field.name)}
              className="mt-[1px] !h-6 !w-6"
            >
              <Trash2 size={16} className="group-hover:text-danger text-muted" />
            </IconButton>,
          ];
        }}
        itemRender={({ listDom, action }, meta) => {
          return (
            <ProCard
              bordered
              extra={action}
              title={
                <span className="text-sm">{`${l('models.register.controlnet')} #${
                  meta.index + 1
                }`}</span>
              }
              headStyle={{ height: 36, padding: 12, paddingBottom: 0 }}
              bodyStyle={{ padding: 12, paddingBottom: 0 }}
              style={{ marginBottom: 8 }}
              className="w-min min-w-full"
            >
              {listDom}
            </ProCard>
          );
        }}
      >
        <ProFormGroup>
          <ProFormText
            name="model_name"
            label={l('models.register.modelName')}
            rules={[{ required: true }]}
            colProps={{ span: 12 }}
          />
          <ProFormText
            name="model_uri"
            label={l('models.register.modelPath')}
            rules={[{ required: true }]}
            colProps={{ span: 12 }}
          />
        </ProFormGroup>
        <ProFormRadio.Group
          label={l('models.register.modelFamily')}
          name="model_family"
          rules={[{ required: true }]}
          options={MODEL_FORMAT_IMAGE_OPTIONS}
        />
      </ProFormList>
      <div ref={footerDivRef}></div>
    </>
  );
};
export default ModelControlnet;
