import { ProLayoutProps } from '@ant-design/pro-components';

/** ProLayout token（layout.pure=true 时主壳由 MainLayout 接管；保留兼容） */
const proLayoutTheme: ProLayoutProps = {
  navTheme: 'realDark',
  layout: 'mix',
  contentWidth: 'Fluid',
  fixedHeader: false,
  fixSiderbar: true,
  colorWeak: false,
  iconfontUrl: '',
  menu: {
    locale: true,
  },
  token: {
    header: {
      colorBgHeader: 'var(--c-surface)',
      colorHeaderTitle: 'var(--c-ink)',
      colorTextMenu: 'var(--c-ink-2)',
      colorTextMenuSecondary: 'var(--c-ink-3)',
      colorTextMenuSelected: 'var(--c-primary)',
      colorBgMenuItemSelected: 'var(--c-primary-light)',
      colorTextMenuActive: 'var(--c-primary)',
      colorTextRightActionsItem: 'var(--c-ink-2)',
      heightLayoutHeader: 52,
    },
    sider: {
      colorMenuBackground: 'var(--c-sidebar-bg)',
      colorTextMenuTitle: 'var(--c-ink)',
      colorMenuItemDivider: 'var(--c-sidebar-divider)',
      colorTextMenu: 'var(--c-sidebar-text)',
      colorTextMenuSelected: 'var(--c-sidebar-active-text)',
      colorTextMenuActive: 'var(--c-sidebar-active-text)',
      colorBgMenuItemSelected: 'var(--c-sidebar-active-bg)',
      colorBgMenuItemHover: 'var(--c-sidebar-hover)',
    },
    pageContainer: {
      paddingBlockPageContainerContent: 20,
      paddingInlinePageContainerContent: 24,
    },
  },
  splitMenus: false,
  siderMenuType: 'sub',
};
export default proLayoutTheme;
