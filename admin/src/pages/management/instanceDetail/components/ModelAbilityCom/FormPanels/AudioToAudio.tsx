import { FC } from 'react';
import { ProFormField, ProFormTextArea } from '@ant-design/pro-components';
import { l, lGet } from '@/utils/intl';
import ExtraParams from '../ExtraParams';
import AudioUpload from '../AudioUpload';
import type { FormPanelProps } from '../modelAbilityConfig';

const AudioToAudio: FC<FormPanelProps> = ({ form }) => {
  return (
    <>
      <ProFormField
        name="prompt_speech"
        rules={[{ required: true, message: lGet('models.instances.detail.audioFile.rule') }]}
      >
        <AudioUpload form={form} />
      </ProFormField>
      <ProFormTextArea
        rules={[{ required: true }]}
        name="input"
        placeholder={l('models.instances.detail.promptAudio.placeholder')}
        fieldProps={{
          rows: 8,
        }}
      />
      <ExtraParams />
    </>
  );
};
export default AudioToAudio;
