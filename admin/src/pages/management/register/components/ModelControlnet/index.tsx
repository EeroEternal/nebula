import { Button, Form } from 'antd';
import {
  ProFormList,
  ProCard,
  ProFormText,
  ProFormRadio,
  ProFormGroup,
} from '@ant-design/pro-components';
import type { FormListActionType, FormInstance } from '@ant-design/pro-components';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import { FC, useRef } from 'react';
import classNames from 'classnames';
import { size } from 'lodash';

import { l } from '@/utils/intl';
import { MODEL_FORMAT_IMAGE_OPTIONS } from '@/constants/modelData';

interface ModelControlnetProps {
  form: FormInstance;
}
const ModelControlnet: FC<ModelControlnetProps> = ({ form }) => {
  const modelControlnetRef = useRef<FormListActionType>();
  const footerDivRef = useRef<HTMLDivElement>(null);
  const controlnetValue = Form.useWatch('controlnet', form);

  const handleDelete = (index: number) => {
    modelControlnetRef.current?.remove(index);
  };
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
        label={
          <div className="flex items-center">
            {l('model.register.controlnet')}
            <Button className="ml-[8px]" type="primary" onClick={handleAdd} icon={<PlusOutlined />}>
              {l('model.register.more')}
            </Button>
          </div>
        }
        className={classNames({ 'h-[50px]': size(controlnetValue) === 0 })}
        actionRef={modelControlnetRef}
        copyIconProps={false}
        creatorButtonProps={false}
        itemRender={({ listDom }, { index }) => (
          <ProCard bordered style={{ marginBlockEnd: 8 }} bodyStyle={{ paddingBlockEnd: 0 }}>
            <div className="flex items-center gap-x-[20px]">
              <div className="flex-1">{listDom}</div>
              <div
                onClick={() => handleDelete(index)}
                className="w-[30px] h-[30px] text-[18px] text-[#8c8c8c] bg-card rounded-full flex items-center justify-center hover:bg-primary hover:text-[#fff] cursor-pointer"
              >
                <DeleteOutlined />
              </div>
            </div>
          </ProCard>
        )}
      >
        <ProFormGroup>
          <ProFormText
            name="model_name"
            label={l('model.register.modelName')}
            rules={[{ required: true }]}
            colProps={{ span: 12 }}
          />
          <ProFormText
            name="model_uri"
            label={l('model.register.modelPath')}
            rules={[{ required: true }]}
            colProps={{ span: 12 }}
          />
        </ProFormGroup>
        <ProFormRadio.Group
          label={l('model.register.modelFamily')}
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
