/* eslint-disable no-undef */
/** Request 网络请求工具 更详细的 api 文档: https://github.com/umijs/umi-request */
import { extend } from 'umi-request';
import type { RequestOptionsInit } from 'umi-request';
import { message } from 'antd';
import { getLocal } from '@/utils';
import eventBus from '@/utils/eventBus';
import { lGet } from '@/utils/intl';
import { LOGIN_PATH } from '@/constants';
import { IO_HREF } from '@/constants';
import {
  formatApiError,
  buildErrorDescription,
  type UserFacingError,
} from '@/utils/formatApiError';
import { notifyErrorOnce } from '@/utils/notifyOnce';

/**
 * 异常处理程序
 */

class APIError extends Error {
  errcode: number;
  errmsg?: string;
  response?: Response;
  userError?: UserFacingError;

  constructor(errcode: number, errmsg?: string, response?: Response, userError?: UserFacingError) {
    super(errmsg);
    this.errcode = errcode;
    this.errmsg = errmsg;
    this.response = response;
    this.userError = userError;
  }
}

/**
 * @zh-CN 异常处理程序
 * @en-US Exception handler
 */

const errorHandler = (error: Error) => {
  // 网络断开 / 超时（umi-request 会抛 TypeError 或带 type 的错误）
  const anyErr = error as Error & { type?: string; name?: string; request?: unknown };
  const isNetwork =
    anyErr.type === 'Timeout' ||
    anyErr.name === 'TimeoutError' ||
    anyErr.message?.toLowerCase?.().includes('network') ||
    anyErr.message?.toLowerCase?.().includes('failed to fetch') ||
    (error instanceof TypeError && !('response' in error));

  if (isNetwork && !(error instanceof APIError)) {
    // 网络/超时：右侧服务异常通知（同内容只提示一次）
    const userError = formatApiError(0, { detail: error.message || '网络异常' });
    notifyErrorOnce({
      message: userError.title,
      description: buildErrorDescription(userError),
    });
    throw error;
  }

  if (error instanceof APIError && error.userError) {
    const status = error.userError.status;
    if (status >= 500 || status === 0) {
      notifyErrorOnce({
        message: error.userError.title,
        description: buildErrorDescription(error.userError),
      });
    } else {
      message.error(
        (error.userError.description || '').trim() || error.userError.title,
      );
    }
  } else if (error instanceof APIError && error.response) {
    const userError = formatApiError(error.response.status, {
      detail: error.errmsg,
    });
    if (error.response.status >= 500) {
      notifyErrorOnce({
        message: userError.title,
        description: buildErrorDescription(userError),
      });
    } else {
      message.error(
        (userError.description || '').trim() || userError.title,
      );
    }
  } else {
    console.error(error);
    // 未知异常按服务问题处理
    const userError = formatApiError(0, { detail: error.message });
    notifyErrorOnce({
      message: userError.title,
      description: buildErrorDescription(userError),
    });
  }
  throw error;
};

function getAuthorization(optionsAuth?: string) {
  const { token, token_type } = getLocal('user') || {};
  if (optionsAuth) {
    return optionsAuth;
  }
  if (token && token_type) {
    return `${token_type} ${token}`;
  }
  return undefined;
}

async function parseResponseBody(response: Response) {
  const cloned = response.clone();
  const contentType = response.headers.get('Content-Type') || '';
  if (contentType.includes('application/octet-stream')) {
    return cloned.blob();
  }
  const text = await cloned.text();
  if (!text) {
    return { detail: '', message: '' };
  }
  try {
    return JSON.parse(text);
  } catch {
    return { detail: text, message: text };
  }
}

/**
 * @en-US Configure the default parameters for request
 * @zh-CN 配置request请求时的默认参数
 */
const request = extend({
  errorHandler,
  timeout: 60000,
});
// 请求拦截
request.interceptors.request.use(
  (url: string, options) => {
    const { customOptions = {} } = options || {};
    const isFormData = options?.data instanceof FormData;
    const headers = {
      ...(isFormData ? {} : { 'Content-Type': 'application/json' }),
      'Access-Control-Allow-Origin': '*',
      Accept: 'application/json',
      Authorization: getAuthorization(options.Authorization),
    };
    const isAbsoluteUrl = url.startsWith('http');
    const finalUrl = isAbsoluteUrl ? url : `${window.DOMAIN_API}${url}`;
    return {
      url: finalUrl,
      options: 'customOptions' in options ? customOptions : { ...options, headers },
    };
  },
  {
    global: false,
  },
);
type ResponseEnvelope = {
  data: unknown;
  success: boolean;
};

type ResponseHandler = (
  response: Response,
  options?: RequestOptionsInit,
) => Promise<ResponseEnvelope>;

const handleResponse: ResponseHandler = async (response, options) => {
  let responseData: ResponseEnvelope = {
    data: {
      detail: '',
      message: '',
    },
    success: false,
  };
  const skipNotification = Boolean(options?.skipNotification);
  if (response.url.startsWith(IO_HREF)) {
    responseData.data = await parseResponseBody(response);
    responseData.success = response.status === 200;
    return responseData;
  }
  if ([521, 522, 523].includes(response.status)) {
    responseData.data = await parseResponseBody(response);
    // 发布消息，打开证书弹窗
    eventBus.emit('showModal', { visible: true, modalName: 'licenseModal' });
    return responseData;
  }
  // HA secondary: historically 525; now 503 so nginx can skip follower upstreams.
  if (response.status === 525 || response.status === 503) {
    responseData.data = await parseResponseBody(response);
    const detail = String(
      (responseData.data as { detail?: string } | undefined)?.detail || '',
    );
    if (
      response.status === 525 ||
      detail.toLowerCase().includes('secondary supervisor')
    ) {
      eventBus.emit('showMessage', {
        message: lGet('global.message.switchedPrimarySupervisorMsg') as string,
      });
      return responseData;
    }
    if (response.status === 503) {
      // 服务不可用：右侧 notification
      responseData.success = false;
      const userError = formatApiError(
        response.status,
        responseData.data,
        response.statusText,
      );
      eventBus.emit('notification', {
        message: userError.title,
        description: buildErrorDescription(userError),
      });
      return responseData;
    }
  }
  // 登录过期或未登录 → 登录恢复
  if (response.status === 401) {
    responseData.data = await parseResponseBody(response);
    const userError = formatApiError(401, responseData.data);
    eventBus.emit('redirect', {
      path: LOGIN_PATH,
      message: userError.description || '登录已失效，请重新登录',
    });
    return responseData;
  }
  if (response.status === 200) {
    responseData.data = await parseResponseBody(response);
    responseData.success = true;
  } else {
    responseData.data = await parseResponseBody(response);
    responseData.success = false;
  }
  if (!responseData.success && !skipNotification) {
    const userError = formatApiError(
      response.status,
      responseData.data,
      response.statusText,
    );
    // 业务/参数错误 → 顶部 message；仅服务端/网关类问题用右侧 notification
    const isServiceProblem =
      response.status >= 500 || response.status === 0;
    if (isServiceProblem) {
      eventBus.emit('notification', {
        message: userError.title,
        description: buildErrorDescription(userError),
      });
    } else {
      const text =
        (userError.description || '').trim() ||
        userError.title ||
        '请求失败';
      eventBus.emit('showMessage', { message: text, type: 'error' });
    }
  }
  return responseData;
};

// umi-request 声明必须返回 Response；产品拦截器返回包络。
(request.interceptors.response.use as unknown as (handler: ResponseHandler) => void)(
  handleResponse,
);

export default request;
export { formatApiError, buildErrorDescription };
