declare module 'slash2';
declare module '*.json' {
  const value: Record<string, unknown>;
  export default value;
}
declare module '*.css';
declare module '*.less';
declare module '*.scss';
declare module '*.sass';
declare module '*.svg';
declare module '*.png';
declare module '*.jpg';
declare module '*.jpeg';
declare module '*.gif';
declare module '*.bmp';
declare module '*.tiff';
declare module 'numeral';
declare module '@antv/data-set';
declare module 'mockjs';
declare module 'react-fittext';

declare const REACT_APP_ENV: 'test' | 'dev' | 'pre' | false;

declare interface Window {
  /** 浏览器图标 */
  FAVICON?: string;
  /** logo（兼容旧配置；浅色面优先 LOGO_BLUE） */
  LOGO?: string;
  /** 品牌蓝 logo（浅色侧栏 / 登录页默认，design.md #2744A5） */
  LOGO_BLUE?: string;
  /** 深色 logo */
  LOGO_DARK?: string;
  /** 白色 logo（深色底场景） */
  LOGO_WHITE?: string;
  /** 后端API地址 */
  DOMAIN_API?: string;
  /** 后端API地址(不带v1) */
  DOMAIN_API_RAW: string;
  /** 默认侧边栏展开收起状态: true表示默认收起，false或不设置表示默认展开 */
  DEFAULT_SIDER_COLLAPSED: boolean;
  /** 路由前缀 */
  PREFIX_PATH: string;
  /** 主题色（规范见仓库根 design.md） */
  THEME_PRIMARY_COLOR: string;
  /** 标题(logo旁边的标题，默认空) */
  TITLE: string;
  /** 可选环境标识（登录页展示，如 staging / prod） */
  ENV_LABEL?: string;
  /** 可选版本号（登录页展示） */
  VERSION?: string;
}
