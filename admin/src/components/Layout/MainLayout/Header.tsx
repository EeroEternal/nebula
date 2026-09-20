import {
  PanelLeft,
  Settings,
  Bell,
  User,
  Server,
  KeyRound,
  LogOut,
  Languages,
  Trash2,
} from 'lucide-react';
import { useModel, history, useSelectedRoutes, setLocale, getLocale } from '@umijs/max';
import { Badge, Dropdown, Popover, Tooltip, App } from 'antd';
import type { MenuProps } from 'antd';
import type { ModelInitialState } from '@/types/global';
import { SETTING_MODAL_TABS, LOGIN_PATH } from '@/constants';
import { IconButton } from '@/components';
import { l, lGet } from '@/utils/intl';
import { formatRelativeTime, deleteLocal, canManageSystemSettings } from '@/utils';
import { FC, MouseEvent, useMemo, useState } from 'react';
import UpdatePasswordModal from '@/components/RightContent/UpdatePasswordModal';
import request from '@/utils/request';
import {
  saveAppNotifications,
  type AppNotification,
} from '@/utils/appNotifications';

const LANGUAGES = [
  { key: 'zh-CN', label: '简体中文' },
  { key: 'zh-TW', label: '繁體中文' },
  { key: 'en-US', label: 'English' },
  { key: 'ko-KR', label: '한국어' },
  { key: 'ja-JP', label: '日本語' },
];

interface NotificationItemProps {
  title: React.ReactNode;
  timeAgo: React.ReactNode;
  desc: React.ReactNode;
  onClick?: () => void;
}
const NotificationItemNode: FC<NotificationItemProps> = ({
  title,
  timeAgo,
  desc,
  onClick,
}) => {
  return (
    <button
      type="button"
      className="w-full rounded-sm text-sm flex flex-col items-start gap-1 p-3 hover:bg-background text-left bg-transparent border-0 cursor-pointer"
      onClick={onClick}
    >
      <div className="flex items-center gap-2 w-full">
        <Badge
          status="warning"
          classNames={{ indicator: '!h-2 !w-2' }}
          text={<span className="text-sm font-medium">{title}</span>}
        />
        <span className="ml-auto text-xs text-muted">{timeAgo}</span>
      </div>
      <p className="text-xs text-muted m-0">{desc}</p>
    </button>
  );
};
const Header = () => {
  const { modal } = App.useApp();
  const { initialState, setInitialState } = useModel('@@initialState');
  const [showUpdatePasswordModal, setShowUpdatePasswordModal] = useState(false);
  const collapsed = !!initialState?.collapsed;

  const handleCollapsed = () => {
    setInitialState(
      (prev) =>
        ({
          ...prev,
          collapsed: !initialState?.collapsed,
        } as ModelInitialState),
    );
  };
  const showSettingModal = () => {
    const canSystem = canManageSystemSettings(initialState?.currentUser);
    setInitialState(
      (prev) =>
        ({
          ...prev,
          settingModalVisible: true,
          settingModalActiveTab: canSystem
            ? SETTING_MODAL_TABS.DATABASE
            : SETTING_MODAL_TABS.MODEL_HUB,
        } as ModelInitialState),
    );
  };
  const handleChangeLanguages: MenuProps['onClick'] = async ({ key }) => {
    await request('/user/me', {
      method: 'PUT',
      data: { locale: key },
    });
    setLocale(key);
  };
  const closeSupervisorChange = async () => {
    const res = await request('/supervisor_transfer_status', { method: 'put' });
    if (res.success) {
      setInitialState(
        (prev) =>
          ({
            ...prev,
            showSupervisorChangeTips: false,
            supervisorTransferTime: undefined,
          } as ModelInitialState),
      );
    }
  };

  const appNotifications = initialState?.appNotifications || [];

  const openDeployReadyPrompt = (n: AppNotification) => {
    if (!n.modelUid) return;
    setInitialState(
      (prev) =>
        ({
          ...prev,
          deployReadyPrompt: {
            modelUid: n.modelUid!,
            modelName: n.modelName,
          },
        } as ModelInitialState),
    );
  };

  const clearAllNotifications = (e?: MouseEvent) => {
    e?.preventDefault();
    e?.stopPropagation();
    modal.confirm({
      title: lGet('global.notification.clearAllConfirm'),
      okText: lGet('global.yes'),
      cancelText: lGet('global.no'),
      getContainer: () => document.body,
      onOk: () => {
        saveAppNotifications([]);
        setInitialState(
          (prev) =>
            ({
              ...prev,
              appNotifications: [],
              showSupervisorChangeTips: false,
              supervisorTransferTime: undefined,
            } as ModelInitialState),
        );
      },
    });
  };

  const notificationItems = useMemo(() => {
    const items: {
      key: string;
      title: React.ReactNode;
      timeAgo: React.ReactNode;
      desc: React.ReactNode;
      onClick?: () => void;
    }[] = [];

    if (initialState?.showSupervisorChangeTips) {
      items.push({
        key: 'supervisor_change',
        title: l('global.waringTips'),
        timeAgo: formatRelativeTime(initialState?.supervisorTransferTime),
        desc: (
          <div>
            {l('global.message.supervisorChange', undefined, {
              time: initialState?.supervisorTransferTime || '',
            })}
            <button
              type="button"
              className="underline ml-3 text-primary hover:text-primary/60 cursor-pointer bg-transparent border-0 p-0"
              onClick={(e) => {
                e.stopPropagation();
                void closeSupervisorChange();
              }}
            >
              {l('global.actions.iSee')}
            </button>
          </div>
        ),
      });
    }

    for (const n of appNotifications) {
      items.push({
        key: n.id,
        title: n.title,
        timeAgo: formatRelativeTime(n.createdAt),
        desc: n.desc || n.modelUid || '',
        onClick:
          n.type === 'deploy_ready'
            ? () => openDeployReadyPrompt(n)
            : undefined,
      });
    }
    return items;
  }, [
    initialState?.showSupervisorChangeTips,
    initialState?.supervisorTransferTime,
    appNotifications,
  ]);

  const hasNotifications =
    notificationItems.length > 0 || !!initialState?.showSupervisorChangeTips;

  const handleLogOut = async () => {
    await request('/user/logout', { method: 'POST' });
    history.replace(LOGIN_PATH);
    setInitialState(
      (s) => ({ ...s, currentUser: undefined, globalConfig: {} } as ModelInitialState),
    );
    deleteLocal('user');
  };

  const sidebarExpanded = !collapsed;
  const collapseLabel = sidebarExpanded
    ? l('global.actions.collapseSidebar')
    : l('global.actions.expandSidebar');

  const selectedRoutes = useSelectedRoutes();
  const breadcrumbItems = useMemo(() => {
    const nameChain: string[] = [];
    const items: { key: string; label: string }[] = [];
    for (const match of selectedRoutes) {
      const route = match.route as {
        name?: string;
        path?: string;
        hideInBreadcrumb?: boolean;
        locale?: string;
      };
      if (!route?.name || route.hideInBreadcrumb) continue;
      nameChain.push(route.name);
      const localeKey = route.locale || `menu.${nameChain.join('.')}`;
      const label = lGet(localeKey) || route.name;
      if (!label || items.some((i) => i.key === localeKey)) continue;
      items.push({ key: localeKey, label });
    }
    return items;
  }, [selectedRoutes]);

  return (
    <header className="flex h-[52px] shrink-0 items-center justify-between gap-2 border-b border-[var(--c-border)] bg-[var(--c-surface)] px-4">
      <div className="flex min-w-0 items-center gap-2">
        <IconButton
          onClick={handleCollapsed}
          className="-ml-1 shrink-0"
          aria-label={collapseLabel}
          aria-expanded={sidebarExpanded}
          aria-controls="main-sidebar"
        >
          <PanelLeft size={16} aria-hidden />
        </IconButton>
        {breadcrumbItems.length > 0 ? (
          <nav
            className="flex min-w-0 items-center gap-2 overflow-x-auto text-[13px] text-[color:var(--c-ink-3)]"
            aria-label="breadcrumb"
          >
            {breadcrumbItems.map((item, index) => {
              const isLast = index === breadcrumbItems.length - 1;
              return (
                <span key={`${item.key}-${index}`} className="inline-flex items-center gap-2 shrink-0">
                  {index > 0 ? <span aria-hidden>/</span> : null}
                  <span
                    className={
                      isLast
                        ? 'font-semibold text-[color:var(--c-ink)]'
                        : 'text-[color:var(--c-ink-3)]'
                    }
                  >
                    {item.label}
                  </span>
                </span>
              );
            })}
          </nav>
        ) : null}
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <Dropdown
          menu={{
            items: LANGUAGES,
            selectedKeys: [getLocale()],
            onClick: handleChangeLanguages,
          }}
          placement="bottomRight"
          overlayClassName="w-[128px]"
        >
          <IconButton aria-label="Language">
            <Languages size={16} aria-hidden />
          </IconButton>
        </Dropdown>
        {initialState?.currentUser ? (
          <IconButton onClick={showSettingModal} aria-label={l('global.setting')}>
            <Settings size={16} aria-hidden />
          </IconButton>
        ) : null}
        <Popover
          trigger="click"
          classNames={{ body: 'w-80 !p-1 !rounded-md border !shadow-pop' }}
          content={
            <div className="text-default">
              <div className="px-2 py-1.5 text-sm font-semibold flex items-center justify-between gap-2">
                <span>{l('global.notification')}</span>
                {hasNotifications ? (
                  <Tooltip title={l('global.notification.clearAll')}>
                    <IconButton
                      className="!w-7 !h-7"
                      aria-label={l('global.notification.clearAll')}
                      onClick={clearAllNotifications}
                    >
                      <Trash2 size={14} />
                    </IconButton>
                  </Tooltip>
                ) : null}
              </div>
              <div className="-mx-1 my-1 h-px bg-background-muted" />
              <div className="max-h-[300px] overflow-y-auto">
                {notificationItems.length ? (
                  notificationItems.map((item) => (
                    <NotificationItemNode
                      key={item.key}
                      title={item.title}
                      timeAgo={item.timeAgo}
                      desc={item.desc}
                      onClick={item.onClick}
                    />
                  ))
                ) : (
                  <div className="flex flex-col items-center justify-center py-10 text-muted">
                    <Server className="h-10 w-10 mb-3 opacity-50" aria-hidden />
                    <p className="text-sm">{l('global.data.empty')}</p>
                  </div>
                )}
              </div>
            </div>
          }
          arrow={false}
          placement="bottomRight"
        >
          <IconButton className="relative" aria-label={l('global.notification')}>
            <Bell size={16} aria-hidden />
            {hasNotifications && (
              <span className="absolute top-1.5 right-1.5 h-2 w-2 rounded-full bg-red-500" />
            )}
          </IconButton>
        </Popover>

        <Popover
          classNames={{ body: 'w-56 !p-1 !rounded-md border shadow-pop' }}
          content={
            <div className="text-default ">
              <div className="px-2 py-1.5 text-sm font-semibold">
                <div className="flex flex-col gap-1">
                  <span className="text-sm font-medium">
                    {initialState?.currentUser?.role === 'administrator'
                      ? l('global.administrator')
                      : initialState?.currentUser?.role}
                  </span>
                  <span className="text-xs text-muted font-normal">
                    {initialState?.currentUser?.name}
                  </span>
                </div>
              </div>
              <div className="-mx-1 my-1 h-px bg-background-muted" />
              <button
                type="button"
                className="w-full flex items-center gap-2 cursor-pointer px-2 py-1.5 rounded-sm hover:bg-background-focused text-left bg-transparent border-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
                onClick={() => setShowUpdatePasswordModal(true)}
              >
                <KeyRound size={16} aria-hidden />
                {l('global.login.changePassword')}
              </button>
              <div className="-mx-1 my-1 h-px bg-background-muted" />
              <button
                type="button"
                className="w-full flex items-center gap-2 cursor-pointer px-2 py-1.5 rounded-sm hover:bg-background-focused text-danger text-left bg-transparent border-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
                onClick={handleLogOut}
              >
                <LogOut size={16} aria-hidden />
                {l('global.login.exit')}
              </button>
            </div>
          }
          arrow={false}
          placement="bottomRight"
        >
          <IconButton aria-label={l('global.administrator')}>
            <User size={16} aria-hidden />
          </IconButton>
        </Popover>
      </div>
      {showUpdatePasswordModal && (
        <UpdatePasswordModal
          visible={showUpdatePasswordModal}
          handleCancel={() => setShowUpdatePasswordModal(false)}
        />
      )}
    </header>
  );
};
export default Header;
