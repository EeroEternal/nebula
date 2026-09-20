import {
  ProFormTextArea,
  ProFormDigit,
  ProFormField,
  ProFormText,
} from '@ant-design/pro-components';
import { FC } from 'react';
import { InputNumberWithSlider } from '@/components';
import { l } from '@/utils/intl';

const Generate: FC = () => {
  return (
    <>
      <ProFormTextArea
        name="prompt"
        label={l('models.instances.detail.prompt')}
        placeholder={l('models.instances.detail.prompt')}
        rules={[{ required: true }]}
        fieldProps={{ rows: 6 }}
      />
      <ProFormText
        name="lora_name"
        label="LoRA Name"
        colProps={{ span: 12 }}
        initialValue={undefined}
        fieldProps={{ size: 'large' }}
      />
      <ProFormDigit
        label="Max Tokens"
        name="max_tokens"
        placeholder="0 stands for maximum possible tokens"
        fieldProps={{ size: 'large' }}
        colProps={{ span: 12 }}
      />
      <ProFormDigit
        label="Top K"
        name="top_k"
        colProps={{ span: 12 }}
        fieldProps={{ size: 'large' }}
      />
      <ProFormField name="temperature" initialValue={1} colProps={{ span: 12 }}>
        <InputNumberWithSlider label="Temperature" min={0} max={2} step={0.01} precision={2} />
      </ProFormField>
    </>
  );
};
export default Generate;
