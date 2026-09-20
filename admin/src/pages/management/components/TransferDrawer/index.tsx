import { Alert, Button, Checkbox, Drawer, Radio, Space } from 'antd';
import { ArrowRightLeft } from 'lucide-react';
import { ReactNode, useEffect, useMemo, useState } from 'react';

import { l } from '@/utils/intl';

export type TransferMode = 'migrate' | 'copy';

export type TransferNodeOption = {
  id: string;
  label: ReactNode;
  hint?: ReactNode;
  sourceDisabled?: boolean;
};

type Props = {
  open: boolean;
  title: ReactNode;
  subject?: ReactNode;
  nodes: TransferNodeOption[];
  initialSource?: string;
  initialMode?: TransferMode;
  submitLabel?: ReactNode;
  modeHint?: { migrate?: ReactNode; copy?: ReactNode };
  warning?: ReactNode;
  submitting?: boolean;
  onClose: () => void;
  onSubmit: (payload: {
    source: string;
    dests: string[];
    mode: TransferMode;
  }) => void | Promise<void>;
};

const TransferDrawer: React.FC<Props> = ({
  open,
  title,
  subject,
  nodes,
  initialSource,
  initialMode = 'migrate',
  submitLabel,
  modeHint,
  warning,
  submitting,
  onClose,
  onSubmit,
}) => {
  const [source, setSource] = useState('');
  const [dests, setDests] = useState<string[]>([]);
  const [mode, setMode] = useState<TransferMode>(initialMode);

  const sourceable = useMemo(
    () => nodes.filter((n) => n.id && !n.sourceDisabled),
    [nodes],
  );
  const sourceableKey = sourceable.map((n) => n.id).join('\0');

  useEffect(() => {
    if (!open) return;
    setDests([]);
    setMode(initialMode);
  }, [open, initialMode]);

  useEffect(() => {
    if (!open) return;
    setSource((prev) => {
      if (prev && sourceable.some((n) => n.id === prev)) return prev;
      if (initialSource && sourceable.some((n) => n.id === initialSource)) {
        return initialSource;
      }
      return sourceable[0]?.id || '';
    });
    // sourceable 随父组件新数组换引用；用 id 串判断集合是否真变
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initialSource, sourceableKey]);

  const toggleDest = (id: string, checked: boolean) => {
    setDests((prev) =>
      checked ? [...prev.filter((x) => x !== id), id] : prev.filter((x) => x !== id),
    );
  };

  const destCandidates = nodes.filter((n) => n.id && n.id !== source);
  const allSelected =
    destCandidates.length > 0 && destCandidates.every((n) => dests.includes(n.id));

  const handleSubmit = async () => {
    const clean = dests.filter((id) => id && id !== source);
    if (!source || !clean.length) {
      return;
    }
    await onSubmit({ source, dests: clean, mode });
  };

  return (
    <Drawer
      title={title}
      open={open}
      onClose={onClose}
      width={460}
      destroyOnClose
      styles={{
        wrapper: { borderRadius: 'var(--r-xl) 0 0 var(--r-xl)' },
      }}
      footer={
        <Space style={{ width: '100%', justifyContent: 'flex-end' }}>
          <Button
            type="primary"
            icon={<ArrowRightLeft size={16} />}
            loading={submitting}
            disabled={!source || !dests.filter((id) => id !== source).length}
            onClick={handleSubmit}
          >
            {submitLabel ||
              (mode === 'migrate'
                ? l('models.downloads.migrateSubmit')
                : l('models.repository.detail.versions.copySubmit'))}
          </Button>
          <Button onClick={onClose}>{l('global.actions.cancel')}</Button>
        </Space>
      }
    >
      <div className="flex flex-col gap-4">
        {subject}
        <div className="rounded-lg bg-[var(--c-surface-2)] px-4 py-3 flex flex-col gap-3">
          <span className="text-[12px] font-semibold text-secondary">
            {l('models.downloads.migrateSource')}
          </span>
          <div className="flex flex-col gap-1.5">
            {nodes.map((n) => (
              <label
                key={n.id}
                className={[
                  'flex items-center gap-2 rounded-md border px-3 py-2',
                  source === n.id
                    ? 'border-[color:var(--c-primary)] bg-card'
                    : 'border-[color:var(--c-border-light)] bg-card',
                  n.sourceDisabled
                    ? 'opacity-50 cursor-not-allowed'
                    : 'cursor-pointer',
                ].join(' ')}
              >
                <input
                  type="radio"
                  name="transfer-source"
                  checked={source === n.id}
                  disabled={n.sourceDisabled}
                  onChange={() => {
                    setSource(n.id);
                    setDests((prev) => prev.filter((x) => x !== n.id));
                  }}
                />
                <span className="text-sm text-default min-w-0 flex-1">{n.label}</span>
                {n.hint ? (
                  <span className="ml-auto text-[11px] text-muted shrink-0">{n.hint}</span>
                ) : null}
              </label>
            ))}
          </div>
        </div>

        <div className="rounded-lg bg-[var(--c-surface-2)] px-4 py-3 flex flex-col gap-2">
          <span className="text-[12px] font-semibold text-secondary">
            {l('models.transfer.mode')}
          </span>
          <Radio.Group
            value={mode}
            onChange={(e) => setMode(e.target.value)}
            className="flex gap-4"
          >
            <Radio value="migrate">{l('models.transfer.modeMigrate')}</Radio>
            <Radio value="copy">{l('models.transfer.modeCopy')}</Radio>
          </Radio.Group>
          {modeHint?.[mode] ? (
            <div className="text-[11px] text-muted">{modeHint[mode]}</div>
          ) : null}
        </div>

        <div className="rounded-lg bg-[var(--c-surface-2)] px-4 py-3 flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[12px] font-semibold text-secondary">
              {l('models.downloads.migrateDest')}
            </span>
            <Button
              type="link"
              size="small"
              className="!px-0"
              disabled={!destCandidates.length}
              onClick={() =>
                setDests(allSelected ? [] : destCandidates.map((n) => n.id))
              }
            >
              {allSelected
                ? l('global.actions.deselectAll')
                : l('models.engines.selectAllNodes')}
            </Button>
          </div>
          {destCandidates.length ? (
            <div className="flex flex-col gap-1.5">
              {destCandidates.map((n) => (
                <label
                  key={n.id}
                  className="flex items-center gap-2 rounded-md border border-[color:var(--c-border-light)] bg-card px-3 py-2 cursor-pointer"
                >
                  <Checkbox
                    checked={dests.includes(n.id)}
                    onChange={(e) => toggleDest(n.id, e.target.checked)}
                  />
                  <span className="text-sm text-default min-w-0 flex-1">{n.label}</span>
                  {n.hint ? (
                    <span className="text-[11px] text-muted shrink-0">{n.hint}</span>
                  ) : null}
                </label>
              ))}
            </div>
          ) : (
            <div className="text-sm text-muted">
              {l('models.downloads.migrateNoDest')}
            </div>
          )}
        </div>

        {mode === 'migrate' && warning ? (
          <Alert type="warning" showIcon message={warning} />
        ) : null}
      </div>
    </Drawer>
  );
};

export default TransferDrawer;
