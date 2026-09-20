import { FC } from 'react';
import { Copy } from 'lucide-react';
import { IconButton } from '@/components';
import { copyToClipboard } from '@/utils';

interface DocanalyzeProps {
  result: {
    data: string;
  };
}
const Docanalyze: FC<DocanalyzeProps> = ({ result }) => {
  const jsonString = JSON.stringify(result?.data, null, 2);
  return (
    <>
      <IconButton
        className="absolute top-3 right-3 !w-7 !h-7  hover:text-primary hover:bg-primary/15"
        onClick={() => copyToClipboard(jsonString)}
      >
        <Copy size={16} />
      </IconButton>
      <pre className="min-h-full bg-background p-2 rounded-lg text-sm whitespace-pre-wrap font-mono leading-relaxed break-all">
        {jsonString}
      </pre>
    </>
  );
};
export default Docanalyze;
