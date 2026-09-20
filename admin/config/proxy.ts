/**
 * @name 代理的配置
 * @see 在生产环境 代理是无法生效的，所以这里没有生产环境的配置
 * -------------------------------
 * The agent cannot take effect in the production environment
 * so there is no configuration of the production environment
 * For details, please see
 * https://pro.ant.design/docs/deploy
 *
 * @doc https://umijs.org/docs/guides/proxy
 */
const remoteBackend = {
  target: process.env.NEBULA_BFF_PROXY || process.env.POWERLLM_API_PROXY || 'http://127.0.0.1:18090',
  changeOrigin: true,
  timeout: 120000,
  proxyTimeout: 120000,
  // SSE：关闭缓冲，避免 event-stream 被代理攒包/中断
  onProxyRes(proxyRes: { headers: Record<string, string | string[] | undefined> }) {
    const ct = String(proxyRes.headers?.['content-type'] || '');
    if (ct.includes('text/event-stream')) {
      proxyRes.headers['cache-control'] = 'no-cache, no-transform';
      proxyRes.headers['x-accel-buffering'] = 'no';
      delete proxyRes.headers['content-length'];
    }
  },
};

const remoteConsole = {
  target: process.env.POWERLLM_CONSOLE_PROXY || 'http://127.0.0.1:18090',
  changeOrigin: true,
  timeout: 120000,
  proxyTimeout: 120000,
};

export default {
  dev: {
    '/token': remoteBackend,
    '/v1': remoteBackend,
    '/sdapi': remoteBackend,
    '/controlnet': remoteBackend,
    '/api/console': remoteConsole,
    '/api/regions': remoteConsole,
  },

  /**
   * @name 详细的代理配置
   * @doc https://github.com/chimurai/http-proxy-middleware
   */
  test: {
    // localhost:8000/api/** -> https://preview.pro.ant.design/api/**
    '/api/': {
      target: 'https://proapi.azurewebsites.net',
      changeOrigin: true,
      pathRewrite: { '^': '' },
    },
  },
  pre: {
    '/api/': {
      target: 'your pre url',
      changeOrigin: true,
      pathRewrite: { '^': '' },
    },
  },
};
