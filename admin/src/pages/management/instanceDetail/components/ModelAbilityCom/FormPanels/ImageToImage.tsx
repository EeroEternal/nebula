import {
  ProFormUploadDragger,
  ProFormTextArea,
  ProFormSegmented,
  ProFormDigit,
  ProFormSelect,
  ProFormField,
} from '@ant-design/pro-components';
import { FC } from 'react';
import { Form, Image } from 'antd';
import { size } from 'lodash';
import { Trash2 } from 'lucide-react';
import { IconButton } from '@/components';
import { l } from '@/utils/intl';
import { GENERATIONS_NUMBER, RESPONSE_FORMAT, SAMPLING_METHODS } from '@/constants/modelData';
import ImageSize from '../ImageSize';
import ExtraParams from '../ExtraParams';
import AdvancedParamsCard from '../AdvancedParamsCard';
import type { FormPanelProps } from '../modelAbilityConfig';

const ImageToImage: FC<FormPanelProps> = ({ form }) => {
  const imageValue = Form.useWatch('image', form);
  const handleDeleteImg = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    form.setFieldsValue({ image: undefined });
  };
  return (
    <>
      <ProFormUploadDragger
        name="image"
        accept="image/*"
        rules={[{ required: true }]}
        {...(size(imageValue)
          ? {
              icon: (
                <IconButton
                  onClick={handleDeleteImg}
                  className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                >
                  <Trash2 size={16} />
                </IconButton>
              ),
              title: (
                <div onClick={(e) => e.stopPropagation()}>
                  <Image
                    className="object-contain"
                    width={'80%'}
                    height={110}
                    src={URL.createObjectURL(imageValue[0].originFileObj)}
                  />
                </div>
              ),
              description: false,
            }
          : {
              title: l('models.instances.detail.uploadImageTitle2'),
              description: l('models.instances.detail.uploadImageDesc'),
            })}
        fieldProps={{
          maxCount: 1,
          showUploadList: false,
          customRequest: () => {},
        }}
      />
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
