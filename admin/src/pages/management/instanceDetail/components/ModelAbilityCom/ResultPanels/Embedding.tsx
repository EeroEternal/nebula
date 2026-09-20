import { FC } from 'react';
import { size } from 'lodash';
import { Copy } from 'lucide-react';
import { IconButton } from '@/components';
import { copyToClipboard } from '@/utils';

interface EmbeddingProps {
  result?: {
    data: {
      embedding: number[];
      index: number;
      object: string;
    }[];
    model: string;
    model_replica: string;
    object: string;
    usage: {
      prompt_tokens: number;
      total_tokens: number;
    };
  };
}
const Embedding: FC<EmbeddingProps> = ({ result }) => {
  const embeddingList = result?.data?.[0]?.embedding || [];
  if (!size(embeddingList)) return null;
  const value = JSON.stringify(embeddingList, null, '\t');
  return (
    <div>
      <div className="absolute top-3 right-3 flex items-center gap-1">
        {embeddingList.length}
        <IconButton
          className=" !w-7 !h-7  hover:text-primary hover:bg-primary/15"
          onClick={() => copyToClipboard(value)}
        >
          <Copy size={16} />
        </IconButton>
      </div>
      <pre className="min-h-full bg-background p-2 rounded-lg text-sm whitespace-pre-wrap font-mono leading-relaxed break-all">
        {value}
      </pre>
    </div>
  );
};
export default Embedding;
