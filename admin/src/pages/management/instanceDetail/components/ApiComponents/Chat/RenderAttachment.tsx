import { FC, useState } from 'react';
import { Drawer } from 'antd';
import { X, Music } from 'lucide-react';
import { FileInfo } from './index';
import classNames from 'classnames';
interface RenderAttachmentProps {
  fileInfo: FileInfo;
  deleteFile?: () => void;
  className?: string;
}
const RenderAttachment: FC<RenderAttachmentProps> = ({ fileInfo, deleteFile, className = '' }) => {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const attr = {
    src: fileInfo.url,
    className: 'shrink-0 w-[30px] h-[30px] rounded-[8px] object-contain',
  };
  const renderAttachment = () => {
    if (fileInfo.type === 'audio') {
      return (
        <div className="w-[30px] h-[30px] rounded-[8px] flex items-center justify-center">
          <Music size={20} />
        </div>
      );
    }
    if (fileInfo.type === 'video') {
      return <video {...attr} />;
    }
    return <img {...attr} />;
  };

  const renderDrawerContent = () => {
    if (fileInfo.type === 'audio') {
      return <audio controls src={fileInfo.url} className="w-full max-h-[500px]" />;
    }
    if (fileInfo.type === 'video') {
      return <video controls src={fileInfo.url} className="w-full max-h-[500px]" />;
    }
    return <img src={fileInfo.url} className="object-contain" />;
  };
  const handleDeleteFile = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    deleteFile?.();
  };
  const showDrawer = () => {
    setDrawerOpen(true);
  };
  return (
    <>
      <div
        className={classNames(
          'group bg-background fade-in rounded-[10px] w-[250px] p-[8px] text-muted text-[12px] h-[50px] flex gap-x-[8px] items-center relative cursor-pointer hover:shadow-img-shadow',
          className,
        )}
        style={{ transition: 'box-shadow .2s ease-in-out' }}
        onClick={showDrawer}
      >
        {renderAttachment()}
        <div className="overflow-hidden">
          <div className="truncate">{fileInfo.name}</div>
          <div>{fileInfo.size}</div>
        </div>
        {!!deleteFile && (
          <div
            onClick={handleDeleteFile}
            className="absolute text-[16px] right-[-5px] top-[-5px] border rounded-full items-center justify-center p-[2px] bg-card hidden group-hover:flex"
          >
            <X size={10} />
          </div>
        )}
      </div>
      <Drawer destroyOnClose open={drawerOpen} width={600} onClose={() => setDrawerOpen(false)}>
        <div className="flex items-center justify-center">{renderDrawerContent()}</div>
      </Drawer>
    </>
  );
};
export default RenderAttachment;
