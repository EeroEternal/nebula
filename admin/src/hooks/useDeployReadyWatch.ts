import { useEffect, useRef } from 'react';
import { useModel } from '@umijs/max';

import type { ModelInitialState } from '@/types/global';
import { IntanceStatus } from '@/constants/intance';
import {
  hasSeenDeployReady,
  isPendingDeployReady,
  loadAppNotifications,
  makeDeployReadyNotification,
  markPendingDeployReady,
  markSeenDeployReady,
  saveAppNotifications,
} from '@/utils/appNotifications';

type ReadyWatchItem = {
  model_uid: string;
  model_name?: string;
  status?: string;
};

const LAUNCHING = new Set<string>([
  IntanceStatus.CREATING,
  IntanceStatus.UPDATING,
]);

/**
 * Detect launch → READY; push notification + open deploy-ready prompt.
 * Uses session pending markers so logs page / remount still fires.
 */
export function useDeployReadyWatch(items: ReadyWatchItem[] | undefined) {
  const { setInitialState } = useModel('@@initialState');
  const prevStatusRef = useRef<Record<string, string>>({});

  useEffect(() => {
    if (!items?.length) return;
    const prev = prevStatusRef.current;
    const nextMap: Record<string, string> = { ...prev };

    for (const item of items) {
      const uid = item.model_uid;
      if (!uid) continue;
      const status = String(item.status || '');
      const was = prev[uid];
      nextMap[uid] = status;

      if (LAUNCHING.has(status)) {
        markPendingDeployReady(uid);
      }

      const edgeFromMemory =
        Boolean(was) && was !== IntanceStatus.READY && status === IntanceStatus.READY;
      const edgeFromPending =
        status === IntanceStatus.READY && isPendingDeployReady(uid);

      if ((edgeFromMemory || edgeFromPending) && !hasSeenDeployReady(uid)) {
        markSeenDeployReady(uid);
        const note = makeDeployReadyNotification(uid, item.model_name);
        // Dedupe same uid entries already in the tray
        const prevNotes = loadAppNotifications().filter(
          (n) => !(n.type === 'deploy_ready' && n.modelUid === uid),
        );
        const list = [note, ...prevNotes].slice(0, 100);
        saveAppNotifications(list);
        setInitialState(
          (s) =>
            ({
              ...s,
              appNotifications: list,
              deployReadyPrompt: {
                modelUid: uid,
                modelName: item.model_name,
              },
            } as ModelInitialState),
        );
      }
    }

    prevStatusRef.current = nextMap;
  }, [items, setInitialState]);
}
