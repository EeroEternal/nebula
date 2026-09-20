import { Image } from 'antd';
import type { ImageProps } from 'antd';
import { EyeOutlined, CloseOutlined } from '@ant-design/icons';
import React, { useState, FC } from 'react';
import classNames from 'classnames';
import ReactDOM from 'react-dom';
export type AttachmentType = 'image' | 'video' | 'audio';
interface BaseProps {
  url: string;
  type: 'image' | 'video' | 'audio';
}
interface ImageTypeProps extends ImageProps {
  type: 'image';
  /** 如果传了，则支持左右切换 */
  urlList?: string[];
}
interface VideoAudioTypeProps {
  type?: 'video' | 'audio';
  children?: React.ReactNode;
  className?: string;
}
type DrawerAttachmentProps = BaseProps & (ImageTypeProps | VideoAudioTypeProps);
const DrawerAttachment: FC<DrawerAttachmentProps> = (props) => {
  const { type, url, className = '', ...reset } = props;
  const [open, setOpen] = useState(false);

  if (type === 'image') {
    return (
      <Image.PreviewGroup items={props?.urlList || [url]}>
        <Image width={80} height={80} className="object-cover rounded-[8px]"  src={url} {...reset} />
      </Image.PreviewGroup>
    );
  }
  const renderPlayer = () => {
    if (type === 'audio') {
      return (
        <audio
          controls
          className={classNames('w-full h-[60px] object-contain', className)}
          src={url}
        />
      );
    }
    return (
      <div className={classNames("border relative rounded-[4px] overflow-hidden group w-[80px] h-[80px]",className)} onClick={() => setOpen(true)}>
        {props?.children || <video  className='w-full h-full object-contain' src={url} />}
        <div className="cursor-pointer bg-[rgba(0,0,0,0.5)] inset-0 absolute opacity-0 group-hover:opacity-100 flex text-[#fff] items-center justify-center">
          <EyeOutlined />
          预览
        </div>
      </div>
    );
  };
  return (
    <>
      {renderPlayer()}
      {open && ReactDOM.createPortal(
        <div
          className={'fixed inset-0 z-[1000] flex items-center justify-center bg-[rgba(0,0,0,0.45)] fade-in'}
          onClick={() => setOpen(false)}
        >
          <div
            className="absolute z-[1001] top-[32px] right-[32px] w-[42px] h-[42px] flex items-center justify-center cursor-pointer bg-[rgba(0,0,0,0.1)] hover:bg-[rgba(0,0,0,0.2)] text-[#fff] text-[18px] rounded-full"
            onClick={() => setOpen(false)}
          >
            <CloseOutlined />
          </div>
          <div className="max-w-screen-lg h-[80vh] ">
            <video controls autoPlay className="w-full h-full object-contain" src={url} />
          </div>
        </div>,
         document.body
      )}
    </>
  );
};
export default DrawerAttachment;
