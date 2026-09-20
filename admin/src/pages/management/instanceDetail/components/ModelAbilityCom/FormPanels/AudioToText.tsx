import { FC } from 'react';
import { ProFormText, ProFormField, ProFormTextArea } from '@ant-design/pro-components';
import { l, lGet } from '@/utils/intl';
import { InputNumberWithSlider } from '@/components';
import AudioUpload from '../AudioUpload';
import type { FormPanelProps } from '../modelAbilityConfig';

const AudioToText: FC<FormPanelProps> = ({ form }) => {
  return (
    <>
      <ProFormField
        name="file"
        rules={[{ required: true, message: lGet('models.instances.detail.audioFile.rule') }]}
      >
        <AudioUpload form={form} />
      </ProFormField>
      <ProFormTextArea
        label={l('models.instances.detail.prompt')}
        name="prompt"
        placeholder="Provide context or vocabulary(Optional)"
      />
      <ProFormText
        name="language"
        label="Language"
        placeholder="Example: en、zh..."
        fieldProps={{ size: 'large' }}
      />
      <ProFormField name="temperature" initialValue={0}>
        <InputNumberWithSlider label="Temperature" min={0} max={1} step={0.1} precision={1} />
      </ProFormField>
    </>
  );
};
export default AudioToText;
