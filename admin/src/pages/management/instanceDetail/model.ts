import { ModelAbility } from '@/constants/modelData';
import { InstanceDetail } from '@/types/Public/data';
import { instanceDetailPath } from '@/utils/instanceAccess';
import { readThrownMessage } from '@/utils/formatApiError';
import request from '@/utils/request';
import { useRequest, useSetState } from 'ahooks';
import { isArray } from 'lodash';

const transformersAbility = (modelAbility?: string | string[]) => {
  return modelAbility ? (isArray(modelAbility) ? modelAbility : [modelAbility]) : [];
};

/** 优先 READY 副本，否则取第一个 */
const pickReplicaId = (detail?: Partial<InstanceDetail> | null) => {
  const list = detail?.replica_data_source || [];
  const ready = list.find((r) => String(r?.replica_status || '').toUpperCase() === 'READY');
  return (ready || list[0])?.replica_model_uid || '';
};

interface IModelState {
  isStream: boolean;
  selectModelAbility: ModelAbility;
  replicaId: string;
  compareData: InstanceDetail[];
  /** idle | loading | ready | missing | error */
  loadState: 'idle' | 'loading' | 'ready' | 'missing' | 'error';
  loadError?: string;
}
export default () => {
  const [state, setState] = useSetState<IModelState>({
    isStream: true,
    selectModelAbility: '' as ModelAbility,
    replicaId: '',
    compareData: [],
    loadState: 'idle',
    loadError: undefined,
  });
  const {
    loading: instanceDetailLoading,
    runAsync,
    data,
    mutate,
  } = useRequest(
    (modelId: string) =>
      request<{ data: { data: InstanceDetail }; success: boolean }>(instanceDetailPath(modelId), {
        skipNotification: true,
      }),
    {
      manual: true,
    },
  );

  const initLoad = async (modelId: string) => {
    if (!modelId) {
      setState({
        loadState: 'missing',
        loadError: undefined,
        replicaId: '',
        compareData: [],
        selectModelAbility: '' as ModelAbility,
      });
      mutate(undefined);
      return;
    }
    // 同实例已就绪时静默刷新，避免 Tab 切到对话测试时整页 loading 闪白
    const sameReady =
      state.loadState === 'ready' && data?.data?.data?.model_uid === modelId;
    if (!sameReady) {
      setState({ loadState: 'loading', loadError: undefined });
    }
    try {
      const res = await runAsync(modelId);
      const detail = res?.data?.data as InstanceDetail | undefined;
      if (!res?.success || !detail?.model_uid) {
        const detailMsg =
          (res?.data as { detail?: string } | undefined)?.detail ||
          (res?.data as { message?: string } | undefined)?.message;
        const missing =
          String(detailMsg || '').toLowerCase().includes('not found') || !detail?.model_uid;
        setState({
          loadState: missing ? 'missing' : 'error',
          loadError: detailMsg,
          replicaId: '',
          compareData: [],
          selectModelAbility: '' as ModelAbility,
        });
        return;
      }
      const abilities = transformersAbility(detail.model_ability);
      const picked = pickReplicaId(detail);
      const stillValid =
        sameReady &&
        !!state.replicaId &&
        (detail.replica_data_source || []).some(
          (r) => r?.replica_model_uid === state.replicaId,
        );
      setState({
        loadState: 'ready',
        loadError: undefined,
        selectModelAbility: (abilities[0] || '') as ModelAbility,
        replicaId: stillValid ? state.replicaId : picked,
        compareData: [
          {
            ...detail,
            model_ability: abilities,
          } as InstanceDetail,
        ],
      });
    } catch (e: unknown) {
      setState({
        loadState: 'error',
        loadError: readThrownMessage(e) || String(e),
        replicaId: '',
        compareData: [],
        selectModelAbility: '' as ModelAbility,
      });
    }
  };
  const raw = data?.success ? data?.data?.data : undefined;
  return {
    initLoad,
    instanceDetailLoading: instanceDetailLoading || state.loadState === 'loading',
    instanceDetail: {
      ...(raw || {}),
      model_ability: transformersAbility(raw?.model_ability),
    } as InstanceDetail,
    ...state,
    updateState: setState,
  };
};
