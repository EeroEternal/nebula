import { FC } from 'react';

interface AudioProps {
  result: Blob;
}
const Audio: FC<AudioProps> = ({ result }) => {
  const audioUrl = URL.createObjectURL(result);
  if (!audioUrl) return null;
  return <audio className="w-full" controls src={audioUrl} />;
};
export default Audio;
