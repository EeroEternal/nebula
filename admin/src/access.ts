// import { ModelInitialState } from '@/types/global';

/**
 * @see https://umijs.org/docs/max/access#access
 * */
// 路由权限access 已废弃，具体配置请至 @/constants/index -> ROUTER_ACCESS_MAP
// 原因 已在 app.tsx中getInitialState 中解释
export default function access() {
  return {
    // enableLangfuse: globalReady ? globalConfig?.enable_langfuse : true,
    // enableLangfuse: globalConfig?.enable_langfuse,
  };
}
