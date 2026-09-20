import { useModel, Link } from '@umijs/max';
import { Grid } from 'antd';
import classNames from 'classnames';

import type { ModelInitialState } from '@/types/global';
import SidebarMenu from './SidebarMenu';

/** 浅色面默认品牌蓝 logo；另保留 logo-dark / logo-white */
const brandLogo = () =>
  window.LOGO_BLUE || window.LOGO || '/logo-blue.png';

const SidebarHeader = ({ iconOnly = false }: { iconOnly?: boolean }) => {
  return (
    <div
      className={classNames(
        'flex items-center shrink-0 h-16',
        iconOnly ? 'justify-center mx-2 my-2' : 'mx-4 my-0',
      )}
    >
      <Link
        to="/dashboard"
        className={classNames(
          'flex items-center overflow-hidden rounded-md bg-[var(--c-sidebar-logo-bg)]',
          iconOnly
            ? 'justify-center h-12 w-12 px-1.5'
            : 'w-full h-12 gap-x-2 px-2.5',
        )}
        aria-label={window.TITLE || 'Home'}
      >
        <img src={brandLogo()} alt="" className="h-7 w-auto shrink-0" />
        {!iconOnly && window.TITLE ? (
          <h1 className="truncate font-semibold text-[13px] leading-tight text-[color:var(--c-sidebar-active-text)]">
            {window.TITLE}
          </h1>
        ) : null}
      </Link>
    </div>
  );
};

const Sidebar = () => {
  const { initialState, setInitialState } = useModel('@@initialState');
  const screens = Grid.useBreakpoint();
  /** < md：移动端抽屉；≥ md：桌面图标栏折叠（md 未就绪时先按桌面，避免首帧抽屉闪烁） */
  const isMobile = screens.md === false;
  const collapsed = !!initialState?.collapsed;
  /** 桌面折叠：图标栏；移动端折叠：抽屉关闭 */
  const desktopIconOnly = !isMobile && collapsed;
  const mobileOpen = isMobile && !collapsed;

  const closeMobileDrawer = () => {
    if (!isMobile) return;
    setInitialState(
      (prev) =>
        ({
          ...prev,
          collapsed: true,
        } as ModelInitialState),
    );
  };

  // 桌面占位宽度：展开 full / 折叠 collapsed；移动端不占位
  const spacerWidthClass = isMobile
    ? 'w-0'
    : collapsed
      ? 'w-[var(--sidebar-width-collapsed)]'
      : 'w-[var(--sidebar-width)]';

  const panelWidthClass = isMobile
    ? 'w-[var(--sidebar-width)]'
    : collapsed
      ? 'w-[var(--sidebar-width-collapsed)]'
      : 'w-[var(--sidebar-width)]';

  const panelPositionClass = isMobile
    ? mobileOpen
      ? 'left-0'
      : 'left-[calc(var(--sidebar-width)*-1)]'
    : 'left-0';

  return (
    <>
      {/* 移动端遮罩 */}
      {mobileOpen && (
        <button
          type="button"
          className="fixed inset-0 z-[15] bg-black/45 md:hidden border-0 cursor-default"
          aria-label="Close menu"
          onClick={closeMobileDrawer}
        />
      )}
      <div
        className="text-[color:var(--c-sidebar-text)] group/sidebar overflow-hidden shrink-0"
        data-collapsed={collapsed}
        data-mobile={isMobile || undefined}
      >
        {/* 布局占位（桌面保留折叠宽度，移动端为 0） */}
        <div
          className={classNames(
            'relative min-h-screen bg-transparent transition-[width] duration-complex ease-enter',
            spacerWidthClass,
          )}
        />
        <div
          id="main-sidebar"
          className={classNames(
            'flex flex-col fixed inset-y-0 z-20 min-h-screen bg-[var(--c-sidebar-bg)] shadow-nav border-r border-[color:var(--c-sidebar-divider)] transition-[left,width] duration-complex ease-enter',
            panelWidthClass,
            panelPositionClass,
          )}
          role={isMobile ? 'dialog' : 'navigation'}
          aria-label="Sidebar"
          aria-hidden={isMobile ? !mobileOpen : undefined}
        >
          <SidebarHeader iconOnly={desktopIconOnly} />
          <div
            className="h-px mx-3 bg-[var(--c-sidebar-divider)] shrink-0"
            aria-hidden
          />
          <SidebarMenu
            iconOnly={desktopIconOnly}
            onNavigate={isMobile ? closeMobileDrawer : undefined}
          />
        </div>
      </div>
    </>
  );
};
export default Sidebar;
