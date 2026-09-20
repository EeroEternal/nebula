import { Empty, Select } from 'antd';
import React, { useEffect, useMemo, useRef } from 'react';
import { useModel, useOutletContext } from '@umijs/max';
import { RefreshCw } from 'lucide-react';
import { l, lGet } from '@/utils/intl';
import { ModelAbility } from '@/constants/modelData';
import { ActionWithTips, IconButton, SectionLoading } from '@/components';
import type { ModelsInstancesListItem } from '@/types/Public/data';
import { canChatInstance } from '@/utils/instanceAccess';
import InstanceMoreActions from '@/pages/management/instance/components/InstanceMoreActions';
import type { InstanceWorkspaceOutletContext } from '@/pages/management/instance/workspace';
import Chat from './components/ApiComponents/Chat';
import ModelAbilityCom from './components/ModelAbilityCom';
import type { ModelAbilityComRef } from './components/ModelAbilityCom';
import CompareBtn from './components/CompareBtn';

const PANEL_CLASS =
  'rounded-lg bg-[var(--c-surface)] shadow-card border border-[color:var(--c-border-light)] overflow-hidden';

const InstanceChatPanel: React.FC = () => {
  const { modelUid, instance: shellInstance, refresh: shellRefresh } =
    useOutletContext<InstanceWorkspaceOutletContext>();
  const mediaComRef = useRef<ModelAbilityComRef>(null);
  const { initialState } = useModel('@@initialState');
  const { globalReady } = initialState || {};
  const {
    initLoad,
    instanceDetail,
    loadState,
    loadError,
    replicaId,
    updateState,
  } = useModel('management.instanceDetail.model');

  const { model_ability } = instanceDetail || {};
  const hasChat = (model_ability || []).includes(ModelAbility.chat);
  const hasOCR = (model_ability || []).includes(ModelAbility.ocr);
  const loaded = loadState === 'ready';
  const canChat = loaded
    ? canChatInstance(instanceDetail as ModelsInstancesListItem)
    : modelUid && (loadState === 'missing' || loadState === 'error')
      ? false
      : shellInstance
        ? canChatInstance(shellInstance)
        : undefined;

  useEffect(() => {
    if (!globalReady) return;
    // 同 uid 已就绪时不强制 loading，减轻 Tab 切换闪白
    if (loaded && instanceDetail?.model_uid === modelUid) return;
    initLoad(modelUid);
  }, [globalReady, modelUid]);

  const handleRefresh = () => {
    if (modelUid) initLoad(modelUid);
    shellRefresh();
  };

  const headerUid = modelUid || instanceDetail?.model_uid || '-';
  const replicaOptions = useMemo(() => {
    const src = instanceDetail?.replica_data_source || [];
    return src
      .map((r) => {
        const uid = String(r?.replica_model_uid || '');
        if (!uid) return null;
        const st = String(r?.replica_status || 'UNKNOWN').toUpperCase();
        const stLabel = lGet(`models.instances.replicaDetail.status.${st}`, st);
        return { value: uid, label: `${uid} (${stLabel})` };
      })
      .filter((o): o is { value: string; label: string } => !!o);
  }, [instanceDetail?.replica_data_source]);

  return (
    <div className={PANEL_CLASS}>
      <div className="p-6 flex items-start justify-between border-b gap-3">
        <div className="min-w-0">
          <h3 className="font-semibold tracking-tight text-lg truncate">{headerUid}</h3>
        </div>
        <div className="flex flex-wrap items-center gap-2 shrink-0 justify-end">
          {replicaOptions.length ? (
            <Select
              showSearch
              optionFilterProp="label"
              className="min-w-[220px] max-w-[360px]"
              placeholder={l('models.instances.replicaDetail')}
              options={replicaOptions}
              value={replicaId || undefined}
              onChange={(v) => updateState({ replicaId: v })}
            />
          ) : null}
          {hasOCR && canChat ? <CompareBtn key="compare" /> : null}
          <ActionWithTips title={l('global.actions.refresh')}>
            <IconButton
              className="!h-8 !w-8 hover:text-primary hover:bg-primary/15"
              onClick={handleRefresh}
              aria-label={l('global.actions.refresh')}
            >
              <RefreshCw size={18} />
            </IconButton>
          </ActionWithTips>
          {(loaded && instanceDetail?.model_uid) || shellInstance?.model_uid ? (
            <InstanceMoreActions
              data={
                (loaded
                  ? instanceDetail
                  : shellInstance) as ModelsInstancesListItem
              }
              onRefresh={handleRefresh}
            />
          ) : null}
        </div>
      </div>
      <div className="p-6">
        {loadState === 'missing' || loadState === 'error' ? (
          <Empty
            description={
              loadState === 'missing'
                ? l('models.instances.instanceMissing')
                : loadError ||
                  lGet('models.instances.chat.loadFailed')
            }
          />
        ) : !loaded ? (
          <SectionLoading />
        ) : !canChat ? (
          <Empty
            description={
              instanceDetail?.status === 'ERROR' || instanceDetail?.error_info
                ? lGet('models.instances.chat.disabledErrorHint',
                  )
                : lGet('models.instances.chat.disabledHint',
                  )
            }
          />
        ) : hasChat ? (
          <Chat />
        ) : (
          <ModelAbilityCom ref={mediaComRef} />
        )}
      </div>
    </div>
  );
};

export default InstanceChatPanel;
