import { useEffect, useRef } from 'react';
import { App } from 'antd';
import { useModel } from '@umijs/max';

import type { ModelInitialState } from '@/types/global';
import { listEngineNodes } from '@/services/engineImages';
import { l, lGet } from '@/utils/intl';
import {
  loadAppNotifications,
  makeRuntimeMismatchNotification,
  saveAppNotifications,
} from '@/utils/appNotifications';

const SESSION_KEY = 'powerllm.runtimeMismatch.modal';

function formatMismatchDesc(
  runtimes?: Array<{ label?: string; node_id?: string; runtime?: string }>,
): string {
  const parts = (runtimes || [])
    .map((n) => {
      const name = n.label || n.node_id || '';
      const rt = n.runtime || '';
      if (!name || !rt) return '';
      return `${name}=${rt}`;
    })
    .filter(Boolean);
  return parts.join(', ');
}

/**
 * Supervisor/Worker docker vs k8s mix: one modal per session + persist to bell.
 */
export function useEngineRuntimeMismatchWatch() {
  const { modal } = App.useApp();
  const { setInitialState } = useModel('@@initialState');
  const ranRef = useRef(false);

  useEffect(() => {
    if (ranRef.current) return;
    ranRef.current = true;
    let cancelled = false;

    (async () => {
      try {
        const res = await listEngineNodes();
        if (cancelled || !res?.success) return;
        const data = res.data;
        if (!data?.runtime_mismatch) return;
        const detail = formatMismatchDesc(data.runtimes);
        const title = String(
          lGet('global.runtimeMismatch.title'),
        );
        const desc = detail
          ? String(
              lGet('global.runtimeMismatch.desc', undefined, { nodes: detail }),
            )
          : String(lGet('global.runtimeMismatch.descFallback'));
        const note = makeRuntimeMismatchNotification(title, desc);
        const prev = loadAppNotifications().filter(
          (n) => n.type !== 'runtime_mismatch',
        );
        const list = [note, ...prev].slice(0, 100);
        saveAppNotifications(list);
        setInitialState(
          (s) => ({ ...s, appNotifications: list } as ModelInitialState),
        );
        if (sessionStorage.getItem(SESSION_KEY) === desc) return;
        sessionStorage.setItem(SESSION_KEY, desc);
        modal.warning({
          title,
          content: desc,
          okText: String(l('global.actions.iSee')),
        });
      } catch {
        /* ignore: nodes API optional on login */
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [modal, setInitialState]);
}
