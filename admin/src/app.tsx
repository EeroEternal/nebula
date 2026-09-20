import React from 'react';
import { isEmpty, sortBy } from 'lodash';
import { App as AntdApp, ConfigProvider } from 'antd';

import { AvatarDropdown, AvatarName, Footer } from '@/components';
import type { Settings as LayoutSettings, MenuDataItem } from '@ant-design/pro-components';
import { SettingOutlined } from '@ant-design/icons';
import type { RunTimeLayoutConfig } from '@umijs/max';
import { SelectLang, history, setLocale, getLocale } from '@umijs/max';
import { changeUserConfig } from '@/services/user';
import {
  LANGUAGE_PRIORITY,
  ROUTER_ACCESS_MAP,
  LOGIN_PATH,
  COLLAPSE_SIDER,
  SETTING_MODAL_TABS,
} from '@/constants';
import { ModelInitialState } from '@/types/global';
import { getLocal, canManageSystemSettings } from '@/utils';
import useRequestError from '@/hooks/useRequestError';
import useGlobal from '@/hooks/useGlobal';
import { useEngineRuntimeMismatchWatch } from '@/hooks/useEngineRuntimeMismatchWatch';
import { HeaderContent, MainLayout } from '@/components/Layout';
import proLayoutTheme from '@/utils/theme';
import RegionSwitcher from '@/components/RegionSwitcher';
import { isRegionModeEnabled } from '@/utils/region';
import antTheme from '@/utils/antTheme';
import { loadAppNotifications } from '@/utils/appNotifications';

export async function getInitialState(): Promise<ModelInitialState> {
  localStorage.removeItem('isLoading');
  const currentUser = getLocal('user') || {};
  // 这里废弃掉在getInitialState中请求全局接口是因为，如果global请求时间过长，页面一直白屏，但是如果废弃掉此处代码，页面的路由权限也就无法使用框架自带的access.ts
  // 白屏原因：getInitialState 执行时 layout 都还没挂载上来，所以 childrenRender 也就无法执行，所以在childrenRender中加Spin的方案行不通
  // const globalConfig = await getGlobalConfig()
  return {
    currentUser,
    globalConfig: {},
    loading: false,
    globalReady: false,
    settings: proLayoutTheme as LayoutSettings,
    licenseModalVisible: false,
    showSupervisorChangeTips: false,
    licenseStatus: undefined,
    // 移动端默认收起为抽屉关闭；桌面默认展开（可用 window.DEFAULT_SIDER_COLLAPSED 覆盖）
    collapsed:
      window.DEFAULT_SIDER_COLLAPSED ??
      (typeof window !== 'undefined' && window.innerWidth < 768),
    settingModalVisible: false,
    settingModalActiveTab: SETTING_MODAL_TABS.DATABASE,
    appNotifications: loadAppNotifications(),
    deployReadyPrompt: null,
  };
}
// 处理请求报错、全局初始化放到 React 组件中执行，避免在 layout 函数里使用 Hooks
// 在 childrenRender 中挂载，执行全局副作用 Hooks
const LayoutBoot: React.FC = () => {
  useRequestError();
  useGlobal();
  useEngineRuntimeMismatchWatch();
  return null;
};

export function rootContainer(container: React.ReactNode) {
  const theme = {
    ...antTheme,
    token: {
      ...antTheme.token,
      // 运行时可被 window.THEME_PRIMARY_COLOR 覆盖；规范见仓库根 design.md
      colorPrimary: window.THEME_PRIMARY_COLOR || 'var(--c-primary)',
    },
  };
  return (
    <ConfigProvider theme={theme}>
      <AntdApp>{container}</AntdApp>
    </ConfigProvider>
  );
}

let hasHandledFirstCollapse = false;
// ProLayout 支持的api https://procomponents.ant.design/components/layout
export const layout: RunTimeLayoutConfig = ({ initialState, setInitialState }) => {
  const language = getLocale();
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
  const changeLanguage = async (lang: { key: string }) => {
    await changeUserConfig({ locale: lang.key });
    // 切换刷新页面
    setLocale(lang.key);
  };
  // 左侧菜单不按页面权限隐藏；无权限时由右侧内容区展示引导
  const filterMenuByAuth = (menuData: MenuDataItem[]): MenuDataItem[] => {
    return menuData
      .map((item) => {
        if (item.children) {
          const filteredChildren = filterMenuByAuth(item.children);
          if (filteredChildren.length > 0) {
            return { ...item, children: filteredChildren };
          }
          return null;
        }
        // 左侧菜单不按页面权限隐藏；无权限时由右侧内容区展示引导
        // 单 region / 未接入 Console 时隐藏“全部地区”与全局 Console 菜单
        if (
          (item.path === '/regions' || item.path === '/console') &&
          !isRegionModeEnabled()
        ) {
          return null;
        }
        // langfuse权限
        const match = ROUTER_ACCESS_MAP.find((sub) => (item.path || '').startsWith(sub.pathPrefix));
        if (match && !initialState?.globalConfig?.[match.accessKey]) {
          return null;
        }
        return item;
      })
      .filter(Boolean) as MenuDataItem[];
  };

  return {
    ...initialState?.settings,
    // colorPrimary: window.THEME_PRIMARY_COLOR,
    pure: true,
    postMenuData: (menuData) => filterMenuByAuth(menuData || []),
    headerContentRender: () => <HeaderContent />,
    actionsRender: () => [
      <RegionSwitcher key="region" />,
      ...(initialState?.currentUser
        ? [
            <SettingOutlined
              key="setting"
              className="text-[18px] p-[12px]"
              onClick={showSettingModal}
            />,
          ]
        : []),
      <SelectLang
        postLocalesData={(list) =>
          sortBy(
            list,
            (item) => LANGUAGE_PRIORITY[item.lang as keyof typeof LANGUAGE_PRIORITY] || 5,
          )
        }
        key={language}
        onItemClick={changeLanguage}
      />,
    ],
    avatarProps: {
      src: initialState?.currentUser?.avatar,
      title: <AvatarName />,
      render: (_, avatarChildren) => {
        return <AvatarDropdown>{avatarChildren}</AvatarDropdown>;
      },
    },
    // breakpoint: false,
    // waterMarkProps: false,
    footerRender: () => <Footer />,
    onPageChange: () => {
      const pathname = history.location.pathname;
      // 如果没有登录，重定向到 login
      if (isEmpty(initialState?.currentUser) && pathname !== LOGIN_PATH) {
        history.push(LOGIN_PATH);
      }
      if (COLLAPSE_SIDER.includes(pathname) && initialState?.collapsed === false) {
        setInitialState(
          (prev) =>
            ({
              ...prev,
              collapsed: true,
            } as ModelInitialState),
        );
      }
    },
    bgLayoutImgList: [],
    links: [],
    menuHeaderRender: undefined,
    // 自定义 403 页面
    // unAccessible: <div>unAccessible</div>,
    childrenRender: (children) => (
      <>
        <LayoutBoot />
        <MainLayout>{children}</MainLayout>
      </>
    ),
    collapsed: initialState?.collapsed,
    onCollapse: (collapsed) => {
      // ProLayout 在初始化时会自动触发一次 onCollapse
      if (!hasHandledFirstCollapse && window.DEFAULT_SIDER_COLLAPSED) {
        hasHandledFirstCollapse = true;
        return;
      }
      setInitialState((prev) => ({ ...prev, collapsed } as ModelInitialState));
    },
    logo: (
      <img
        src={window.LOGO_BLUE || window.LOGO || '/logo-blue.png'}
        alt="logo"
      />
    ),
    title: window.TITLE || '',
    menuFooterRender: () => {
      if (initialState?.collapsed || !initialState?.version) {
        return null;
      }
      return (
        <div className="truncate text-right" title={initialState?.versionRevision || ''}>
          {initialState?.version}
        </div>
      );
    },
  };
};

// src/app.ts
export function modifyClientRenderOpts(opts: {
  runtimePublicPath?: string;
  basename?: string;
  history: {
    push: (...args: unknown[]) => unknown;
    replace: (...args: unknown[]) => unknown;
  };
}) {
  // 此处的window.PREFIX_PATH 已在 global 中处理过（斜杠）
  const prefixPath = window.PREFIX_PATH;
  const finalPrefixPath = prefixPath ? `/${prefixPath}/` : '/';

  opts.runtimePublicPath = finalPrefixPath;
  opts.basename = finalPrefixPath;

  if (prefixPath && window.location.pathname === '/') {
    const { history } = opts;
    history.replace(`/${prefixPath}/dashboard`);
  }
  const { history } = opts;
  const originPush = history.push;
  const originReplace = history.replace;

  const processPath = (path: string | { pathname: string }): string | { pathname: string } => {
    // 如果没有配置前缀，忽略以下处理（即默认 '/' 根路径的情况）
    if (!prefixPath) return path;

    if (typeof path === 'string') {
      if (path.startsWith(finalPrefixPath)) return path;
      return `/${prefixPath}/${path.replace(/^\/+/, '')}`;
    } else if (typeof path === 'object' && 'pathname' in path) {
      if (path.pathname.startsWith(finalPrefixPath)) return path;
      return {
        ...path,
        pathname: `/${prefixPath}/${path.pathname.replace(/^\/+/, '')}`,
      };
    } else {
      return path;
    }
  };

  history.push = (...args: unknown[]) => {
    // 仅处理第一个参数 (path)
    const processedPath = processPath(args[0] as string | { pathname: string });
    originPush.apply(history, [processedPath, ...args.slice(1)]);
  };

  // 重写 replace 方法
  history.replace = (...args: unknown[]) => {
    // 仅处理第一个参数 (path)
    const processedPath = processPath(args[0] as string | { pathname: string });
    originReplace.apply(history, [processedPath, ...args.slice(1)]);
  };

  return opts;
}
