import { ModelInitialState } from '@/types/global';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Alert } from 'antd';

const HeaderContent = () => {
  const { initialState, setInitialState } = useModel('@@initialState');
  const { showSupervisorChangeTips, supervisorTransferTime } = initialState || {};
  const { run: closeSupervisorChangeTips } = useRequest(
    () => request('/supervisor_transfer_status', { method: 'PUT' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          setInitialState(
            (prev) => ({ ...prev, showSupervisorChangeTips: false } as ModelInitialState),
          );
        }
      },
    },
  );

  if (showSupervisorChangeTips) {
    return (
      <Alert
        message={l('global.message.supervisorChangeMsg', undefined, {
          time: supervisorTransferTime || '',
        })}
        type="warning"
        showIcon
        closable
        onClose={closeSupervisorChangeTips}
      />
    );
  }
  return null;
};
export default HeaderContent;
