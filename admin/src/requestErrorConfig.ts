import type { RequestOptions } from '@@/plugin-request/request';
import type { RequestConfig } from '@umijs/max';
import { notification } from 'antd';
import { getLocal } from '@/utils';
import eventBus from '@/utils/eventBus';
/**
 * 异常处理程序
 */

class APIError extends Error {
  errcode: number;
  errmsg?: string;
  response?: Response;

  constructor(errcode: number, errmsg?: string, response?: Response) {
    super(errmsg);
    this.errcode = errcode;
    this.errmsg = errmsg;
    this.response = response;
  }
}

type ResponseStructure = {
  success?: boolean;
  data?: unknown;
  errorCode?: string | number;
  errorMessage?: string;
  showType?: unknown;
};

class BizError extends Error {
  info?: {
    errorCode?: string | number;
    errorMessage?: string;
    showType?: unknown;
    data?: unknown;
  };

  constructor(
    message?: string,
    info?: {
      errorCode?: string | number;
      errorMessage?: string;
      showType?: unknown;
      data?: unknown;
    },
  ) {
    super(message);
    this.name = 'BizError';
    this.info = info;
  }
}

const codeMessage: Record<number, string> = {
  200: '服务器成功返回请求的数据',
  201: '新建或修改数据成功。',
  202: '一个请求已经进入后台排队（异步任务）。',
  204: '删除数据成功。',
  400: '发出的请求有错误，服务器没有进行新建或修改数据的操作。',
  401: '无权限',
  403: '无权限',
  404: '发出的请求针对的是不存在的记录，服务器没有进行操作。',
  406: '请求的格式不可得。',
  410: '请求的资源被永久删除，且不会再得到的。',
  422: '当创建一个对象时，发生一个验证错误。',
  500: '服务器发生错误，请检查服务器。',
  502: '网关错误。',
  503: '服务不可用，服务器暂时过载或维护。',
  504: '网关超时。',
};
/**
 * @name 错误处理
 * pro 自带的错误处理， 可以在这里做自己的改动
 * @doc https://umijs.org/docs/max/request#配置
 */
export const errorConfig: RequestConfig = {
  // 错误处理： umi@3 的错误处理方案。
  errorConfig: {
    // 错误抛出
    errorThrower: (res) => {
      const { success, data, errorCode, errorMessage, showType } =
        res as unknown as ResponseStructure;
      if (!success) {
        throw new BizError(errorMessage, { errorCode, errorMessage, showType, data });
      }
    },
    // 错误接收及处理
    errorHandler: (error: unknown, _opts?: RequestOptions) => {
      if (error instanceof APIError && error.response) {
        const errorText = codeMessage[error.response.status] || error.response.statusText;
        notification.error({
          message: errorText,
          description: errorText,
        });
      } else {
        // 处理其他类型的错误

      }
      throw error;
      // if(error.response){
      //   console.log('error.response.status', error.response.status);
      //   console.log('error.response.headers', error.response.headers);
      //   console.log('error.data', error.data);
      //   console.log('error.request', error.request);
      //   console.log('message', codeMessage[error.response.status]);
      //   const errorText = codeMessage[error.response.status] || response.statusText;
      //   notification.error({
      //     message: `Request error ${status}: ${url}`,
      //     description: errorText,
      //   });
      // } else {
      //   // The request was made but no response was received or error occurs when setting up the request.
      //   console.log(error.message);
      // }
      // throw error;
    },
  },

  // 请求拦截器
  requestInterceptors: [
    (config: RequestOptions) => {
      let url = config?.url;
      // 此处为拦截器，每次发送请求之前判断能否取到token
      const rawUserData = getLocal('user');
      const headers = {
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': '*',
        Accept: 'application/json',
        token: undefined,
        Authorization: undefined,
      };
      if (rawUserData) {
        const { token, token_type: type } = rawUserData || {};
        headers.token = `${type} ${token}`;
        headers.Authorization = `${type} ${token}`;
      }
      if (url) {
        url = url.indexOf('http') >= 0 ? url : window.DOMAIN_API + url;
      }
      return {
        url,
        options: { ...options, headers },
      };
    },
  ],

  // 响应拦截器
  responseInterceptors: [
    async (response) => {
      // 拦截401
      let responseData = {
        data: {
          detail: '',
        },
        success: false,
      };
      if ([521, 522, 523].includes(response.status)) {
        // 发布消息，打开证书弹窗
        eventBus.emit('showModal', {visible: true, modalName: 'licenseModal'})
        return responseData;
      }
      if (response.status === 200) {
        // const responseData = await response.clone().json();
        responseData.data = await response.clone().json();
        responseData.success = true;
      } else if (response.status === 500) {
        responseData.success = false;
        responseData.data = await response.clone().json();
      } else {
        responseData.data = await response.clone().json();
        responseData.success = false;
      }
      if (!responseData.success) {
        const errorText = codeMessage[response.status] || response.statusText;
        const errorDetail = JSON.stringify(responseData.data.detail);
        notification.error({
          message: errorText,
          description: errorDetail,
        });
      }
      return responseData;
    },
  ],
};
