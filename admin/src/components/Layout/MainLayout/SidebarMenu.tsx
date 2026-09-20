import {
  LayoutDashboard,
  Box,
  Database,
  Layers,
  Library,
  Settings2,
  Activity,
  Network,
  ScrollText,
  Users,
  Shield,
  Key,
  ListTodo,
  ChevronDown,
  Container,
  FlaskConical,
  Send,
  Monitor,
  Cog,
} from 'lucide-react';
import { Link, useAppData, useModel, useLocation } from '@umijs/max';
import { transformRoute } from '@umijs/route-utils';
import { Tooltip } from 'antd';
import { FC, useMemo, useState } from 'react';
import classNames from 'classnames';
import { size } from 'lodash';
import type { MenuDataItem } from '@ant-design/pro-components';
import { l } from '@/utils/intl';
import { ROUTER_ACCESS_MAP } from '@/constants';
import { isRegionModeEnabled } from '@/utils/region';
import { useConsoleReachable } from '@/hooks/useConsoleReachable';
import { useK8sRuntime } from '@/hooks/useK8sRuntime';

const MENU_ICON_MAP = {
  '/dashboard': LayoutDashboard,
  '/models/repository': Database,
  '/models/register': Box,
  '/models/catalog': Library,
  '/models/engines': Container,
  '/models/instances': Layers,
  '/tasks/finetune': Settings2,
  '/tasks/batch': ListTodo,
  '/monitor/cluster': Activity,
  '/monitor/instances': Layers,
  '/monitor/traffic': Network,
  '/monitor/logs': ScrollText,
  '/monitor/hami': Layers,
  '/admin/users': Users,
  '/admin/roles': Shield,
  '/admin/secretKey': Key,
  '/console/users': Users,
  '/console/roles': Shield,
  '/console/releases': Send,
};

/** 主分组标题小图标 */
const GROUP_ICON_MAP: Record<string, FC<{ className?: string; size?: number }>> = {
  '/models': Box,
  '/monitor': Monitor,
  '/admin': Cog,
  '/tasks': ListTodo,
  'testing-features': FlaskConical,
};

/** 收入「测试功能」折叠组的菜单 path */
const TESTING_MENU_PATHS = [
  '/console/users',
  '/console/roles',
  '/console/releases',
  '/models/catalog',
];

interface NavItemProps {
  item: MenuDataItem;
  /** 桌面折叠为图标栏时仅显示图标 */
  iconOnly?: boolean;
}

const NavItem: FC<NavItemProps> = ({ item, iconOnly = false }) => {
  const Icon = MENU_ICON_MAP[item.path as keyof typeof MENU_ICON_MAP];
  // 这里需要注意，如果使用history.location.pathname 来判断的话，若 config.js 中配置了 PREFIX_PATH 会导致激活的样式有问题
  // 故使用useLocation 的location.pathname，可以规避这个问题(router中的原始路由)
  const location = useLocation();
  const isActive = location.pathname.startsWith(item.path || '');
  const label = l(item.locale);

  const link = (
    <Link
      to={item.path as string}
      title={iconOnly ? label : undefined}
      aria-current={isActive ? 'page' : undefined}
      className={classNames(
        'shrink-0 group text-[13px] text-left px-2 rounded-md overflow-hidden h-9 flex items-center relative transition-colors duration-micro ease-default hover:bg-[var(--c-sidebar-hover)]',
        iconOnly ? 'justify-center gap-0' : 'gap-3',
        isActive
          ? 'bg-[var(--c-sidebar-active-bg)] font-medium !text-[color:var(--c-sidebar-active-text)]'
          : '!text-[color:var(--c-sidebar-text)] hover:!text-[color:var(--c-sidebar-text-hover)]',
      )}
    >
      {MENU_ICON_MAP?.[item.path as keyof typeof MENU_ICON_MAP] && (
        <Icon
          className={classNames(
            'h-4 w-4 shrink-0',
            isActive
              ? 'text-[color:var(--c-sidebar-active-text)]'
              : 'text-[color:var(--c-ink)]',
          )}
        />
      )}
      {!iconOnly && <span className="truncate">{label}</span>}
    </Link>
  );

  if (iconOnly) {
    return (
      <Tooltip title={label} placement="right">
        {link}
      </Tooltip>
    );
  }
  return link;
};

const NavGroup: FC<NavItemProps & { maxVisible?: number; groupIcon?: FC<{ className?: string; size?: number }> }> = ({
  item,
  maxVisible = 3,
  iconOnly = false,
  groupIcon: GroupIcon,
}) => {
  const location = useLocation();
  const hasActiveInHidden = useMemo(
    () =>
      (item.children || [])
        .slice(maxVisible)
        .some((sub) => location.pathname.startsWith(sub.path || '')),
    [location.pathname, maxVisible, item],
  );
  const [isExpanded, setIsExpanded] = useState(hasActiveInHidden);

  const visibleItems = useMemo(
    () => (item?.children || []).slice(0, maxVisible),
    [item, maxVisible],
  );
  const hiddenItems = useMemo(() => (item?.children || []).slice(maxVisible), [item, maxVisible]);
  const needsCollapse = useMemo(() => size(item?.children) > maxVisible, [item, maxVisible]);

  const handleCollapsed = () => setIsExpanded(!isExpanded);
  const ResolvedGroupIcon = GroupIcon || GROUP_ICON_MAP[item.path || ''] || GROUP_ICON_MAP[item.id || ''];
  return (
    <div className={classNames('flex flex-col', iconOnly ? 'p-1' : 'py-1')}>
      {!iconOnly && (
        <div
          className={classNames(
            'px-2 mb-1.5 text-[10px] uppercase h-7 shrink-0 flex items-center justify-between gap-1 font-semibold text-[color:var(--c-sidebar-group)] tracking-wider',
            needsCollapse &&
              'cursor-pointer rounded hover:bg-[var(--c-sidebar-hover)]/60 select-none',
          )}
          onClick={needsCollapse ? handleCollapsed : undefined}
          onKeyDown={
            needsCollapse
              ? (e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    handleCollapsed();
                  }
                }
              : undefined
          }
          role={needsCollapse ? 'button' : undefined}
          tabIndex={needsCollapse ? 0 : undefined}
          aria-expanded={needsCollapse ? isExpanded : undefined}
        >
          <div className="flex items-center gap-1.5 min-w-0">
            {ResolvedGroupIcon ? (
              <ResolvedGroupIcon size={12} className="shrink-0 opacity-80" />
            ) : null}
            <span className="truncate">{l(item.locale)}</span>
          </div>
          {needsCollapse ? (
            <span
              className="shrink-0 inline-flex h-5 w-5 items-center justify-center rounded text-[color:var(--c-sidebar-group)]"
              aria-hidden
            >
              <ChevronDown
                size={14}
                className={classNames(
                  'transition-transform duration-200',
                  isExpanded ? 'rotate-0' : '-rotate-90',
                )}
              />
            </span>
          ) : null}
        </div>
      )}
      <div className="flex flex-col gap-1">
        {visibleItems.map((sub) => (
          <NavItem key={sub.id || sub.path} item={sub} iconOnly={iconOnly} />
        ))}
        {needsCollapse &&
          isExpanded &&
          hiddenItems.map((sub) => (
            <NavItem key={sub.id || sub.path} item={sub} iconOnly={iconOnly} />
          ))}
      </div>
    </div>
  );
};

interface SidebarMenuProps {
  /** 桌面折叠图标栏模式 */
  iconOnly?: boolean;
  /** 移动端点选后关闭抽屉 */
  onNavigate?: () => void;
}

/** 从路由树抽出测试功能 / 任务，主区去掉对应项 */
const reorganizeMenu = (menuData: MenuDataItem[]) => {
  const testingByPath = new Map<string, MenuDataItem>();
  let tasksGroup: MenuDataItem | undefined;
  const main: MenuDataItem[] = [];

  for (const item of menuData) {
    if (item.hideInMenu) continue;
    // 全局用户/角色/发布：收入测试功能（路由仍挂 /console）
    if (item.path === '/console') {
      for (const child of item.children || []) {
        if (child.hideInMenu) continue;
        if (child.path && TESTING_MENU_PATHS.includes(child.path)) {
          testingByPath.set(child.path, child);
        }
      }
      continue;
    }
    if (item.path === '/tasks') {
      tasksGroup = item;
      continue;
    }
    if (item.children?.length) {
      const children: MenuDataItem[] = [];
      for (const child of item.children) {
        if (child.path && TESTING_MENU_PATHS.includes(child.path)) {
          testingByPath.set(child.path, child);
        } else {
          children.push(child);
        }
      }
      if (children.length) {
        main.push({ ...item, children });
      }
      continue;
    }
    if (item.path && TESTING_MENU_PATHS.includes(item.path)) {
      testingByPath.set(item.path, item);
      continue;
    }
    main.push(item);
  }

  const testingChildren = TESTING_MENU_PATHS.map((p) => testingByPath.get(p)).filter(
    Boolean,
  ) as MenuDataItem[];

  return {
    main,
    tasksGroup,
    testingGroup: {
      id: 'testing-features',
      name: 'testing',
      locale: 'menu.testing',
      children: testingChildren,
    } as MenuDataItem,
  };
};

const SidebarMenu: FC<SidebarMenuProps> = ({ iconOnly = false, onNavigate }) => {
  const app = useAppData();
  const treeRoutes = transformRoute(app.clientRoutes).menuData?.[0]?.children || [];
  const { initialState } = useModel('@@initialState');

  const { reachable: consoleReachable } = useConsoleReachable();
  const { hamiEnabled } = useK8sRuntime();

  // 左侧菜单按功能开关过滤；页面权限不足时仍展示入口，由右侧内容区提示并给出操作方法
  const filterMenuByFeature = (menuData: MenuDataItem[]): MenuDataItem[] => {
    return menuData
      .map((item) => {
        const path = item.path || '';
        if ((path === '/console' || path.startsWith('/console/')) && !consoleReachable) {
          return null;
        }
        if (item.children) {
          const filteredChildren = filterMenuByFeature(item.children);
          if (filteredChildren.length > 0) {
            return { ...item, children: filteredChildren };
          }
          return null;
        }
        if (path === '/regions' && !isRegionModeEnabled()) {
          return null;
        }
        if (path === '/monitor/hami' && !hamiEnabled) {
          return null;
        }
        const match = ROUTER_ACCESS_MAP.find((sub) => path.startsWith(sub.pathPrefix));
        if (match && !initialState?.globalConfig?.[match.accessKey]) {
          return null;
        }
        return item;
      })
      .filter(Boolean) as MenuDataItem[];
  };

  const { main, tasksGroup, testingGroup } = useMemo(
    () => reorganizeMenu(filterMenuByFeature(treeRoutes)),
    [treeRoutes, initialState?.globalConfig, consoleReachable, hamiEnabled],
  );

  return (
    <nav
      className="flex-1 overflow-y-auto overflow-x-hidden px-2 py-2 flex flex-col gap-2"
      aria-label="Main"
      onClick={(e) => {
        // 点击 Link 后在移动端关闭抽屉
        if (onNavigate && (e.target as HTMLElement).closest('a')) {
          onNavigate();
        }
      }}
    >
      {main.map((item) =>
        size(item.children) ? (
          <NavGroup
            key={item.id || item.path}
            item={item}
            iconOnly={iconOnly}
            maxVisible={99}
            groupIcon={GROUP_ICON_MAP[item.path || '']}
          />
        ) : (
          <NavItem key={item.id || item.path} item={item} iconOnly={iconOnly} />
        ),
      )}

      {/* 自上而下：主菜单 → 任务 → 测试功能（默认折叠） */}
      {tasksGroup && size(tasksGroup.children) ? (
        <NavGroup
          key={tasksGroup.id || tasksGroup.path}
          item={tasksGroup}
          iconOnly={iconOnly}
          maxVisible={99}
          groupIcon={GROUP_ICON_MAP['/tasks']}
        />
      ) : null}
      {size(testingGroup.children) ? (
        <NavGroup
          key={testingGroup.id}
          item={testingGroup}
          iconOnly={iconOnly}
          /* 图标栏模式无法点「展开」，故常显；展开侧栏默认折叠 */
          maxVisible={iconOnly ? 99 : 0}
          groupIcon={GROUP_ICON_MAP['testing-features']}
        />
      ) : null}
    </nav>
  );
};
export default SidebarMenu;
