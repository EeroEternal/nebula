import { FC } from 'react';

interface AudioToTextProps {
  result: {
    text: string;
  };
}
const AudioToText: FC<AudioToTextProps> = ({ result }) => {
  if (!result.text) return null;
  return <div>{result.text}</div>;
};
export default AudioToText;
