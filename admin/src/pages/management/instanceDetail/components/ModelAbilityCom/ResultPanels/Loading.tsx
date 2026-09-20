import { Progress } from 'antd';
import { Sparkles } from 'lucide-react';
import { FC } from 'react';
import { formatProgress } from '@/utils';
import { l } from '@/utils/intl';

interface GeneratingProps {
  progress?: number;
  label?: string;
}

const Generating: FC<GeneratingProps> = ({ label, progress }) => {
  const hasProgress = progress !== undefined;

  return (
    <div className="h-full flex flex-col items-center justify-center gap-4 px-8">
      <div className="relative">
        <div className="h-20 w-20 rounded-full bg-primary/10 flex items-center justify-center">
          <Sparkles className="h-8 w-8 text-primary animate-pulse" />
        </div>
        <div
          className="absolute inset-0 rounded-full border-2 border-transparent border-t-primary/40 animate-spin"
          style={{ animationDuration: '2s' }}
        />
        <div
          className="absolute -inset-2 rounded-full border-2 border-transparent border-b-primary/20 animate-spin"
          style={{ animationDuration: '3s', animationDirection: 'reverse' }}
        />
      </div>
      <div className="text-sm font-medium flex gap-1 items-center">
        {label ? l(label) : l('models.instances.detail.generate.loading')}
        <div className="flex gap-1">
          <span
            className="h-1.5 w-1.5 rounded-full bg-primary/60 animate-bounce"
            style={{ animationDelay: '0ms' }}
          />
          <span
            className="h-1.5 w-1.5 rounded-full bg-primary/60 animate-bounce"
            style={{ animationDelay: '150ms' }}
          />
          <span
            className="h-1.5 w-1.5 rounded-full bg-primary/60 animate-bounce"
            style={{ animationDelay: '300ms' }}
          />
        </div>
      </div>
      {hasProgress && (
        <div className="w-full max-w-xs space-y-1">
          <Progress
            showInfo={false}
            status="normal"
            percent={progress}
            strokeColor={window.THEME_PRIMARY_COLOR}
            className="!leading-[0]"
          />
          <div className="flex items-center justify-between text-xs text-muted">
            <span>{l('models.instances.detail.progress')}</span>
            <span className="font-mono">{formatProgress(progress)}%</span>
          </div>
        </div>
      )}
    </div>
  );
};
export default Generating;
