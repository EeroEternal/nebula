import { FC } from 'react';
import { Copy } from 'lucide-react';
import { ReactMarkdown, IconButton } from '@/components';
import { copyToClipboard, formatDisplayTime } from '@/utils';

interface GenerateProps {
  result: {
    choices: { text: string }[];
    created: number;
    usage: {
      total_tokens: number;
      prompt_tokens: number;
      completion_tokens: number;
    };
  };
}
const Generate: FC<GenerateProps> = ({ result }) => {
  const text = result?.choices?.[0]?.text;
  if (!text) return null;
  return (
    <>
      <IconButton
        className="absolute top-3 right-3 !w-7 !h-7  hover:text-primary hover:bg-primary/15"
        onClick={() => copyToClipboard(text)}
      >
        <Copy size={16} />
      </IconButton>
      <div className="flex flex-col justify-between gap-2 min-h-full relative">
        <ReactMarkdown classnames="rounded-2">{text}</ReactMarkdown>
        <div className="shrink-0 flex flex-col items-end text-[#8a8a8a]">
          {result?.created && (
            <div>{formatDisplayTime(result.created * 1000)}</div>
          )}
          {!!result?.usage?.total_tokens && (
            <div>
              {result?.usage?.prompt_tokens} → {result?.usage?.completion_tokens} (∑
              {result?.usage?.total_tokens})
            </div>
          )}
        </div>
      </div>
    </>
  );
};
export default Generate;
