import { FC } from 'react';
import { ProFormTextArea, ProFormDigit, ProFormField } from '@ant-design/pro-components';
import { InputNumberWithSlider } from '@/components';
import { l } from '@/utils/intl';
import AdvancedParamsCard from '../AdvancedParamsCard';
import ExtraParams from '../ExtraParams';

const TextToVideo: FC = () => {
  return (
    <>
      <ProFormTextArea
        name="prompt"
        label={l('models.instances.detail.prompt')}
        placeholder={l('models.instances.detail.promptVideo.placeholder')}
        rules={[{ required: true }]}
        fieldProps={{
          rows: 4,
        }}
      />
      <ProFormTextArea
        name="negative_prompt"
        label={l('models.instances.detail.negativePrompt')}
        placeholder={l('models.instances.detail.negativePrompt.placeholder')}
        fieldProps={{ rows: 4 }}
      />
      <ProFormDigit
        name={['kwargs', 'width']}
        label="Width"
        initialValue={512}
        colProps={{ span: 12 }}
        fieldProps={{
          step: 4,
          precision: 0,
          size: 'large',
        }}
      />
      <ProFormDigit
        name={['kwargs', 'height']}
        label="Height"
        initialValue={512}
        colProps={{ span: 12 }}
        fieldProps={{
          step: 4,
          precision: 0,
          size: 'large',
        }}
      />
      <AdvancedParamsCard>
        <div className="grid grid-cols-2">
          <ProFormDigit
            name={['kwargs', 'num_frames']}
            label="Frames"
            initialValue={16}
            fieldProps={{
              precision: 0,
              size: 'large',
            }}
          />
          <ProFormDigit
            label="FPS"
            name={['kwargs', 'fps']}
            initialValue={8}
            fieldProps={{
              precision: 0,
              size: 'large',
            }}
          />
          <ProFormDigit
            label="Inference Steps"
            name={['kwargs', 'num_inference_steps']}
            initialValue={25}
            fieldProps={{
              precision: 0,
              size: 'large',
            }}
          />
          <ProFormField name={['kwargs', 'guidance_scale']} initialValue={7.5}>
            <InputNumberWithSlider
              label="Guidance Scale"
              min={1}
              max={20}
              step={0.1}
              precision={1}
            />
          </ProFormField>
        </div>
        <ExtraParams />
      </AdvancedParamsCard>
    </>
  );
};
export default TextToVideo;
