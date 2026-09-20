import { useEffect } from 'react';
import { useModel, history } from '@umijs/max';
import { message } from 'antd';
import eventBus from '@/utils/eventBus';
import type {
  ShowModalHandler,
  ShowNotificationHandler,
  RedirectHandler,
  ShowMessageHandler,
} from '@/utils/eventBus';
import { ModelInitialState } from '@/types/global';
import { deleteLocal, canManageSystemSettings } from '@/utils';
import { LOGIN_PATH } from '@/constants';
import { notifyErrorOnce } from '@/utils/notifyOnce';

const useRequestError = () => {
  const { initialState, setInitialState } = useModel('@@initialState');

  useEffect(() => {
    const showModal: ShowModalHandler = (data) => {
      if (data.modalName === 'licenseModal') {
        if (!canManageSystemSettings(initialState?.currentUser)) {
          return;
        }
        setInitialState(
          (prev) => ({ ...prev, licenseModalVisible: data.visible } as ModelInitialState),
        );
      }
    };
    const showNotification: ShowNotificationHandler = (data) => {
      notifyErrorOnce(data);
    };
    const redirect: RedirectHandler = (data) => {
      if (data.path === LOGIN_PATH) {
        // 重定向至登录页面需删除浏览器缓存的user
        deleteLocal('user');
      }
      if (data.message) {
        message.error(data.message);
      }
      if (history.location.pathname === LOGIN_PATH) {
        return;
      }
      history.push(data.path);
    };
    const showMessage: ShowMessageHandler = (data) => {
      const text = data?.message;
      if (!text) return;
      const kind = data?.type || 'warning';
      if (kind === 'error') message.error(text);
      else if (kind === 'success') message.success(text);
      else if (kind === 'info') message.info(text);
      else message.warning(text);
    };
    eventBus.on('showModal', showModal);
    eventBus.on('redirect', redirect);
    eventBus.on('notification', showNotification);
    eventBus.on('showMessage', showMessage);
    return () => {
      eventBus.off('showModal', showModal);
      eventBus.off('notification', showNotification);
      eventBus.off('redirect', redirect);
      eventBus.off('showMessage', showMessage);
    };
  }, [initialState?.currentUser, setInitialState]);
};
export default useRequestError;
