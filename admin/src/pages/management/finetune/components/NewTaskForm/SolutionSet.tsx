import {
  ProCard,
  ProFormField,
  ProFormSwitch,
  ProFormGroup,
  ProFormText,
  ProFormSelect,
} from '@ant-design/pro-components';
import { FC, useMemo } from 'react';
import { Layers, Check, Sparkles } from 'lucide-react';
import { Form } from 'antd';
import { l } from '@/utils/intl';
import { BADAM_MODE, BADAM_SWITCH_MODE } from '@/constants/finetune';
import { InputNumberWithSlider } from '@/components';
import type { StepFormProps } from './index';

interface FinetuneMethodProps {
  value?: string;
  onChange?: (v: string) => void;
}
const FinetuneMethod: FC<FinetuneMethodProps> = ({ value, onChange }) => {
  const finetuneMethodOptions = useMemo(
    () => [
      { label: 'LoRA', value: 'lora', desc: l('tasks.finetune.solutionSet.finetuneMethod.lora') },
      { label: 'Full', value: 'full', desc: l('tasks.finetune.solutionSet.finetuneMethod.full') },
      {
        label: 'Freeze',
        value: 'freeze',
        desc: l('tasks.finetune.solutionSet.finetuneMethod.freeze'),
      },
    ],
    [],
  );
  return (
    <div className="grid gap-3 md:grid-cols-3 mt-2">
      {finetuneMethodOptions.map((item) => (
        <div
          key={item.value}
          onClick={() => onChange?.(item.value)}
          className={`flex-1 relative cursor-pointer rounded-xl border-2 p-4 transition-all ${
            value === item.value
              ? 'border-primary bg-primary/5 shadow-sm'
              : 'border-border/50 hover:border-border'
          }`}
        >
          {value === item.value && (
            <div className="absolute top-2 right-2 h-5 w-5 rounded-full bg-primary flex items-center justify-center">
              <Check className="h-3 w-3 text-[#fff]" />
            </div>
          )}
          <div className="font-semibold text-sm">{item.label}</div>
          <div className="text-xs text-muted mt-1">{item.desc}</div>
        </div>
      ))}
    </div>
  );
};
const SolutionSet: FC<StepFormProps> = () => {
  const form = Form.useFormInstance();
  const finetuneMethod = Form.useWatch(['tuning_args', 'finetuning_type']);
  const useGalore = Form.useWatch(['galore_args', 'use_galore'], form);
  const useBadam = Form.useWatch(['badam_args', 'use_badam'], form);
  const handleGaloreSwithChange = (checked: boolean) => {
    // Galore配置与Badam配置互斥，只能存在其一
    if (checked && useBadam) {
      form.setFieldsValue({
        badam_args: {
          use_badam: false,
        },
      });
    }
  };
  const handleBAdamSwithChange = (checked: boolean) => {
    // Galore配置与Badam配置互斥，只能存在其一
    if (checked && useGalore) {
      form.setFieldsValue({
        galore_args: {
          use_galore: false,
        },
      });
    }
  };

  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <Layers size={18} className="text-primary" />
          {l('tasks.finetune.modelSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">4/6</span>}
    >
      <ProFormField
        name={['tuning_args', 'finetuning_type']}
        required
        label={l('tasks.finetune.solutionSet.finetuneMethod')}
        tooltip={l('tasks.finetune.solutionSet.finetuneMethod.tips')}
      >
        <FinetuneMethod />
      </ProFormField>
      <ProFormSwitch
        tooltip={l('tasks.finetune.solutionSet.useLlamaPro.tips')}
        name={['tuning_args', 'use_llama_pro']}
        label={l('tasks.finetune.solutionSet.useLlamaPro')}
        required
        checkedChildren={l('global.yes')}
        unCheckedChildren={l('global.no')}
      />
      {finetuneMethod === 'lora' ? (
        <>
          <div className="mt-8 mb-4 font-medium text-base flex items-center gap-2">
            <Sparkles size={18} className="text-primary" />
            {l('tasks.finetune.solutionSet.loraConfig')}
          </div>
          <ProFormGroup rowProps={{ gutter: [16, 0] }}>
            <ProFormField
              name={['lora_args', 'lora_rank']}
              rules={[{ required: true }]}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                label={l('tasks.finetune.solutionSet.loraConfig.loraRank')}
                tooltip={l('tasks.finetune.solutionSet.loraConfig.loraRank.tips')}
                min={1}
                max={1024}
              />
            </ProFormField>
            <ProFormField
              name={['lora_args', 'lora_alpha']}
              rules={[{ required: true }]}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                label={l('tasks.finetune.solutionSet.loraConfig.loraAlpha')}
                tooltip={l('tasks.finetune.solutionSet.loraConfig.loraAlpha.tips')}
                min={1}
                max={2048}
              />
            </ProFormField>
            <ProFormField
              name={['lora_args', 'lora_dropout']}
              rules={[{ required: true }]}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                label={l('tasks.finetune.solutionSet.loraConfig.loraDropout')}
                tooltip={l('tasks.finetune.solutionSet.loraConfig.loraDropout.tips')}
                min={0}
                max={1}
                step={0.01}
              />
            </ProFormField>
            <ProFormField
              name={['lora_args', 'loraplus_lr_ratio']}
              rules={[{ required: true }]}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                label={l('tasks.finetune.solutionSet.loraConfig.loraplusLrRatio')}
                tooltip={l('tasks.finetune.solutionSet.loraConfig.loraplusLrRatio.tips')}
                min={0}
                max={64}
                step={0.01}
              />
            </ProFormField>
            <ProFormText
              name={['lora_args', 'lora_target']}
              label={l('tasks.finetune.solutionSet.loraConfig.loraTarget')}
              placeholder={l('tasks.finetune.solutionSet.loraConfig.loraTarget.placeholder')}
              tooltip={l('tasks.finetune.solutionSet.loraConfig.loraTarget.tips')}
              colProps={{ span: 12 }}
              fieldProps={{ size: 'large' }}
            />
            <ProFormText
              name={['lora_args', 'additional_target']}
              label={l('tasks.finetune.solutionSet.loraConfig.additionalTarget')}
              placeholder={l('tasks.finetune.solutionSet.loraConfig.additionalTarget.placeholder')}
              tooltip={l('tasks.finetune.solutionSet.loraConfig.additionalTarget.tips')}
              colProps={{ span: 12 }}
              fieldProps={{ size: 'large' }}
            />
            <ProFormSwitch
              name={['lora_args', 'create_new_adapter']}
              label={l('tasks.finetune.solutionSet.loraConfig.createNewAdapter')}
              required
              tooltip={l('tasks.finetune.solutionSet.loraConfig.createNewAdapter.tips')}
              colProps={{ span: 6 }}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
            />
            <ProFormSwitch
              name={['lora_args', 'use_rslora']}
              label={l('tasks.finetune.solutionSet.loraConfig.useRslora')}
              required
              tooltip={l('tasks.finetune.solutionSet.loraConfig.useRslora.tips')}
              colProps={{ span: 6 }}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
            />
            <ProFormSwitch
              name={['lora_args', 'use_dora']}
              label={l('tasks.finetune.solutionSet.loraConfig.useDora')}
              required
              tooltip={l('tasks.finetune.solutionSet.loraConfig.useDora.tips')}
              colProps={{ span: 6 }}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
            />
            {/* 后端暂时未更新-暂不展示 */}
            {/* <ProFormSwitch
          name={['lora_args', 'use_pissa']}
          label={l('model.tuning.usePiSSA')}
          required
          tooltip={l('model.tuning.usePiSSATooltip')}
          colProps={{ span: 6 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        /> */}
          </ProFormGroup>
        </>
      ) : (
        <>
          <div className="mt-8 mb-4 font-medium text-base flex items-center gap-2">
            <Sparkles size={18} className="text-primary" />
            {l('tasks.finetune.solutionSet.galoreConfig')}
            <ProFormSwitch
              noStyle
              colProps={{ span: 2 }}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
              name={['galore_args', 'use_galore']}
              fieldProps={{
                className: '-mt-0.5',
                onChange: handleGaloreSwithChange,
              }}
            />
          </div>
          {useGalore && (
            <ProFormGroup rowProps={{ gutter: [16, 0] }}>
              <ProFormField
                name={['galore_args', 'galore_rank']}
                rules={[{ required: true }]}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  label={l('tasks.finetune.solutionSet.galoreConfig.galoreRank')}
                  tooltip={l('tasks.finetune.solutionSet.galoreConfig.galoreRank.tips')}
                  min={1}
                  max={1024}
                />
              </ProFormField>
              <ProFormField
                name={['galore_args', 'galore_update_interval']}
                rules={[{ required: true }]}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  label={l('tasks.finetune.solutionSet.galoreConfig.galoreUpdateInterval')}
                  tooltip={l('tasks.finetune.solutionSet.galoreConfig.galoreUpdateInterval.tips')}
                  min={1}
                  max={2048}
                />
              </ProFormField>
              <ProFormField
                name={['galore_args', 'galore_scale']}
                rules={[{ required: true }]}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  label={l('tasks.finetune.solutionSet.galoreConfig.galoreScale')}
                  tooltip={l('tasks.finetune.solutionSet.galoreConfig.galoreScale.tips')}
                  min={0}
                  max={100}
                  step={0.1}
                />
              </ProFormField>
              <ProFormText
                name={['galore_args', 'galore_target']}
                label={l('tasks.finetune.solutionSet.galoreConfig.galoreTarget')}
                rules={[
                  {
                    required: true,
                  },
                ]}
                tooltip={l('tasks.finetune.solutionSet.galoreConfig.galoreTarget.tips')}
                colProps={{ span: 12 }}
                fieldProps={{ size: 'large' }}
              />
            </ProFormGroup>
          )}
          <div className="border-b my-4" />
          <div
            className={`font-medium text-base flex items-center gap-2 ${
              useBadam ? 'mb-4' : 'mb-0'
            }`}
          >
            <Sparkles size={18} className="text-primary" />
            {l('tasks.finetune.solutionSet.badamConfig')}
            <ProFormSwitch
              noStyle
              colProps={{ span: 2 }}
              checkedChildren={l('global.yes')}
              unCheckedChildren={l('global.no')}
              name={['badam_args', 'use_badam']}
              fieldProps={{
                className: '-mt-0.5',
                onChange: handleBAdamSwithChange,
              }}
            />
          </div>
          {useBadam && (
            <ProFormGroup rowProps={{ gutter: [16, 0] }}>
              <ProFormSelect
                name={['badam_args', 'badam_mode']}
                label={l('tasks.finetune.solutionSet.badamConfig.badamMode')}
                tooltip={l('tasks.finetune.solutionSet.badamConfig.badamMode.tips')}
                placeholder={l('tasks.finetune.solutionSet.badamConfig.badamMode')}
                valueEnum={BADAM_MODE}
                colProps={{ span: 12 }}
                fieldProps={{ size: 'large' }}
              />
              <ProFormSelect
                name={['badam_args', 'badam_switch_mode']}
                label={l('tasks.finetune.solutionSet.badamConfig.badamSwitchMode')}
                rules={[{ required: true }]}
                tooltip={l('tasks.finetune.solutionSet.badamConfig.badamSwitchMode.tips')}
                placeholder={l('tasks.finetune.solutionSet.badamConfig.badamSwitchMode')}
                valueEnum={BADAM_SWITCH_MODE}
                colProps={{ span: 12 }}
                fieldProps={{ size: 'large' }}
              />
              <ProFormField
                name={['badam_args', 'badam_switch_interval']}
                rules={[{ required: true }]}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  label={l('tasks.finetune.solutionSet.badamConfig.badamSwitchInterval')}
                  tooltip={l('tasks.finetune.solutionSet.badamConfig.badamSwitchInterval.tips')}
                  min={1}
                  max={1024}
                />
              </ProFormField>
              <ProFormField
                name={['badam_args', 'badam_update_ratio']}
                rules={[{ required: true }]}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  label={l('tasks.finetune.solutionSet.badamConfig.badamUpdateRatio')}
                  tooltip={l('tasks.finetune.solutionSet.badamConfig.badamUpdateRatio.tips')}
                  min={0}
                  max={1}
                  step={0.01}
                  precision={2}
                />
              </ProFormField>
            </ProFormGroup>
          )}
        </>
      )}
    </ProCard>
  );
};
export default SolutionSet;
