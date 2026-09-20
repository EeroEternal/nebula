import { Upload, message, Image, Button } from 'antd';
import type { UploadProps, ButtonProps } from 'antd';
import { InboxOutlined, DeleteOutlined } from '@ant-design/icons';
import { FC, useEffect, useState } from 'react';
import { lGet } from '@/utils/intl';
interface UploadImgDraggerProps {
  value?: string;
  onChange?: (v?: string) => void;
  title: React.ReactNode;
  description: React.ReactNode;
}
const UploadImgDragger: FC<UploadProps & UploadImgDraggerProps> = ({
  title,
  description,
  value,
  onChange,
  ...reset
}) => {
  const [base64, setBase64] = useState<string | undefined>(undefined);
  const handleBeforeUpload: UploadProps['beforeUpload'] = (file) => {
    const reader = new FileReader();
    reader.onload = () => {
      const base64 = reader.result as string;
      onChange?.(base64);
      message.success(lGet('global.message.uploadSuccess'));
    };
    reader.onerror = () => {
      onChange?.(undefined);
      message.error(lGet('global.message.uploadReadError', undefined, { fileName: file.name }));
    };
    reader.readAsDataURL(file);
  };
  const handleDelete: ButtonProps['onClick'] = (e) => {
    e.stopPropagation();
    onChange?.(undefined);
  };
  useEffect(() => setBase64(value), [value]);
  return (
    <div className="h-full">
      <Upload.Dragger
        beforeUpload={handleBeforeUpload}
        showUploadList={false}
        {...reset}
        className="relative w-full h-full ant-upload-dragger-mask"
      >
        <div className="absolute z-[100]  p-[4px] rounded-[8px] left-0 top-0 text-[#999]">
          Upload Mask
        </div>
        {base64 ? (
          <>
            <Button
              className="absolute top-[10px] right-[10px] z-[100]"
              icon={<DeleteOutlined />}
              size="small"
              onClick={handleDelete}
            />
            <div
              onClick={(e) => e.stopPropagation()}
              className="flex justify-center items-center w-full  h-full overflow-hidden"
            >
              <Image src={base64} height={160} className="object-contain" />
            </div>
          </>
        ) : (
          <>
            <p className="ant-upload-drag-icon">
              <InboxOutlined />
            </p>
            <p className="ant-upload-text">{title}</p>
            <p className="ant-upload-hint" style={{ fontSize: 12 }}>
              {description}
            </p>
          </>
        )}
      </Upload.Dragger>
    </div>
  );
};
export default UploadImgDragger;
