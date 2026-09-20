import { Button, Form } from 'antd';
import type { FormInstance } from 'antd';
import {
  ProFormList,
  ProCard,
  ProFormText,
  ProFormRadio,
  ProFormGroup,
} from '@ant-design/pro-components';
import type { FormListActionType } from '@ant-design/pro-components';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import { FC, useRef } from 'react';

import { l, lGet } from '@/utils/intl';
import { MODEL_FORMAT_OPTIONS, MODEL_DEFAULT_VALUE, ModelType } from '@/constants/modelData';
interface ModelSpecsProps {
  form: FormInstance;
  modelType: ModelType;
}
export interface ModelSpecsFormListItem {
  model_uri: string;
  model_size_in_billions: string;
  model_format: string;
  quantization: string;
}
const ModelSpecs: FC<ModelSpecsProps> = ({ form, modelType }) => {
  const modelSpecsRef = useRef<FormListActionType>();
  const footerDivRef = useRef<HTMLDivElement>(null);
  const modelSpecsValue = Form.useWatch('model_specs', form);
  const handleDelete = (index: number) => {
    modelSpecsRef.current?.remove(index);
  };
  const handleAdd = () => {
    modelSpecsRef.current?.add(MODEL_DEFAULT_VALUE[ModelType.LLM]?.model_specs?.[0]);
    setTimeout(
      () =>
        footerDivRef.current?.scrollIntoView({
          behavior: 'smooth',
          block: 'end',
        }),
      10,
    );
  };
  const handleRadioChange = (value: string, index: number) => {
    if (value === 'pytorch') {
      form.setFieldValue(['model_specs', index, 'quantization'], 'none');
    } else if (value === 'ggufv2') {
      form.setFieldValue(['model_specs', index, 'quantization'], 'default');
    } else {
      form.setFieldValue(['model_specs', index, 'quantization'], '');
    }
  };
  return (
    <>
      <ProFormList
        name="model_specs"
        label={
          <div className="flex items-center">
            {l('model.register.specs')}
            <Button className="ml-[8px]" type="primary" onClick={handleAdd} icon={<PlusOutlined />}>
              {l('model.register.more')}
            </Button>
          </div>
        }
        actionRef={modelSpecsRef}
        copyIconProps={false}
        creatorButtonProps={false}
        initialValue={[{}]}
        itemRender={({ listDom }, { index, fields }) => (
          <ProCard bordered style={{ marginBlockEnd: 8 }} bodyStyle={{ paddingBlockEnd: 0 }}>
            <div className="flex items-center gap-x-[20px]">
              <div className="flex-1">{listDom}</div>
              {fields.length > 1 ? (
                <div
                  onClick={() => handleDelete(index)}
                  className="w-[30px] h-[30px] text-[18px] text-[#8c8c8c] bg-card rounded-full flex items-center justify-center hover:bg-primary hover:text-[#fff] cursor-pointer"
                >
                  <DeleteOutlined />
                </div>
              ) : (
                <div className="w-[30px]"></div>
              )}
            </div>
          </ProCard>
        )}
      >
        {(meta, index) => {
          const currentItemValue = modelSpecsValue?.[index] || {};
          return (
            <ProFormGroup>
              {currentItemValue.model_format === 'ggufv2' && (
                <ProFormText name="model_file_name_template" hidden initialValue="" />
              )}
              <ProFormRadio.Group
                label={l('model.register.modelFormat')}
                name="model_format"
                rules={[{ required: true }]}
                options={MODEL_FORMAT_OPTIONS[modelType] || []}
                fieldProps={{
                  onChange: (e) => handleRadioChange(e.target.value, index),
                }}
              />
              <ProFormText
                name="model_uri"
                label={l('model.register.modelPath')}
                rules={[{ required: true }]}
                extra={l('model.register.modelSpecs.modelUriExtra')}
                style={{ flex: 1 }}
                colProps={{ span: 12 }}
              />

              {modelType === ModelType.LLM && (
                <ProFormText
                  name="model_size_in_billions"
                  label={l('model.register.modelSize')}
                  rules={[
                    { required: true },
                    { pattern: /^\d+(\.\d+)?$/, message: l('global.tips.greaterZeroNumber') },
                  ]}
                  style={{ flex: 1 }}
                  colProps={{ span: 12 }}
                />
              )}

              <ProFormText
                name="quantization"
                label={l('model.register.modelSpecs.quantization')}
                hidden={currentItemValue.model_format === 'pytorch'}
                rules={[
                  {
                    required: ['gptq', 'awq', 'fp8', 'mlx'].includes(currentItemValue.model_format),
                    message: lGet('model.register.modelSpecs.quantizationTips'),
                  },
                ]}
                extra={l('model.register.modelSpecs.quantizationExtra')}
                colProps={{ span: 12 }}
              />
            </ProFormGroup>
          );
        }}
      </ProFormList>
      <div ref={footerDivRef}></div>
    </>
  );
};
export default ModelSpecs;
