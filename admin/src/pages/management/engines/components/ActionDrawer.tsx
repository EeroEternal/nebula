import {
  Alert,
  App,
  Button,
  Checkbox,
  Drawer,
  Space,
} from 'antd';
import {
  Download,
  Trash2,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import {
  deleteEngineImage,
  EngineNodeImages,
  EngineNodeInfo,
  EngineProbeResult,
  EnginePullProgress,
  migrateEngineImage,
  probeEngineImage,
  pullEngineImage,
} from '@/services/engineImages';
import { stringifyDetail } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';

import {
  displayNodeName,
  engineTagLabel,
  engineTagTone,
  nodeInventoryHasImage,
  PullDraft,
  SUPERVISOR_NODE_ID,
} from '../utils';
import StatusTag from './StatusTag';
import TransferDrawer from '../../components/TransferDrawer';

export type ActionMode = 'download' | 'delete' | 'migrate';

export type ActionDraft = PullDraft & {
  mode: ActionMode;
};

type Props = {
  open: boolean;
  draft: ActionDraft | null;
  nodes: EngineNodeInfo[];
  localNodes: EngineNodeImages[];
  localImages: string[];
  onClose: () => void;
  onStarted: (progress: EnginePullProgress) => void;
};

const ActionDrawer: React.FC<Props> = ({
  open,
  draft,
  nodes,
  localNodes,
  localImages,
  onClose,
  onStarted,
}) => {
  const { message, modal } = App.useApp();
  const [submitting, setSubmitting] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);

  const nodeIds = useMemo(
    () => nodes.map((n) => n.node_id),
    [nodes],
  );

  const nodeLabel = (id: string) => {
    const n = nodes.find((x) => x.node_id === id);
    return n ? displayNodeName(n) : id;
  };

  useEffect(() => {
    if (!open || !draft) return;
    if (draft.mode === 'download') {
      setSelected([SUPERVISOR_NODE_ID]);
    } else if (draft.mode === 'delete') {
      const present = nodeIds.filter((id) =>
        nodeInventoryHasImage(localNodes, id, draft.image, localImages),
      );
      setSelected(present.length ? present : []);
    }
  }, [open, draft?.image, draft?.mode, nodeIds, localNodes, localImages]);

  const title = useMemo(() => {
    if (!draft) return '';
    if (draft.mode === 'download') return l('models.engines.actionDownloadTitle');
    if (draft.mode === 'delete') return l('models.engines.actionDeleteTitle');
    return l('models.engines.actionMigrateTitle');
  }, [draft?.mode]);

  const toggle = (id: string, checked: boolean) => {
    setSelected((prev) =>
      checked ? Array.from(new Set([...prev, id])) : prev.filter((x) => x !== id),
    );
  };

  const selectableTargets = useMemo(() => [...nodeIds], [nodeIds]);

  const allTargetsSelected =
    selectableTargets.length > 0 &&
    selectableTargets.every((id) => selected.includes(id));

  const selectAll = () => {
    setSelected(allTargetsSelected ? [] : selectableTargets);
  };

  const enqueuePullResult = (data: EnginePullProgress) => {
    onStarted(data);
    (data.relay_tasks || []).forEach((t) => onStarted(t));
  };

  const startDirectPulls = async (targets: string[]) => {
    if (!draft) return;
    for (const target of targets) {
      const res = await pullEngineImage({
        engine: draft.engine,
        version: draft.version,
        image: draft.fromRemote ? draft.image : undefined,
        target,
        wait: false,
      });
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.pullFailed')),
        );
        return false;
      }
      enqueuePullResult(res.data as EnginePullProgress);
    }
    return true;
  };

  const startRelayPull = async (
    relayDests: string[],
    keepSourceOnRelay: boolean,
  ) => {
    if (!draft) return false;
    const res = await pullEngineImage({
      engine: draft.engine,
      version: draft.version,
      image: draft.fromRemote ? draft.image : undefined,
      target: SUPERVISOR_NODE_ID,
      relay_dests: relayDests,
      keep_source_on_relay: keepSourceOnRelay,
      wait: false,
    });
    if (!res?.success) {
      message.error(
        stringifyDetail(res?.data?.detail) ||
          String(lGet('models.engines.pullFailed')),
      );
      return false;
    }
    enqueuePullResult(res.data as EnginePullProgress);
    return true;
  };

  const handleDownload = async () => {
    if (!draft) return;
    const workers = selected.filter((id) => id !== SUPERVISOR_NODE_ID);
    const includeSupervisor = selected.includes(SUPERVISOR_NODE_ID);

    const reachableWorkers: string[] = [];
    const unreachableWorkers: string[] = [];

    for (const target of workers) {
      const res = await probeEngineImage({ image: draft.image, target });
      const data = (res?.data || {}) as EngineProbeResult;
      if (res?.success && data.ok) {
        reachableWorkers.push(target);
      } else {
        unreachableWorkers.push(target);
      }
    }

    if (!unreachableWorkers.length) {
      const directTargets = [
        ...(includeSupervisor ? [SUPERVISOR_NODE_ID] : []),
        ...reachableWorkers,
      ];
      const ok = await startDirectPulls(directTargets);
      if (!ok) return;
      message.success(
        String(
          lGet('models.engines.pullStartedMulti', undefined, {
            count: directTargets.length,
          }),
        ),
      );
      onClose();
      return;
    }

    // Relay already pulls on supervisor; keep it only if user also selected it.
    const keepSourceOnRelay = includeSupervisor;
    // Reachable workers still pull directly; supervisor covered by relay pull.
    const directTargets = [...reachableWorkers];

    const names = unreachableWorkers.map(nodeLabel).join(', ');
    modal.confirm({
      title: lGet('models.engines.relayConfirmTitle'),
      content: String(
        lGet('models.engines.relayConfirmContent', undefined, {
          nodes: names,
        }),
      ),
      okText: lGet('models.engines.relayConfirmOk'),
      cancelText: lGet('models.engines.cancel'),
      onOk: async () => {
        setSubmitting(true);
        try {
          if (directTargets.length) {
            const ok = await startDirectPulls(directTargets);
            if (!ok) return;
          }
          const okRelay = await startRelayPull(
            unreachableWorkers,
            keepSourceOnRelay,
          );
          if (!okRelay) return;
          message.success(
            String(
              lGet('models.engines.relayStarted', undefined, {
                count: unreachableWorkers.length,
              }),
            ),
          );
          onClose();
        } finally {
          setSubmitting(false);
        }
      },
    });
  };

  const handleSubmit = async () => {
    if (!draft) return;
    if (!selected.length) {
      message.warning(lGet('models.engines.actionNeedNodes'));
      return;
    }
    setSubmitting(true);
    try {
      if (draft.mode === 'download') {
        await handleDownload();
        return;
      }

      if (draft.mode === 'delete') {
        const res = await deleteEngineImage({
          image: draft.image,
          targets: selected,
          engine: draft.engine,
          version: draft.version,
          wait: false,
        });
        if (!res?.success) {
          message.error(
            stringifyDetail(res?.data?.detail) ||
              String(lGet('models.engines.deleteFailed')),
          );
          return;
        }
        onStarted(res.data as EnginePullProgress);
        message.success(
          String(
            lGet('models.engines.deleteStarted', undefined, {
              count: selected.length,
            }),
          ),
        );
        onClose();
        return;
      }
    } finally {
      setSubmitting(false);
    }
  };

  if (draft?.mode === 'migrate') {
    const transferNodes = nodes.map((n) => {
      const has = nodeInventoryHasImage(
        localNodes,
        n.node_id,
        draft.image,
        localImages,
      );
      return {
        id: n.node_id,
        label: displayNodeName(n),
        hint: has
          ? l('models.engines.localReady')
          : l('models.engines.localMissing'),
        sourceDisabled: !has,
      };
    });
    const firstSrc = transferNodes.find((n) => !n.sourceDisabled)?.id;
    return (
      <TransferDrawer
        open={open}
        title={l('models.engines.actionMigrateTitle')}
        initialSource={firstSrc}
        initialMode="copy"
        submitting={submitting}
        warning={l('models.engines.migrateRemoveSourceWarn')}
        modeHint={{
          copy: l('models.engines.keepSourceHint'),
          migrate: l('models.engines.migrateRemoveSourceWarn'),
        }}
        subject={
          <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card shadow-card p-5 flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <StatusTag tone={engineTagTone(draft.engine)} dot={false}>
                {engineTagLabel(draft.engine)}
              </StatusTag>
              <span className="font-medium text-default text-[15px]">
                {draft.version}
              </span>
            </div>
            {draft.description ? (
              <div className="text-sm text-secondary">{draft.description}</div>
            ) : null}
            <code className="font-mono text-xs text-muted break-all">
              {draft.image}
            </code>
          </div>
        }
        nodes={transferNodes}
        onClose={onClose}
        onSubmit={async ({ source, dests, mode }) => {
          setSubmitting(true);
          try {
            for (const dest of dests) {
              const res = await migrateEngineImage({
                image: draft.image,
                source,
                dest,
                keep_source: mode === 'copy',
                engine: draft.engine,
                version: draft.version,
                wait: false,
              });
              if (!res?.success) {
                message.error(
                  stringifyDetail(res?.data?.detail) ||
                    String(lGet('models.engines.migrateFailed')),
                );
                return;
              }
              onStarted(res.data as EnginePullProgress);
            }
            message.success(
              String(
                lGet('models.engines.migrateStartedMulti', undefined, {
                  count: dests.length,
                }),
              ),
            );
            onClose();
          } finally {
            setSubmitting(false);
          }
        }}
      />
    );
  }

  const submitLabel =
    draft?.mode === 'delete'
      ? l('models.engines.confirmDelete')
      : l('models.engines.confirmDownload');

  const submitIcon =
    draft?.mode === 'delete' ? <Trash2 size={16} /> : <Download size={16} />;

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
            danger={draft?.mode === 'delete'}
            icon={submitIcon}
            loading={submitting}
            onClick={handleSubmit}
          >
            {submitLabel}
          </Button>
          <Button onClick={onClose}>{l('models.engines.cancel')}</Button>
        </Space>
      }
    >
      {draft ? (
        <div className="flex flex-col gap-4">
          <div className="rounded-lg border border-[color:var(--c-border-light)] bg-card shadow-card p-5 flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <StatusTag tone={engineTagTone(draft.engine)} dot={false}>
                {engineTagLabel(draft.engine)}
              </StatusTag>
              <span className="font-medium text-default text-[15px]">
                {draft.version}
              </span>
            </div>
            {draft.description ? (
              <div className="text-sm text-secondary">{draft.description}</div>
            ) : null}
            <code className="font-mono text-xs text-muted break-all">
              {draft.image}
            </code>
          </div>

          <div className="rounded-lg bg-[var(--c-surface-2)] px-4 py-3 flex flex-col gap-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-[12px] font-semibold text-secondary">
                {draft.mode === 'delete'
                  ? l('models.engines.deleteTargetHint')
                  : l('models.engines.pullTargetHint')}
              </span>
              <Button type="link" size="small" className="!px-0" onClick={selectAll}>
                {allTargetsSelected
                  ? l('global.actions.deselectAll')
                  : l('models.engines.selectAllNodes')}
              </Button>
            </div>
            <div className="flex flex-col gap-1.5">
              {nodes.map((n) => {
                const has = nodeInventoryHasImage(
                  localNodes,
                  n.node_id,
                  draft.image,
                  localImages,
                );
                return (
                  <label
                    key={n.node_id}
                    className="flex items-center gap-2 rounded-md border border-[color:var(--c-border-light)] bg-card px-3 py-2 cursor-pointer"
                  >
                    <Checkbox
                      checked={selected.includes(n.node_id)}
                      onChange={(e) => toggle(n.node_id, e.target.checked)}
                    />
                    <span className="text-sm text-default flex-1">
                      {displayNodeName(n)}
                    </span>
                    <span className="text-[11px] text-muted">
                      {has
                        ? l('models.engines.localReady')
                        : l('models.engines.localMissing')}
                    </span>
                  </label>
                );
              })}
            </div>
          </div>

          {draft.mode === 'delete' ? (
            <Alert
              type="warning"
              showIcon
              message={l('models.engines.deleteWarn')}
            />
          ) : null}
        </div>
      ) : null}
    </Drawer>
  );
};

export default ActionDrawer;
