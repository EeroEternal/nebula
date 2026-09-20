import { ProFormTextArea, ProFormText, ProFormField } from '@ant-design/pro-components';
import { FC } from 'react';

import { InputNumberWithSlider } from '@/components';
import { l, lGet } from '@/utils/intl';
import { ModelAbility } from '@/constants/modelData';
import AdvancedParamsCard from '../AdvancedParamsCard';
import AudioUpload from '../AudioUpload';
import type { FormPanelProps } from '../modelAbilityConfig';
import ExtraParams from '../ExtraParams';

const TextToAudio: FC<FormPanelProps> = ({ form, instanceDetail }) => {
  const modelAbility = instanceDetail.model_ability;
  const showPrompt = modelAbility.includes(ModelAbility.text2audioVoiceCloning);
  return (
    <>
      <ProFormTextArea
        rules={[{ required: true }]}
        name="input"
        placeholder={l('models.instances.detail.promptAudio.placeholder')}
        fieldProps={{
          rows: 8,
        }}
      />
      <ProFormText
        name="voice"
        label="Voice"
        fieldProps={{ size: 'large' }}
        colProps={{ span: 12 }}
      />
      <ProFormField initialValue={1} name="speed" colProps={{ span: 12 }}>
        <InputNumberWithSlider label="Speed" min={0.5} max={2} step={0.1} precision={1} />
      </ProFormField>

      {showPrompt && (
        <AdvancedParamsCard>
          <ProFormField
            label="Prompt Speech (for cloning)"
            name="prompt_speech"
            rules={[
              {
                required: !modelAbility.includes(ModelAbility.text2audioZeroShot),
                message: lGet('models.instances.detail.audioFile.rule'),
              },
            ]}
          >
            <AudioUpload form={form} />
          </ProFormField>
          <ProFormTextArea
            label="Prompt Text (for cloning)"
            name={['kwargs', 'prompt_text']}
            fieldProps={{
              rows: 4,
            }}
          />
          <ExtraParams />
        </AdvancedParamsCard>
      )}
      {!showPrompt && <ExtraParams />}
    </>
  );
};
export default TextToAudio;
