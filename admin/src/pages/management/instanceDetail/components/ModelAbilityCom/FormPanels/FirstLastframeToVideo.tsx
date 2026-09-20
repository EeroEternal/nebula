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

const FirstLastframeToVideo: FC<FormPanelProps> = ({ form }) => {
  const firstFrameValues = Form.useWatch('first_frame', form);
  const lastFrameValues = Form.useWatch('last_frame', form);
  const handleDeleteImg = (e: React.MouseEvent<HTMLDivElement>, formKey: string) => {
    e.stopPropagation();
    form.setFieldsValue({ [formKey]: undefined });
  };
  return (
    <>
      <ProFormUploadDragger
        name="first_frame"
        accept="image/*"
        rules={[{ required: true }]}
        colProps={{ span: 12 }}
        {...(size(firstFrameValues)
          ? {
              icon: (
                <IconButton
                  onClick={(e) => handleDeleteImg(e, 'first_frame')}
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
                    height={90}
                    src={URL.createObjectURL(firstFrameValues[0].originFileObj)}
                  />
                </div>
              ),
              description: false,
            }
          : {
              title: l('models.instances.detail.uploadImageFirstTitle'),
              description: l('models.instances.detail.uploadImageFirstDesc'),
            })}
        fieldProps={{
          maxCount: 1,
          showUploadList: false,
          customRequest: () => {},
        }}
      />
      <ProFormUploadDragger
        name="last_frame"
        accept="image/*"
        colProps={{ span: 12 }}
        rules={[{ required: true }]}
        {...(size(lastFrameValues)
          ? {
              icon: (
                <IconButton
                  onClick={(e) => handleDeleteImg(e, 'last_frame')}
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
                    height={90}
                    src={URL.createObjectURL(lastFrameValues[0].originFileObj)}
                  />
                </div>
              ),
              description: false,
            }
          : {
              title: l('models.instances.detail.uploadImageLastTitle'),
              description: l('models.instances.detail.uploadImageLastDesc'),
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
export default FirstLastframeToVideo;
