import { FC } from 'react';
import { size } from 'lodash';
import { Copy } from 'lucide-react';
import { IconButton } from '@/components';
import { copyToClipboard } from '@/utils';

interface RerankProps {
  result: {
    id: string;
    meta: {
      api_version: string;
      billed_units: string;
      tokens: number;
      warnings: string;
    };
    model: string;
    results: {
      document: string;
      index: number;
      relevance_score: number;
    }[];
  };
}
const Rerank: FC<RerankProps> = ({ result }) => {
  if (!size(result?.results)) return null;
  const jsonString = JSON.stringify(result.results, null, '\t');
  const meta = JSON.stringify(result.meta, null, '\t');
  return (
    <>
      <IconButton
        className="absolute top-3 right-3 !w-7 !h-7  hover:text-primary hover:bg-primary/15"
        onClick={() => copyToClipboard(jsonString)}
      >
        <Copy size={16} />
      </IconButton>
      <pre className="bg-background p-2 rounded-lg text-sm whitespace-pre-wrap font-mono leading-relaxed break-all">
        {jsonString}
      </pre>
      <pre className="bg-background p-2 my-2 rounded-lg text-sm whitespace-pre-wrap font-mono leading-relaxed break-all">{`meta: ${meta}`}</pre>
      <div className="text-muted">ID: {result.id}</div>
    </>
  );
};
export default Rerank;
