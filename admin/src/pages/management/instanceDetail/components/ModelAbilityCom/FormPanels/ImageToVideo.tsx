import { FC } from 'react';
import { Form, Image } from 'antd';
import { size } from 'lodash';
import { Trash2 } from 'lucide-react';
import {
  ProFormUploadDragger,
  ProFormTextArea,
  ProFormDigit,
  ProFormField,
} from '@ant-design/pro-components';
import { IconButton, InputNumberWithSlider } from '@/components';
import { l } from '@/utils/intl';
import AdvancedParamsCard from '../AdvancedParamsCard';
import ExtraParams from '../ExtraParams';
import type { FormPanelProps } from '../modelAbilityConfig';

const ImageToVideo: FC<FormPanelProps> = ({ form }) => {
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
              description: l('models.instances.detail.uploadVideoDesc'),
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
        placeholder={l('models.instances.detail.promptVideo.placeholder')}
        rules={[{ required: true }]}
        fieldProps={{ rows: 3 }}
      />
      <ProFormTextArea
        name="negative_prompt"
        label={l('models.instances.detail.negativePrompt')}
        placeholder={l('models.instances.detail.negativePrompt.placeholder')}
        fieldProps={{ rows: 3 }}
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
      <ProFormField hidden name="n" initialValue={1} />
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
export default ImageToVideo;
