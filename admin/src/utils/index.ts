import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import timezone from 'dayjs/plugin/timezone';
import { message } from 'antd';

dayjs.extend(utc);
dayjs.extend(timezone);
import type { DefaultOptionType } from 'antd/es/select';
import { lGet } from '@/utils/intl';
import { BYTES_IN_GB, UNLIMITED } from '@/constants';

export const sleep = (ms: number): Promise<void> => {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
};
export function isString(str: unknown) {
  return typeof str === 'string';
}
export function isNumber(num: unknown) {
  return typeof num === 'number';
}
export function isArray(list: unknown) {
  return Array.isArray(list);
}
/** 判断是否是JSON */
export function isJSON(value: unknown) {
  if (typeof value !== 'string') {
    return false;
  }
  try {
    JSON.parse(value);
    return true;
  } catch (e) {
    return false;
  }
}

export function isFormData(payload: unknown) {
  return payload instanceof FormData;
}
export async function asyncRunSafe<T = unknown>(fn: Promise<T>): Promise<[Error] | [null, T]> {
  try {
    return [null, await fn];
  } catch (e) {
    if (e instanceof Error) return [e];
    return [new Error('unknown error')];
  }
}

export const getTextWidthWithCanvas = (text: string, font?: string) => {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (ctx) {
    ctx.font =
      font ??
      '12px Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"';
    return Number(ctx.measureText(text).width.toFixed(2));
  }
  return 0;
};

const chars = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_';

export function randomString(length: number) {
  let result = '';
  for (let i = length; i > 0; --i) result += chars[Math.floor(Math.random() * chars.length)];
  return result;
}

export const getPurifyHref = (href: string) => {
  if (!href) return '';

  return href
    .replace(/javascript:/gi, '')
    .replace(/vbscript:/gi, '')
    .replace(/data:/gi, '');
};
/**
 * // 生成min～max 之间的随机数
 * @param min
 * @param max
 * @returns
 */

export const getRandomNumber = (min: number, max: number) => {
  return Math.floor(Math.random() * (max - min + 1)) + min;
};
/** 处理进度小数点 */
export function formatProgress(data: number) {
  return /\.\d{3}/.test(data.toString()) ? parseFloat(data.toFixed(2)) : data;
}
/** 转成百分比： value * 100, 乘后，如果小数点后大于2为则取前2位*/
export function transformRate(value = 0) {
  let result = value * 100;
  if (result % 1 !== 0 && result.toString().split('.')[1].length > 2) {
    return Number(result.toFixed(2));
  }
  return result;
}
/** 计算百分比 */
export function calculatePercentage(value = 0, total = 0) {
  if (!value || !total) return 0;
  let result = (value / total) * 100;
  if (result % 1 !== 0 && result.toString().split('.')[1].length > 2) {
    return Number(result.toFixed(2));
  }
  return result;
}
/** 与部署默认 TZ 对齐；客户可见墙钟都走此时区。 */
export const DISPLAY_TZ = 'Asia/Shanghai';

/** 获取本地时间 */
export function getLocalTimezone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** 客户可见时间：按 DISPLAY_TZ 格式化（unix / ISO 均先当绝对时刻）。 */
export function formatDisplayTime(
  value?: dayjs.ConfigType,
  fmt = 'YYYY-MM-DD HH:mm:ss',
) {
  if (value == null || value === '') return '-';
  const d = dayjs(value);
  if (!d.isValid()) return '-';
  return d.tz(DISPLAY_TZ).format(fmt);
}
/** 时间格式转UTC */
export function convertDateToUTC(time: string) {
  const timezone = getLocalTimezone();
  return dayjs(time).tz(timezone, true).utc().toISOString();
}

export function transformMarkdownText(text: string) {
  // text = text.replace(/\n\n(\d+)/g, '\n\n $1');
  return text;
}
// Clipboard API 实现
const copyWithClipboardAPI = (text: string) => {
  navigator.clipboard
    .writeText(text)
    .then(() => {
      message.success(lGet('global.message.copySuccess'));
    })
    .catch(() => {
      message.error(lGet('global.message.copyFail'));
    });
};

// Fallback 实现（使用 input 元素）
const copyWithInputElement = (text: string) => {
  const textarea = document.createElement('textarea');
  textarea.value = text;
  document.body.appendChild(textarea);
  textarea.select();
  try {
    document.execCommand('copy');
    message.success(lGet('global.message.copySuccess'));
  } catch (err) {
    message.error(lGet('global.message.copyFail'));
  } finally {
    document.body.removeChild(textarea);
  }
};
const copyToClipboard = (() => {
  // 判断 navigator.clipboard 是否可用
  const useClipboardAPI = !!navigator?.clipboard;
  // 根据判断结果返回对应的实现
  return useClipboardAPI ? copyWithClipboardAPI : copyWithInputElement;
})();

/** 设置 local */
export function setLocal(name: string, value: unknown) {
  localStorage.setItem(name, JSON.stringify(value));
}

/** 删除 local */
export function deleteLocal(name: string) {
  localStorage.removeItem(name);
}

/** 获取 local */
export function getLocal(name: string) {
  try {
    const data = localStorage.getItem(name);
    if (data) {
      return JSON.parse(data);
    } else {
      return undefined;
    }
  } catch (e) {
    return undefined;
  }
}
/** 获取监控api域名 */
export function getMonitorApiAdress(port: string) {
  const { protocol, hostname } = new URL(window.DOMAIN_API as string);
  return `${protocol}//${hostname}:${port}`;
}

interface TableParams {
  pageSize?: number;
  current?: number;
  [key: string]: unknown;
}

export function formPageParams(params: TableParams) {
  const { pageSize, current, ...resets } = params;
  return {
    curPageNum: current || 1,
    numPerPage: pageSize || 10,
    ...(resets || {}),
  };
}

export {
  convertObjToFormList,
  INTERNAL_EXTEND_KWARGS_KEYS,
  transformExtendFormListToObj,
  transformFormListToObj,
  transformFormListToStringObj,
  transformValueType,
} from './formList';

/** 生成uuid */
export function generateUUID(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  // 兼容方案 (UUID v4)
  return '10000000-1000-4000-8000-100000000000'.replace(/[018]/g, (c) => {
    const num = parseInt(c, 10);
    const randomByte = crypto.getRandomValues(new Uint8Array(1))[0];
    return (num ^ (randomByte & (15 >> (num / 4)))).toString(16);
  });
}

export function capitalizeFirstLetter(str: string) {
  if (!str) return '';
  return str.charAt(0).toUpperCase() + str.slice(1);
}

export const hexToColor = (hex?: string): { rgb: string; hsl: string } | null => {
  if (!hex) return null;

  // 支持 #ff0 -> #ffff00
  const shorthandRegex = /^#?([a-f\d])([a-f\d])([a-f\d])$/i;
  const newHex = hex.replace(shorthandRegex, (_, r, g, b) => r + r + g + g + b + b);

  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(newHex);
  if (!result) return null;

  /** ---------- RGB ---------- */
  const r255 = parseInt(result[1], 16);
  const g255 = parseInt(result[2], 16);
  const b255 = parseInt(result[3], 16);

  const rgb = `${r255}, ${g255}, ${b255}`;

  /** ---------- HSL ---------- */
  let r = r255 / 255;
  let g = g255 / 255;
  let b = b255 / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

    switch (max) {
      case r:
        h = (g - b) / d + (g < b ? 6 : 0);
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      case b:
        h = (r - g) / d + 4;
        break;
    }

    h *= 60;
  }

  const hsl = `${Math.round(h)} ${Math.round(s * 100)}% ${Math.round(l * 100)}%`;

  return { rgb, hsl };
};

/**
 * 获取相对时间描述（基于 dayjs）
 * @param targetTime 目标时间
 * @param nowTime 当前时间（可选，默认 dayjs()，方便测试）
 */
export function formatRelativeTime(
  targetTime: dayjs.ConfigType,
  nowTime: dayjs.ConfigType = dayjs(),
): string | JSX.Element {
  const target = dayjs(targetTime);
  const now = dayjs(nowTime);

  if (!targetTime || !target.isValid()) {
    return '-';
  }
  // 未来时间，直接返回原时间
  if (target.isAfter(now)) {
    return formatDisplayTime(target, 'YYYY-MM-DD HH:mm');
  }

  const diffMinutes = now.diff(target, 'minute');
  const diffHours = now.diff(target, 'hour');
  const diffDays = now.diff(target, 'day');

  if (diffMinutes < 1) {
    return lGet('global.time.justNow');
  }

  if (diffMinutes < 60) {
    return lGet('global.time.minuteAgo', '0', { count: diffMinutes });
  }

  if (diffHours < 24) {
    return lGet('global.time.hourAgo', '0', { count: diffHours });
  }

  if (diffDays < 30) {
    return lGet('global.time.dayAgo', '0', { count: diffDays });
  }

  // 超过 30 天，显示原时间
  return formatDisplayTime(target, 'YYYY-MM-DD HH:mm');
}
/** 字节转GB */
export function bytesToGB(bytes: unknown) {
  if (typeof bytes !== 'number') return 0;
  return Number((bytes / BYTES_IN_GB).toFixed(2));
}

export { hasPagePermissions, canManageSystemSettings } from './permissions';
export {
  getUsageLevel,
  getUsageStrokeColor,
  getUsageProgressClassName,
} from './usageColor';

export const optionsWithUnlimited = (options: DefaultOptionType[]) => {
  return [{ label: lGet('global.data.unlimited'), value: UNLIMITED }, ...options];
};

export const highlightJson = (json: string) => {
  let highlighted = json.replace(
    /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|True|False|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/g,
    (match) => {
      let cls = 'text-amber-600 dark:text-amber-400'; // number
      let text = match;

      if (/^"/.test(match)) {
        if (/:$/.test(match)) {
          cls = 'text-primary font-medium'; // key
          text = match.replace(/:$/, '');
          return `<span class="${cls}">${text}</span>:`;
        } else {
          cls = 'text-emerald-600 dark:text-emerald-400'; // string
        }
      } else if (/true|false|True|False/.test(match)) {
        cls = 'text-blue-600 dark:text-blue-400'; // boolean
      } else if (/null/.test(match)) {
        cls = 'text-gray-500'; // null
      }

      return `<span class="${cls}">${text}</span>`;
    },
  );
  highlighted = highlighted.replace(
    /(\s+)(#|\/\/)(\s+Optional(?:(?:\([^)]*\))|(?:【[^】]*】))?)/g,
    (match, space, symbol, word) => {
      return `${space}<span class="text-muted">${symbol}${word}</span>`;
    },
  );

  return highlighted;
};

export { copyToClipboard };
