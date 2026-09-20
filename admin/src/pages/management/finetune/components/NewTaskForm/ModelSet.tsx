import {
  ProCard,
  ProFormGroup,
  ProFormText,
  ProFormSelect,
  ProFormSwitch,
} from '@ant-design/pro-components';
import { FC, useMemo } from 'react';
import { useRequest } from 'ahooks';
import { Form } from 'antd';
import { Brain, Sparkles } from 'lucide-react';

import { QUANTIZATION_BIT, ROPE_SCALING, BOOSTER } from '@/constants/finetune';
import { TABS } from '@/constants/repository';
import { l, lGet } from '@/utils/intl';
import type { ModelVersionTableListItem } from '@/types/Public/data';
import request from '@/utils/request';
import type { StepFormProps } from './index';

const ModelSet: FC<StepFormProps> = ({}) => {
  const form = Form.useFormInstance();
  const model_args = Form.useWatch(['model_args']);
  const modelTypeOptions = useMemo(() => {
    return TABS.filter((item) => item !== 'custom').map((item) => ({
      label: lGet(`global.model.type.${item}`),
      value: item,
    }));
  }, []);
  const { data: modelNameRes } = useRequest(
    () =>
      request<{ data: { results: { model_name: string }[] } }>(
        `/model_registrations/${model_args?.model_type}`,
        {
          params: { is_builtin: model_args?.is_builtin },
        },
      ),
    {
      ready: !!model_args?.model_type,
      refreshDeps: [model_args?.model_type, model_args?.is_builtin],
    },
  );
  const modelNames = (modelNameRes?.data?.results || []).map((item) => item.model_name);
  const { data: modelVersionRes } = useRequest(
    () =>
      request<{ data: { results: ModelVersionTableListItem[] } }>(
        `/models/${model_args?.model_type}/${model_args?.model_name}/versions`,
      ),
    {
      ready: !!(model_args?.model_type && model_args?.model_name),
      refreshDeps: [model_args?.model_name],
    },
  );
  const modelVersions = (modelVersionRes?.data?.results || []).map((item) => item.model_version);

  const handleModelType = () => {
    form.setFieldsValue({
      model_args: {
        model_name: undefined,
        model_version: undefined,
      },
    });
  };
  const handleModelName = () => {
    form.setFieldsValue({
      model_args: {
        model_version: undefined,
      },
    });
  };

  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <Brain size={18} className="text-primary" />
          {l('tasks.finetune.modelSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">2/6</span>}
    >
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormSelect
          name={['model_args', 'model_type']}
          label={
            <div className="flex items-center gap-2">
              <span className="shrink-0">{l('tasks.finetune.modelType')}</span>
              <ProFormSwitch
                noStyle
                name={['model_args', 'is_builtin']}
                checkedChildren={l('tasks.finetune.modelSet.builtin')}
                unCheckedChildren={l('tasks.finetune.modelSet.builtin')}
                fieldProps={{
                  className: 'mt-[-3px]',
                  size: 'small',
                  onChange: handleModelType,
                }}
              />
            </div>
          }
          placeholder={l('tasks.finetune.modelType.rule')}
          rules={[{ required: true, message: lGet('tasks.finetune.modelType.rule') }]}
          options={modelTypeOptions}
          colProps={{ span: 12 }}
          fieldProps={{
            size: 'large',
            onChange: handleModelType,
          }}
        />
        <ProFormSelect
          name={['model_args', 'model_name']}
          label={l('tasks.finetune.modelName')}
          placeholder={l('tasks.finetune.modelName.rule')}
          rules={[{ required: true, message: lGet('tasks.finetune.modelName.rule') }]}
          options={modelNames}
          colProps={{ span: 12 }}
          showSearch={true}
          fieldProps={{
            size: 'large',
            onChange: handleModelName,
          }}
        />
        <ProFormSelect
          name={['model_args', 'model_version']}
          label={l('tasks.finetune.modelVersion')}
          options={modelVersions}
          placeholder={l('tasks.finetune.modelVersion.rule')}
          rules={[{ required: true, message: lGet('tasks.finetune.modelVersion.rule') }]}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormText
          name={['data_args', 'template']}
          label={l('tasks.finetune.modelSet.template')}
          placeholder={l('tasks.finetune.modelSet.template.rule')}
          rules={[{ required: true, message: lGet('tasks.finetune.modelSet.template.rule') }]}
          tooltip={l('tasks.finetune.modelSet.template.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormText
          name={['model_args', 'model_path']}
          label={l('tasks.finetune.modelSet.modelPath')}
          placeholder={l('tasks.finetune.modelSet.modelPath.placeholder')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
      </ProFormGroup>
      <div className="mt-4 mb-4 font-medium text-base flex items-center gap-2">
        <Sparkles size={18} className="text-primary" />
        {l('tasks.finetune.modelSet.advancedOptions')}
      </div>
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormSelect
          placeholder={l('tasks.finetune.modelSet.advancedOptions.quantizationBit.rule')}
          name={['model_args', 'quantization_bit']}
          label={l('tasks.finetune.modelSet.advancedOptions.quantizationBit')}
          rules={[
            {
              required: true,
              message: l('tasks.finetune.modelSet.advancedOptions.quantizationBit.rule'),
            },
          ]}
          tooltip={l('tasks.finetune.modelSet.advancedOptions.quantizationBit.tips')}
          valueEnum={QUANTIZATION_BIT}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />

        <ProFormSelect
          placeholder={l('tasks.finetune.modelSet.advancedOptions.ropeScaling.rule')}
          name={['model_args', 'rope_scaling']}
          label={l('tasks.finetune.modelSet.advancedOptions.ropeScaling')}
          rules={[
            {
              required: true,
              message: l('tasks.finetune.modelSet.advancedOptions.ropeScaling.rule'),
            },
          ]}
          valueEnum={ROPE_SCALING}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormSelect
          placeholder={l('tasks.finetune.modelSet.advancedOptions.booster.rule')}
          name={['model_args', 'booster']}
          label={l('tasks.finetune.modelSet.advancedOptions.booster')}
          rules={[
            { required: true, message: l('tasks.finetune.modelSet.advancedOptions.booster.rule') },
          ]}
          valueEnum={BOOSTER}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
      </ProFormGroup>
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormSwitch
          name={['model_args', 'visual_inputs']}
          label={l('tasks.finetune.modelSet.advancedOptions.visualInputs')}
          colProps={{ span: 6 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
        <ProFormSwitch
          tooltip={l('tasks.finetune.modelSet.advancedOptions.resizeVocabe.tips')}
          name={['model_args', 'resize_vocab']}
          label={l('tasks.finetune.modelSet.advancedOptions.resizeVocabe')}
          colProps={{ span: 6 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
        <ProFormSwitch
          tooltip={l('tasks.finetune.modelSet.advancedOptions.upcastLayernorm.tips')}
          name={['model_args', 'upcast_layernorm']}
          label={l('tasks.finetune.modelSet.advancedOptions.upcastLayernorm')}
          colProps={{ span: 6 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
        <ProFormSwitch
          tooltip={l('tasks.finetune.modelSet.advancedOptions.shiftAttn.tips')}
          name={['model_args', 'shift_attn']}
          label="S² Attention"
          colProps={{ span: 6 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
      </ProFormGroup>
    </ProCard>
  );
};
export default ModelSet;
