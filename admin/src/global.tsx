import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import timezone from 'dayjs/plugin/timezone';
import weekday from 'dayjs/plugin/weekday';
import localeData from 'dayjs/plugin/localeData';
import duration from 'dayjs/plugin/duration';
dayjs.extend(utc);
dayjs.extend(timezone);
dayjs.tz.setDefault('Asia/Shanghai');
dayjs.extend(weekday);
dayjs.extend(localeData);
dayjs.extend(duration);

import { hexToColor } from '@/utils';
import { applyRegionToDomainApi } from '@/utils/region';
// 统一去除prefixPath 中的斜杠，由使用处 自己来拼接前后的斜杠:
// 1.防止用户仅配置了 window.PREFIX_PATH = '/' 这种配置实际上不需配置，默认的即是 '/';
// 2.用户会配置 window.PREFIX_PATH = '/xmodels' 或者 window.PREFIX_PATH = 'xmodels' 或 window.PREFIX_PATH = '/xmodels/';
window.PREFIX_PATH = (window.PREFIX_PATH || '').replace(/^\/|\/$/g, '');

const api = window.DOMAIN_API || window.location.origin;
const apiBaseUrl = api.endsWith('/v1') ? api.slice(0, -3) : api;

// 若含有路由前缀，则接口请求地址也拼接路由前缀（/v1/overview -> /xmodels/v1/overview）
const apiRootUrl = window.PREFIX_PATH ? `${apiBaseUrl}/${window.PREFIX_PATH}` : apiBaseUrl;
window.DOMAIN_API_RAW = apiRootUrl;
window.DOMAIN_API = `${apiRootUrl}/v1`;

// 多地点（region）模式下改写为 Console 代理前缀（单 region / 未接入 Console 时无副作用）
applyRegionToDomainApi();

// 设置浏览器图标
if (window.FAVICON) {
  // 如果已配置 window.FAVICON，动态设置 favicon
  const link = document.createElement('link');
  link.rel = 'shortcut icon';
  link.href = window.FAVICON;
  document.head.appendChild(link);
}
/** 默认品牌交互色（X 系列 logo fill / design.md `--c-primary`） */
const defaultPrimary = '#2744A5';
const parsedColor = hexToColor(window.THEME_PRIMARY_COLOR);
// 防止不合法
window.THEME_PRIMARY_COLOR = parsedColor ? window.THEME_PRIMARY_COLOR : defaultPrimary;

document.documentElement.style.setProperty(
  '--primary',
  parsedColor?.hsl ?? hexToColor(defaultPrimary)!.hsl,
);
document.documentElement.style.setProperty('--primary-color', window.THEME_PRIMARY_COLOR);
document.documentElement.style.setProperty('--c-primary', window.THEME_PRIMARY_COLOR);
document.documentElement.setAttribute('data-new-ui', 'new');
