import { App, Button, Tag } from 'antd';
import { useRequest } from 'ahooks';
import { HeartPulse } from 'lucide-react';
import { formatDisplayTime } from '@/utils';
import { l, lGet } from '@/utils/intl';
import { readRequestFailure } from '@/utils/formatApiError';
import request from '@/utils/request';

type Props = {
  modelUid: string;
  lastOk?: boolean | null;
  lastAt?: number | null;
  lastMessage?: string | null;
  onDone?: () => void;
};

const ProbePanel = ({ modelUid, lastOk, lastAt, lastMessage, onDone }: Props) => {
  const { message } = App.useApp();
  const { loading, run } = useRequest(
    () =>
      request(`/models/${modelUid}/probe`, {
        method: 'POST',
        data: {},
      }),
    {
      manual: true,
      onSuccess: (res) => {
        const body = res?.data?.data || res?.data || {};
        const ok = body?.ok ?? body?.success;
        const detail =
          body?.message ||
          body?.detail ||
          (ok === false
            ? String(lGet('monitor.instances.probe.fail'))
            : String(lGet('monitor.instances.probe.ok')));
        if (ok === false) {
          message.error(
            `${String(lGet('monitor.instances.probe'))}: ${detail}`,
          );
        } else {
          message.success(
            `${String(lGet('monitor.instances.probe'))}: ${detail}`,
          );
        }
        onDone?.();
      },
      onError: (e: unknown) => {
        message.error(
          readRequestFailure(e) ||
            String(lGet('monitor.instances.probe.fail')),
        );
        onDone?.();
      },
    },
  );

  return (
    <div className="rounded-lg border border-[color:var(--c-border-light)] p-3 space-y-2">
      <div className="text-sm font-medium flex items-center gap-1.5">
        <HeartPulse size={14} />
        {l('monitor.instances.probe')}
      </div>
      <div className="text-xs text-muted">
        {lastOk == null ? (
          l('monitor.instances.probe.none')
        ) : (
          <>
            <Tag color={lastOk ? 'success' : 'error'}>
              {lastOk
                ? l('monitor.instances.probe.ok')
                : l('monitor.instances.probe.fail')}
            </Tag>
            {lastAt ? formatDisplayTime(lastAt) : null}
            {lastMessage ? (
              <div className="mt-1 break-all opacity-90">{lastMessage}</div>
            ) : null}
          </>
        )}
      </div>
      <Button type="primary" size="small" loading={loading} onClick={() => run()}>
        {l('monitor.instances.probe.run')}
      </Button>
    </div>
  );
};

export default ProbePanel;
