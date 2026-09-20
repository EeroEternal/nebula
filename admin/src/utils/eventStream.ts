import { getLocal } from '@/utils';
import { readThrownMessage, thrownName } from '@/utils/formatApiError';
export interface PostEventStreamFetcherOptions<T> {
  onData: (data: T) => void;
  onError: (msg: string) => void;
  onEnd?: () => void;
}

export interface PostEventStreamFetcherParams<T> {
  url: string; // 请求的 URL
  data: unknown; // 请求的参数
  options: PostEventStreamFetcherOptions<T>; // 回调选项
  headers?: Record<string, string>; // 可选的请求头
}

export class EventStreamController {
  private abortController: AbortController;

  constructor() {
    this.abortController = new AbortController();
  }

  /**
   * 终止当前的流式请求
   */
  terminate() {
    this.abortController.abort();
  }

  /**
   * 获取当前的信号对象
   */
  getSignal() {
    return this.abortController.signal;
  }
}
const errorText = '系统出错了，请重试！';
const bodyTooLargeText =
  '上传内容过大（常见于带图对话）。请使用更小的图片后重试，或联系管理员提高网关请求体限制（client_max_body_size）。';

export function getErrorText(errorDetail: string) {
  // 若是因为base64报错，提取前1000字符，防止报错信息因含有base64信息，导致报错文案过长
  if (errorDetail && errorDetail.length > 1000 && /data:.+?;base64/.test(errorDetail)) {
    errorDetail = `${errorDetail.slice(0, 1000)}...`;
  }
  return errorDetail;
}

/** 安全读取非 2xx 响应文案；nginx 413 等常返回 HTML，不能直接 .json() */
export async function readHttpErrorMessage(response: Response): Promise<string> {
  if (response.status === 413) {
    return bodyTooLargeText;
  }
  const ct = String(response.headers.get('content-type') || '').toLowerCase();
  try {
    if (ct.includes('application/json')) {
      const error = await response.clone().json();
      return getErrorText(
        (typeof error?.detail === 'string' && error.detail) || errorText,
      );
    }
    const text = (await response.clone().text()).trim();
    if (/request entity too large/i.test(text) || text.includes('413')) {
      return bodyTooLargeText;
    }
    if (text.startsWith('<')) {
      return `服务返回错误（HTTP ${response.status}），请稍后重试。`;
    }
    if (text) {
      return getErrorText(text.length > 200 ? `${text.slice(0, 200)}…` : text);
    }
  } catch {
    /* fall through */
  }
  return response.status
    ? `服务返回错误（HTTP ${response.status}），请稍后重试。`
    : errorText;
}
export interface GetEventStreamFetcherParams<T> {
  url: string;
  params?: Record<string, string | number | boolean | undefined | null>;
  options: PostEventStreamFetcherOptions<T>;
  headers?: Record<string, string>;
}

function buildQuery(params?: GetEventStreamFetcherParams<unknown>['params']): string {
  if (!params) return '';
  const sp = new URLSearchParams();
  Object.entries(params).forEach(([k, v]) => {
    if (v === undefined || v === null || v === '') return;
    sp.set(k, String(v));
  });
  const q = sp.toString();
  return q ? `?${q}` : '';
}

/** GET SSE stream with Bearer auth (logs follow, etc.). */
export async function getEventStreamFetcher<T = unknown>(
  params: GetEventStreamFetcherParams<T>,
  controller: EventStreamController,
): Promise<void> {
  const { url, params: query, options, headers } = params;
  const { onData, onError, onEnd } = options;
  const { token, token_type } = getLocal('user') || {};
  const fullUrl = `${window.DOMAIN_API}${url}${buildQuery(query)}`;

  try {
    const response = await fetch(fullUrl, {
      method: 'GET',
      headers: {
        Accept: 'text/event-stream',
        Authorization: `${token_type} ${token}`,
        ...(headers || {}),
      },
      signal: controller.getSignal(),
    });

    if (!response.ok) {
      onError(await readHttpErrorMessage(response));
      return;
    }

    const reader = response.body?.getReader();
    if (!reader) {
      onError('Failed to get reader from response body');
      return;
    }
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        let framesInRead = 0;
        while (true) {
          let sepLen = 4;
          let pos = buffer.indexOf('\r\n\r\n');
          if (pos < 0) {
            pos = buffer.indexOf('\n\n');
            sepLen = 2;
          }
          if (pos < 0) break;
          const chunk = buffer.slice(0, pos);
          buffer = buffer.slice(pos + sepLen);
          await parseEventData<T>(chunk, onData, onError);
          framesInRead += 1;
          if (framesInRead % 4 === 0) {
            await new Promise<void>((r) => setTimeout(r, 0));
          }
        }
      }
      if (buffer.trim()) {
        await parseEventData<T>(buffer, onData, onError);
        buffer = '';
      }
    } catch (err: unknown) {
      if (thrownName(err) !== 'AbortError') {
        onError(readThrownMessage(err) || errorText);
      }
    } finally {
      reader.releaseLock();
    }
  } catch (err: unknown) {
    if (thrownName(err) !== 'AbortError') {
      onError(readThrownMessage(err) || errorText);
    }
  } finally {
    onEnd?.();
  }
}

export async function postEventStreamFetcher<T = unknown>(
  params: PostEventStreamFetcherParams<T>,
  controller: EventStreamController,
): Promise<void> {
  const { url, data, options, headers } = params;
  const { onData, onError, onEnd } = options;
  const { token, token_type } = getLocal('user') || {};
  const fullUrl = `${window.DOMAIN_API}${url}`;

  try {
    const response = await fetch(fullUrl, {
      method: 'POST',
      body: JSON.stringify(data),
      headers: {
        'Content-Type': 'application/json',
        Authorization: `${token_type} ${token}`,
        ...(headers || {}),
      },
      signal: controller.getSignal(), // 使用 AbortController 的信号
    });

    if (!response.ok) {
      onError(await readHttpErrorMessage(response));
      return;
    }
    const payload =
      data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
    if (!payload.stream) {
      const result = await response.clone().json();
      if (result?.id) {
        onData(result);
      } else {
        onError(errorText);
      }
      return;
    }
    const reader = response.body?.getReader();
    if (!reader) {
      onError('Failed to get reader from response body');
      return;
    }
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        // SSE frames end with blank line: prefer CRLFCRLF, also accept LFLF.
        let framesInRead = 0;
        while (true) {
          let sepLen = 4;
          let pos = buffer.indexOf('\r\n\r\n');
          if (pos < 0) {
            pos = buffer.indexOf('\n\n');
            sepLen = 2;
          }
          if (pos < 0) break;
          const chunk = buffer.slice(0, pos);
          buffer = buffer.slice(pos + sepLen);
          await parseEventData<T>(chunk, onData, onError);
          framesInRead += 1;
          // Yield ~once per animation frame batch so UI streams without
          // stalling on thousands of setTimeout(0) per garbage token.
          if (framesInRead % 4 === 0) {
            await new Promise<void>((r) => setTimeout(r, 0));
          }
        }
      }
      // flush trailing frame without final blank line
      if (buffer.trim()) {
        await parseEventData<T>(buffer, onData, onError);
        buffer = '';
      }
    } catch (err: unknown) {
      if (thrownName(err) !== 'AbortError') {
        onError(readThrownMessage(err) || errorText);
      }
    } finally {
      // 释放 reader
      reader.releaseLock();
    }
  } catch (err: unknown) {
    if (thrownName(err) !== 'AbortError') {
      // 网络错误
      onError(readThrownMessage(err) || errorText);
    }
  } finally {
    onEnd?.();
  }
}

async function parseEventData<T>(
  text: string,
  next: (data: T) => void | Promise<void>,
  onError: (msg: string) => void,
) {
  // 切割接收到的数据段
  const lines = text.split('\n');

  for (const rawLine of lines) {
    const line = rawLine.replace(/\r$/, '');
    if (line.startsWith('data:')) {
      // support both "data: {...}" and "data:{...}"
      const eventData = line.slice(5).replace(/^\s/, '');
      if (eventData === '[DONE]') return;
      try {
        const json = JSON.parse(eventData);
        if (json?.error) {
          onError(json.error);
        } else {
          await next(json); // 处理转换后的JSON数据（逐帧 await，便于 UI 刷新）
        }
      } catch (error) {
        console.error('Error parsing JSON:', error);
      }
    }
  }
}
