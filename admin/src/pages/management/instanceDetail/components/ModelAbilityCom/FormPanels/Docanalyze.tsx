import { FC } from 'react';
import { ProFormUploadDragger } from '@ant-design/pro-components';
import { Form } from 'antd';
import { size } from 'lodash';
import { Trash2 } from 'lucide-react';
import { l, lGet } from '@/utils/intl';
import { IconButton } from '@/components';
import type { FormPanelProps } from '../modelAbilityConfig';

const Docanalyze: FC<FormPanelProps> = ({ form }) => {
  const fileValue = Form.useWatch('file', form);
  const getFileIcon = (type: string) => {
    if (type.includes('pdf')) return 'PDF';
    if (type.includes('word') || type.includes('docx')) return 'DOC';
    if (type.includes('sheet') || type.includes('xlsx') || type.includes('csv')) return 'XLS';
    if (type.includes('presentation') || type.includes('pptx')) return 'PPT';
    if (type.includes('image')) return 'IMG';
    return 'FILE';
  };
  const handleDelete = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    form.setFieldValue('file', undefined);
  };
  return (
    <ProFormUploadDragger
      name="file"
      rules={[{ required: true, message: lGet('model.running.uploadEmpty') }]}
      {...(size(fileValue)
        ? {
            icon: (
              <IconButton
                onClick={handleDelete}
                className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
              >
                <Trash2 size={16} />
              </IconButton>
            ),
            title: (
              <div className="flex flex-col items-center">
                <div className="h-14 w-14 rounded-xl bg-primary/10 flex items-center justify-center mb-2">
                  <span className="text-xs font-bold text-primary">
                    {getFileIcon(fileValue[0].type)}
                  </span>
                </div>
                {fileValue[0].name}
              </div>
            ),
            description: false,
          }
        : {
            title: l('models.instances.detail.uploadDocumentTitle'),
            description: l('models.instances.detail.uploadDocumentDesc'),
          })}
      fieldProps={{
        maxCount: 1,
        customRequest: ({ onSuccess }) => onSuccess?.('ok'),
        showUploadList: false,
      }}
    />
  );
};
export default Docanalyze;
