import { size } from 'lodash';
import { FC } from 'react';
import { DrawerAttachment } from '@/components';

interface ImageProps {
  result?: {
    data: { url?: string; b64_json?: string }[];
  };
}
const Image: FC<ImageProps> = ({ result }) => {
  if (!size(result?.data)) return null;
  const imageList =
    result?.data.map((item) => item.url || `data:image/png;base64,${item.b64_json}`) || [];
  return (
    <div className="grid grid-cols-2 gap-5">
      {imageList.map((url) => (
        <DrawerAttachment
          key={url}
          type="image"
          url={url}
          urlList={imageList}
          width={'100%'}
          height={300}
        />
      ))}
    </div>
  );
};
export default Image;
