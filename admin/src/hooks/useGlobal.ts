import { useEffect } from 'react';
import { useModel } from '@umijs/max';
import { get } from 'lodash';

import type { ModelInitialState } from '@/types/global';
import request from '@/utils/request';

const useGlobal = () => {
  const { setInitialState } = useModel('@@initialState');

  useEffect(() => {
    async function load() {
      try {
        const [globalRes, supervisorTransferStatusRes, versionRes, statusRes] =
          await Promise.all([
            request('/setting/global'),
            request('/supervisor_transfer_status'),
            request('/cluster/version'),
            request('/status').catch(() => undefined),
          ]);

        const licenseStatus = get(statusRes, ['success'], false)
          ? get(statusRes, ['data', 'license'], undefined)
          : undefined;

        setInitialState(
          (prev) =>
            ({
              ...prev,
              globalReady: true,
              globalConfig: get(globalRes, ['data', 'data'], {}),
              showSupervisorChangeTips: get(
                supervisorTransferStatusRes,
                ['data', 'data', 'has_transfered'],
                false,
              ),
              supervisorTransferTime: get(
                supervisorTransferStatusRes,
                ['data', 'data', 'transfer_time'],
                '',
              ),
              licenseStatus,
              version:
                get(versionRes, ['data', 'display']) ||
                get(versionRes, ['data', 'version'], ''),
              versionRevision: get(versionRes, ['data', 'revision'], ''),
            } as ModelInitialState),
        );
      } catch {
        setInitialState(
          (prev) =>
            ({
              ...prev,
              globalReady: true,
              globalConfig: {},
              showSupervisorChangeTips: false,
              licenseStatus: undefined,
            } as ModelInitialState),
        );
      }
    }
    load();
  }, []);
};

export default useGlobal;
