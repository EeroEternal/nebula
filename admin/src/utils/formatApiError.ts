/**
 * 将后端 / 网络错误转为用户可读模型（F10）
 * 避免直接 JSON.stringify(detail) 砸给用户。
 */

import { l } from '@/utils/intl';

export type UserFacingError = {
  /** 短标题 */
  title: string;
  /** 可读原因 */
  description: string;
  /** 可选 request id */
  requestId?: string;
  /** 是否建议重试 */
  retryable: boolean;
  /** 技术详情（可折叠展示） */
  technical?: string;
  /** HTTP 状态；0 表示网络层 */
  status: number;
};

function statusTitle(status: number): string {
  const keys: Record<number, string> = {
    400: 'error.status.400',
    401: 'error.status.401',
    403: 'error.status.403',
    404: 'error.status.404',
    408: 'error.status.408',
    413: 'error.status.413',
    422: 'error.status.422',
    429: 'error.status.429',
    500: 'error.status.500',
    502: 'error.status.502',
    503: 'error.status.503',
    504: 'error.status.504',
  };
  const key = keys[status];
  return key ? l(key) : '';
}

/**
 * 引擎镜像拉取/手动注册：把后端英文或技术细节收成用户可读中文。
 * 匹配不到则原样返回。
 */
export function humanizeEngineImageError(raw?: string | null): string {
  const s = (raw || '').trim();
  if (!s) return '';
  const lower = s.toLowerCase();

  if (
    s.includes('镜像解析错误') ||
    lower.includes('cannot infer engine') ||
    lower.includes('infer engine from image')
  ) {
    return l('error.engine.infer');
  }
  if (
    s.includes('无权限拉取') ||
    lower.includes('unauthorized') ||
    lower.includes('authentication required') ||
    lower.includes('auth required') ||
    lower.includes('access denied') ||
    lower.includes('permission denied') ||
    lower.includes('login failed') ||
    lower.includes('docker login failed') ||
    /\b401\b/.test(lower) ||
    (lower.includes('denied') &&
      (lower.includes('pull') ||
        lower.includes('push') ||
        lower.includes('registry')))
  ) {
    return l('error.engine.unauthorized');
  }
  if (s.includes('请输入镜像地址') || lower.includes('image is required')) {
    return l('error.engine.imageRequired');
  }
  if (
    lower.includes('exported tar') ||
    lower.includes('shared path') ||
    (lower.includes('migrate-') && lower.includes('.tar'))
  ) {
    return l('error.engine.migrate');
  }
  // 仅镜像/仓库语境才收成「镜像不存在」。裸 FastAPI `Not Found`（如探活 404）不要误伤。
  if (
    lower.includes('manifest unknown') ||
    lower.includes('name unknown') ||
    lower.includes('no such image') ||
    ((lower.includes('not found') || lower.includes('does not exist')) &&
      (lower.includes('image') ||
        lower.includes('manifest') ||
        lower.includes('repository') ||
        lower.includes('docker') ||
        lower.includes('registry') ||
        lower.includes('tag')))
  ) {
    return l('error.engine.notFound');
  }
  if (
    lower.includes('timeout') ||
    lower.includes('deadline exceeded') ||
    lower.includes('timed out')
  ) {
    return l('error.engine.timeout');
  }
  // ctr 把短名 org 当成 registry：lookup vllm on … no such host
  if (
    lower.includes('no such host') ||
    lower.includes('failed to resolve reference') ||
    (lower.includes('lookup ') && lower.includes('on ') && lower.includes('53'))
  ) {
    return l('error.engine.host');
  }
  if (
    lower.includes('connection refused') ||
    lower.includes('network is unreachable') ||
    lower.includes('temporary failure in name resolution') ||
    (lower.includes('tls') && lower.includes('failed')) ||
    lower.includes('x509')
  ) {
    return l('error.engine.connect');
  }
  if (
    lower.includes('disk') &&
    (lower.includes('space') ||
      lower.includes('full') ||
      lower.includes('no space'))
  ) {
    return l('error.engine.disk');
  }
  return s;
}

/** 从任意 detail 结构提取可读字符串，绝不 dump 整段 JSON 给用户 */
export function stringifyDetail(detail: unknown): string {
  if (detail == null || detail === '') return '';
  if (typeof detail === 'string') {
    if (detail.includes('Error occurred while trying to proxy')) {
      return l('error.proxy.disconnected');
    }
    // 避免超长堆栈直接展示
    if (detail.length > 400) {
      return `${detail.slice(0, 400)}…`;
    }
    return detail;
  }
  if (typeof detail === 'number' || typeof detail === 'boolean') {
    return String(detail);
  }
  if (Array.isArray(detail)) {
    // FastAPI validation: [{loc, msg, type}, ...]
    const parts = detail
      .map((item) => {
        if (typeof item === 'string') return item;
        if (item && typeof item === 'object') {
          const rec = item as Record<string, unknown>;
          if (typeof rec.msg === 'string') {
            const loc = Array.isArray(rec.loc) ? rec.loc.filter((x) => x !== 'body').join('.') : '';
            return loc ? `${loc}: ${rec.msg}` : rec.msg;
          }
          if (typeof rec.message === 'string') return rec.message;
        }
        return '';
      })
      .filter(Boolean);
    return parts.join('；') || l('error.request.invalid');
  }
  if (typeof detail === 'object') {
    const rec = detail as Record<string, unknown>;
    if (typeof rec.msg === 'string') return rec.msg;
    if (typeof rec.message === 'string') return rec.message;
    if (typeof rec.detail === 'string') return stringifyDetail(rec.detail);
    if (Array.isArray(rec.detail)) return stringifyDetail(rec.detail);
    // 常见字段摘要，而非整对象
    const keys = Object.keys(rec).slice(0, 4);
    if (keys.length) {
      return keys
        .map((k) => {
          const v = rec[k];
          if (typeof v === 'string' || typeof v === 'number') return `${k}: ${v}`;
          return '';
        })
        .filter(Boolean)
        .join('；') || l('error.request.failed');
    }
  }
  return l('error.request.failed');
}

type ResponseEnvelope = {
  data?: {
    detail?: unknown;
    message?: unknown;
    data?: { detail?: unknown };
  };
  message?: unknown;
};

/** 从拦截器包络取 detail，兼容 data.detail / data.data.detail / message */
export function readResponseDetail(res: unknown): string {
  if (!res || typeof res !== 'object') return '';
  const r = res as ResponseEnvelope;
  return (
    stringifyDetail(r.data?.detail) ||
    stringifyDetail(r.data?.data?.detail) ||
    stringifyDetail(r.data?.message) ||
    stringifyDetail(r.message)
  );
}

type ThrownLike = {
  userError?: { description?: string };
  errmsg?: string;
  message?: string;
};

/** 从抛出的错误取用户可读文案（含 request.ts 的 APIError） */
export function readThrownMessage(error: unknown): string {
  if (!error || typeof error !== 'object') {
    return typeof error === 'string' ? error : '';
  }
  const e = error as ThrownLike;
  return (e.userError?.description || '').trim() || e.errmsg || e.message || '';
}

/** 请求失败：包络 detail 优先，其次 Error.message */
export function readRequestFailure(error: unknown): string {
  return readResponseDetail(error) || readThrownMessage(error);
}

export function thrownName(error: unknown): string {
  if (error instanceof Error) return error.name;
  if (error && typeof error === 'object' && 'name' in error) {
    const n = (error as { name?: unknown }).name;
    return typeof n === 'string' ? n : '';
  }
  return '';
}

export function extractRequestId(data: unknown): string | undefined {
  if (!data || typeof data !== 'object') return undefined;
  const rec = data as Record<string, unknown>;
  const id = rec.request_id ?? rec.requestId ?? rec.trace_id ?? rec.traceId;
  return typeof id === 'string' && id ? id : undefined;
}

export function formatApiError(
  status: number,
  data?: unknown,
  fallbackStatusText?: string,
): UserFacingError {
  const payload =
    data && typeof data === 'object'
      ? (data as Record<string, unknown>)
      : ({ detail: data } as Record<string, unknown>);

  const rawDetail = payload.detail ?? payload.message ?? data;
  const rawDescription =
    stringifyDetail(rawDetail) || fallbackStatusText || l('error.retry.later');
  const bareHttpNotFound =
    status === 404 && /^not found$/i.test(rawDescription.trim());
  const description = bareHttpNotFound
    ? rawDescription
    : humanizeEngineImageError(rawDescription) || rawDescription;
  const requestId = extractRequestId(payload) || extractRequestId(rawDetail);
  const technical =
    rawDetail != null && typeof rawDetail === 'object'
      ? safeJsonPreview(rawDetail)
      : typeof rawDetail === 'string' && rawDetail.length > 120
        ? rawDetail
        : undefined;

  // 引擎镜像类错误：标题直接用可读原因，避免「请求无效」+ 英文原文
  const engineTitles = new Set([
    l('error.engine.infer'),
    l('error.engine.unauthorized'),
    l('error.engine.imageRequired'),
    l('error.engine.notFound'),
    l('error.engine.timeout'),
    l('error.engine.connect'),
    l('error.engine.host'),
    l('error.engine.disk'),
  ]);
  if (engineTitles.has(description)) {
    return {
      title: description,
      description: '',
      requestId,
      retryable:
        description === l('error.engine.timeout') ||
        description === l('error.engine.connect'),
      technical:
        technical ||
        (typeof rawDetail === 'string' && rawDetail !== description
          ? rawDetail
          : undefined),
      status,
    };
  }

  if (status === 0) {
    return {
      title: l('error.network'),
      description: description || l('error.network.desc'),
      requestId,
      retryable: true,
      technical,
      status: 0,
    };
  }
  if (status === 401) {
    return {
      title: statusTitle(401),
      description: description || l('error.auth.relogin'),
      requestId,
      retryable: false,
      technical,
      status,
    };
  }
  if (status === 403) {
    return {
      title: statusTitle(403),
      description: description || l('error.auth.forbidden'),
      requestId,
      retryable: false,
      technical,
      status,
    };
  }
  if (status === 404) {
    return {
      title: statusTitle(404),
      description: description || l('error.notfound.desc'),
      requestId,
      retryable: false,
      technical,
      status,
    };
  }
  if (status === 408 || status === 504) {
    return {
      title: statusTitle(status) || l('error.status.408'),
      description: description || l('error.timeout.desc'),
      requestId,
      retryable: true,
      technical,
      status,
    };
  }

  const title =
    statusTitle(status) ||
    (status >= 500 ? l('error.status.500') : l('error.request.failed'));
  const retryable = status >= 500 || status === 429;
  return {
    title,
    description,
    requestId,
    retryable,
    technical,
    status,
  };
}

function safeJsonPreview(value: unknown, max = 500): string | undefined {
  try {
    const s = JSON.stringify(value);
    if (!s || s === '{}' || s === '[]') return undefined;
    return s.length > max ? `${s.slice(0, max)}…` : s;
  } catch {
    return undefined;
  }
}

/** 组装通知 description：正文 + 可选 request id */
export function buildErrorDescription(err: UserFacingError): string {
  const parts = [err.description];
  if (err.requestId) {
    parts.push(`Request ID: ${err.requestId}`);
  }
  if (err.retryable) {
    parts.push(l('error.retry.hint'));
  }
  return parts.filter(Boolean).join('\n');
}
