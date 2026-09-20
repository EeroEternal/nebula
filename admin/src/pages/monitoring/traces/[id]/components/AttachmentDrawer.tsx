import { Drawer } from 'antd';
import { FC, useState } from 'react';
import { IconFont } from '@/components';

interface AttachmentDrawerProps {
  type: 'image_url' | 'video_url' | 'audio_url';
  src: string;
}
const AttachmentDrawer: FC<AttachmentDrawerProps> = ({ type, src }) => {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const attr = {
    src,
    className: 'w-full h-full',
  };
  const renderAttachment = () => {
    if (type === 'audio_url') {
      return <IconFont className="text-[20px]" name="icon-yinpin" />;
    }
    if (type === 'video_url') {
      return <video {...attr} />;
    }
    return <img {...attr} />;
  };

  const renderDrawerContent = () => {
    if (type === 'audio_url') {
      return <audio controls src={src} className="w-full max-h-[500px]" />;
    }
    if (type === 'video_url') {
      return <video controls src={src} className="w-full max-h-[500px]" />;
    }
    return <img src={src} className="object-contain" />;
  };
  return (
    <>
      <div
        className="cursor-pointer overflow-hidden border w-[60px] h-[60px] rounded-[8px] flex items-center justify-center object-contain"
        onClick={() => setDrawerOpen(true)}
      >
        {renderAttachment()}
      </div>
      <Drawer destroyOnClose open={drawerOpen} width={600} onClose={() => setDrawerOpen(false)}>
        <div className="flex items-center justify-center">{renderDrawerContent()}</div>
      </Drawer>
    </>
  );
};
export default AttachmentDrawer;
