/* eslint-disable no-undef */
/** Request 网络请求工具 更详细的 api 文档: https://github.com/umijs/umi-request */
import { extend } from 'umi-request';
import type { RequestOptionsInit } from 'umi-request';
// import { history } from 'umi';
import { notification } from 'antd';
import { getLocal } from '@/utils';

const codeMessage: Record<number, string> = {
  200: '服务器成功返回请求的数据',
  201: '新建或修改数据成功。',
  202: '一个请求已经进入后台排队（异步任务）。',
  204: '删除数据成功。',
  400: '发出的请求有错误，服务器没有进行新建或修改数据的操作。',
  401: 'token失效',
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
 * @zh-CN 异常处理程序
 * @en-US Exception handler
 */

const errorHandler = (error: { response: Response; errcode?: number; errmsg?: string }) => {
  const { response } = error;

  // 当前系统不存在response这个返回
  if (response && response.status) {
    const errorText = codeMessage[response.status] || response.statusText;
    const { status, url } = response;
    notification.error({
      message: `请求错误 ${status}: ${url}`,
      description: errorText,
    });
    throw new Error(error.errmsg);
  }
  throw new Error(error.errmsg);
};
/**
 * @en-US Configure the default parameters for request
 * @zh-CN 配置request请求时的默认参数
 */

const request = extend({
  prefix: 'http://18.116.197.226:3030/v1',
  errorHandler,
  // default error handling
  // credentials: 'include', // Does the default request bring cookies
});

// 请求拦截
request.interceptors.request.use((url: string, options: RequestOptionsInit) => {
  // 此处为拦截器，每次发送请求之前判断能否取到token
  const { token, token_type: type } = getLocal('user') || {};
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'application/json',
  };
  if (token && type) {
    headers.token = `${type} ${token}`;
    headers.Authorization = `${type} ${token}`;
  }
  return {
    url,
    options: { ...options, headers },
  };
});

// 响应拦截
request.interceptors.response.use(async (response: Response) => {
  return response;
});

export default request;
