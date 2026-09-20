import { FC, PropsWithChildren } from 'react';
import { useModel, useLocation } from '@umijs/max';
import { Spin, Skeleton, App } from 'antd';
import { LicenseModal } from '@/components';
import NoPermission from '@/pages/403';
import { hasPagePermissions } from '@/utils';
import { canManageSystemSettings } from '@/utils/permissions';
import LicenseReminderBanner from '../LicenseReminderBanner';
import SettingParamModal from '../SettingParamModal';
import DeployReadyPrompt from '@/components/DeployReadyPrompt';
import Sidebar from './Sidebar';
import Header from './Header';

const MainLayout: FC<PropsWithChildren> = ({ children }) => {
  // 这里需要注意，如果使用history.location.pathname 来判断的话，若 config.js 中配置了 PREFIX_PATH 会导致一直渲染NoPermission
  // 故使用useLocation 的location.pathname，可以规避这个问题(router中的原始路由)
  const location = useLocation();
  const { initialState } = useModel('@@initialState');
  const pagePermissions = initialState?.currentUser?.permissions?.page || {};
  return (
    <App>
      <Spin
        spinning={!initialState?.globalReady}
        tip="Loading..."
        className="!w-screen !h-screen !max-h-none "
      >
        <div className="flex min-h-screen w-full overflow-x-hidden">
          <Sidebar />
          <div className="relative min-h-screen overflow-x-hidden flex-1 bg-[var(--c-bg)] flex flex-col">
            <Header />
            <LicenseReminderBanner />
            <main className="flex-1 overflow-x-auto overflow-y-auto bg-[var(--c-bg)]">
              {!initialState?.globalReady ? (
                <Skeleton active className="py-5 px-6" />
              ) : (
                <div className="mx-auto max-w-[var(--content-max-width)] py-5 px-6">
                  {hasPagePermissions(location.pathname, pagePermissions) ? (
                    children
                  ) : (
                    <NoPermission />
                  )}
                </div>
              )}
            </main>
          </div>
          {/* 更新证书 */}
          {initialState?.licenseModalVisible &&
            canManageSystemSettings(initialState?.currentUser) && <LicenseModal />}
          {/* 设置弹窗 */}
          {initialState?.settingModalVisible && <SettingParamModal />}
          <DeployReadyPrompt />
        </div>
      </Spin>
    </App>
  );
};
export default MainLayout;
