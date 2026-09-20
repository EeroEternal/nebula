import {
  ProFormTextArea,
  ProFormSegmented,
  ProFormDigit,
  ProFormSelect,
  ProFormField,
} from '@ant-design/pro-components';
import { FC } from 'react';
import { InputNumberWithSlider } from '@/components';
import { l, lGet } from '@/utils/intl';
import { GENERATIONS_NUMBER, RESPONSE_FORMAT, SAMPLING_METHODS } from '@/constants/modelData';
import ImageEditorCreatMask from '../ImageEditorCreatMask';
import ImageSize from '../ImageSize';
import ExtraParams from '../ExtraParams';
import AdvancedParamsCard from '../AdvancedParamsCard';

import type { FormPanelProps } from '../modelAbilityConfig';

const ImageToImage: FC<FormPanelProps> = ({ form }) => {
  return (
    <>
      <ProFormField
        name="inpainting_image"
        rules={[{ required: true, message: lGet('models.instances.detail.uploadInpaintError') }]}
      >
        <ImageEditorCreatMask
          updateMask={(value) => {
            form.setFieldValue('mask_image', value);
          }}
        />
      </ProFormField>
      <ProFormField name="mask_image" hidden />

      <ProFormTextArea
        name="prompt"
        label={l('models.instances.detail.prompt')}
        placeholder={l('models.instances.detail.promptImage.placeholder')}
        rules={[{ required: true }]}
        fieldProps={{ rows: 3 }}
      />
      <ProFormTextArea
        name="negative_prompt"
        label={l('models.instances.detail.negativePrompt')}
        placeholder={l('models.instances.detail.negativePrompt.placeholder')}
        fieldProps={{ rows: 3 }}
      />
      <ProFormField name="size" initialValue="1024x1024" className="w-full">
        <ImageSize />
      </ProFormField>
      <ProFormSegmented
        name="n"
        label={l('models.instances.detail.generationsCount')}
        initialValue={GENERATIONS_NUMBER[0]}
        colProps={{ span: 12 }}
        fieldProps={{
          block: true,
          options: GENERATIONS_NUMBER,
          size: 'large',
        }}
      />
      <ProFormSegmented
        name="response_format"
        label={l('models.instances.detail.responseFormat')}
        initialValue={RESPONSE_FORMAT[0]}
        colProps={{ span: 12 }}
        fieldProps={{
          block: true,
          options: RESPONSE_FORMAT,
          size: 'large',
        }}
      />
      <AdvancedParamsCard>
        <div className="grid grid-cols-2">
          <ProFormDigit
            label="Guidance Scale"
            name={['kwargs', 'guidance_scale']}
            initialValue={-1}
            fieldProps={{
              min: -1,
              size: 'large',
            }}
          />

          <ProFormDigit
            label="Inference Step Number"
            name={['kwargs', 'num_inference_steps']}
            initialValue={-1}
            fieldProps={{
              min: -1,
              size: 'large',
            }}
          />

          <ProFormDigit
            label="Padding image to multiple"
            name={['kwargs', 'padding_image_to_multiple']}
            initialValue={-1}
            fieldProps={{
              min: -1,
              size: 'large',
            }}
          />
          <ProFormField name={['kwargs', 'strength']} initialValue={0.6}>
            <InputNumberWithSlider label="Strength" min={0} max={1} step={0.1} precision={1} />
          </ProFormField>

          <ProFormSelect
            name={['kwargs', 'sampler_name']}
            label="Sampling Method"
            options={SAMPLING_METHODS}
            fieldProps={{ size: 'large' }}
          />
        </div>
        <ExtraParams />
      </AdvancedParamsCard>
    </>
  );
};
export default ImageToImage;
