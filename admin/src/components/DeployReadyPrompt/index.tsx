import { FC, useEffect, useRef } from 'react';
import { App } from 'antd';
import { history, useModel } from '@umijs/max';

import type { ModelInitialState } from '@/types/global';
import { lGet } from '@/utils/intl';

/** Global prompt host: 模型部署成功，是否前往预热（App.modal 更可靠） */
const DeployReadyPrompt: FC = () => {
  const { modal } = App.useApp();
  const { initialState, setInitialState } = useModel('@@initialState');
  const prompt = initialState?.deployReadyPrompt;
  const openKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (!prompt?.modelUid) {
      openKeyRef.current = null;
      return;
    }
    const key = `${prompt.modelUid}:${prompt.modelName || ''}`;
    if (openKeyRef.current === key) return;
    openKeyRef.current = key;

    const uid = prompt.modelUid;
    const name = prompt.modelName;
    modal.confirm({
      title: lGet('models.instances.deployReady.title'),
      content: (
        <div>
          <p className="m-0 text-sm">
            {lGet('models.instances.deployReady.content',
            )}
          </p>
          {(name || uid) && (
            <p className="mt-2 mb-0 text-xs text-muted font-mono">
              {name || uid}
              {name && uid ? ` · ${uid}` : ''}
            </p>
          )}
        </div>
      ),
      okText: lGet('global.yes'),
      cancelText: lGet('global.no'),
      centered: true,
      zIndex: 1100,
      onOk: () => {
        setInitialState(
          (prev) =>
            ({
              ...prev,
              deployReadyPrompt: null,
            } as ModelInitialState),
        );
        history.push(`/models/instances/detail?id=${encodeURIComponent(uid)}`);
      },
      onCancel: () => {
        setInitialState(
          (prev) =>
            ({
              ...prev,
              deployReadyPrompt: null,
            } as ModelInitialState),
        );
      },
      afterClose: () => {
        if (openKeyRef.current === key) openKeyRef.current = null;
      },
    });
  }, [prompt, modal, setInitialState]);

  return null;
};

export default DeployReadyPrompt;
