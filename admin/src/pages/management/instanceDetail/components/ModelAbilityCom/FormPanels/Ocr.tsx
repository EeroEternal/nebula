import { ProFormUploadDragger } from '@ant-design/pro-components';
import { FC } from 'react';
import { Form, Image } from 'antd';
import { size } from 'lodash';
import { Trash2 } from 'lucide-react';
import { IconButton } from '@/components';
import { l } from '@/utils/intl';
import type { FormPanelProps } from '../modelAbilityConfig';
import ExtraParams from '../ExtraParams';

const Ocr: FC<FormPanelProps> = ({ form }) => {
  const imageValue = Form.useWatch('image', form);
  const handleDeleteImg = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    form.setFieldsValue({ image: undefined });
  };
  return (
    <>
      <ProFormUploadDragger
        name="image"
        fieldProps={{
          maxCount: 1,
          showUploadList: false,
          customRequest: () => {},
        }}
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
              title: l('models.instances.detail.uploadImageTitle'),
              description: l('models.instances.detail.uploadImageDesc'),
            })}
      />
      <ExtraParams />
    </>
  );
};
export default Ocr;
