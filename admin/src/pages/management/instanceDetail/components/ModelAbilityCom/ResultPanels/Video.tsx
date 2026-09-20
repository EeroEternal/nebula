import { size } from 'lodash';
import { FC } from 'react';
import { formatDisplayTime } from '@/utils';

interface VideoProps {
  result: {
    created: number;
    data: {
      url?: string;
      b64_json?: string;
    }[];
  };
}
const Video: FC<VideoProps> = ({ result }) => {
  if (!size(result?.data)) return null;
  return (
    <div className="min-h-full flex flex-col justify-between gap-2">
      {result?.data.map((item) => (
        <video
          className="h-[250px] w-full rounded-lg"
          controls
          key={item.url || item.b64_json}
          src={item.url || `data:video/mp4;base64,${item.b64_json}`}
        />
      ))}
      {result.created && (
        <div className="shrink-0 text-right text-muted">
          {formatDisplayTime(result.created * 1000)}
        </div>
      )}
    </div>
  );
};
export default Video;
