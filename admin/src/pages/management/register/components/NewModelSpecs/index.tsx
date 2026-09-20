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
import { Plus, Trash2 } from 'lucide-react';
import { FC, useRef } from 'react';

import { l, lGet } from '@/utils/intl';
import { MODEL_FORMAT_OPTIONS, MODEL_DEFAULT_VALUE, ModelType } from '@/constants/modelData';
import { IconButton } from '@/components';
import { size } from 'lodash';

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
  const isLLM = modelType === ModelType.LLM;

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
          <div className="flex items-center gap-2">
            {l('models.register.specs')}
            <Button className="h-7" size="small" onClick={handleAdd} icon={<Plus size={16} />}>
              {l('global.actions.add')}
            </Button>
          </div>
        }
        initialValue={[{}]}
        actionRef={modelSpecsRef}
        actionRender={(field) => {
          if (size(modelSpecsRef.current?.getList()) === 1) return [];
          return [
            <IconButton
              key="delete"
              onClick={() => modelSpecsRef.current?.remove(field.name)}
              className="mt-[1px] !h-6 !w-6"
            >
              <Trash2 size={16} className="group-hover:text-danger text-muted" />
            </IconButton>,
          ];
        }}
        creatorButtonProps={false}
        itemRender={({ listDom, action }, meta) => {
          return (
            <ProCard
              bordered
              extra={action}
              title={
                <span className="text-sm">{`${l('models.register.specs.title')} #${
                  meta.index + 1
                }`}</span>
              }
              headStyle={{ height: 36, padding: 12, paddingBottom: 0 }}
              bodyStyle={{ padding: 12, paddingBottom: 0 }}
              style={{ marginBottom: 8 }}
              // proformlist 下，会有一个div 携带width: max-content 导致下方item 超出card的宽度 从而导致页面出现横向滚动条
              className="w-min min-w-full"
            >
              {listDom}
            </ProCard>
          );
        }}
      >
        {(_, index) => {
          const currentItemValue = modelSpecsValue?.[index] || {};
          return (
            <>
              {currentItemValue.model_format === 'ggufv2' && (
                <ProFormText name="model_file_name_template" hidden initialValue="" />
              )}
              <ProFormRadio.Group
                label={l('models.register.modelFormat')}
                name="model_format"
                rules={[{ required: true }]}
                options={MODEL_FORMAT_OPTIONS[modelType as keyof typeof MODEL_FORMAT_OPTIONS] || []}
                fieldProps={{
                  onChange: (e) => handleRadioChange(e.target.value, index),
                }}
              />

              <ProFormGroup>
                <ProFormText
                  name="model_uri"
                  label={l('models.register.modelPath')}
                  rules={[{ required: true }]}
                  colProps={{ span: isLLM ? 12 : 24 }}
                  tooltip={l('models.register.modelSpecs.modelUriExtra')}
                  extra={l('models.register.modelSpecs.modelUriExtra')}
                />
                {isLLM && (
                  <ProFormText
                    name="model_size_in_billions"
                    label={l('models.register.modelSize')}
                    rules={[
                      { required: true },
                      { pattern: /^\d+(\.\d+)?$/, message: l('global.tips.greaterZeroNumber') },
                    ]}
                    colProps={{ span: 12 }}
                  />
                )}
              </ProFormGroup>
              <ProFormText
                name="quantization"
                label={l('models.register.modelSpecs.quantization')}
                hidden={currentItemValue.model_format === 'pytorch'}
                tooltip={l('models.register.modelSpecs.quantizationExtra')}
                rules={[
                  {
                    required: ['gptq', 'awq', 'fp8', 'mlx'].includes(currentItemValue.model_format),
                    message: lGet('models.register.modelSpecs.quantizationTips'),
                  },
                ]}
                extra={l('models.register.modelSpecs.quantizationExtra')}
              />
            </>
          );
        }}
      </ProFormList>
      <div ref={footerDivRef}></div>
    </>
  );
};
export default ModelSpecs;
