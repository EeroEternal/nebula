/**
 * @name umi 的路由配置
 * @description 只支持 path,component,routes,redirect,wrappers,name,icon 的配置
 * @param path  path 只支持两种占位符配置，第一种是动态参数 :id 的形式，第二种是 * 通配符，通配符只能出现路由字符串的最后。
 * @param component 配置 location 和 path 匹配后用于渲染的 React 组件路径。可以是绝对路径，也可以是相对路径，如果是相对路径，会从 src/pages 开始找起。
 * @param routes 配置子路由，通常在需要为多个路径增加 layout 组件时使用。
 * @param redirect 配置路由跳转
 * @param wrappers 配置路由组件的包装组件，通过包装组件可以为当前的路由组件组合进更多的功能。 比如，可以用于路由级别的权限校验
 * @param name 配置路由的标题，默认读取国际化文件 menu.ts 中 menu.xxxx 的值，如配置 name 为 login，则读取 menu.ts 中 menu.login 的取值作为标题
 * @param icon 配置路由的图标，取值参考 https://ant.design/components/icon-cn， 注意去除风格后缀和大小写，如想要配置图标为 <StepBackwardOutlined /> 则取值应为 stepBackward 或 StepBackward，如想要配置图标为 <UserOutlined /> 则取值应为 user 或者 User
 * @doc https://umijs.org/docs/guides/routes
 */
export default [
  {
    path: '/login',
    layout: false,
    component: './login',
  },
  {
    path: '/dashboard',
    name: 'dashboard',
    icon: 'HomeOutlined',
    component: './dashboard',
  },
  {
    path: '/regions',
    name: 'regions',
    icon: 'GlobalOutlined',
    component: './regions',
  },
  {
    path: '/console',
    name: 'console',
    icon: 'CloudServerOutlined',
    routes: [
      {
        path: '/console',
        redirect: '/console/users',
      },
      {
        path: '/console/users',
        name: 'users',
        component: './console/users',
      },
      {
        path: '/console/roles',
        name: 'roles',
        component: './console/roles',
      },
      {
        path: '/console/releases',
        name: 'releases',
        component: './console/releases',
      },
    ],
  },
  {
    path: '/models',
    name: 'models',
    icon: 'TableOutlined',
    routes: [
      {
        path: '/models',
        redirect: '/models/repository',
      },
      {
        name: 'engines',
        path: '/models/engines',
        routes: [
          {
            path: '/models/engines',
            redirect: '/models/engines/images',
          },
          {
            path: '/models/engines/:tab',
            component: './management/engines',
          },
        ],
      },
      {
        name: 'repository',
        path: '/models/repository',
        routes: [
          {
            path: '/models/repository',
            component: './management/repository/RedirectLast',
          },
          {
            // 静态路径须在 :modelType 之前，避免被动态段吃掉
            name: 'downloads',
            hideInMenu: true,
            path: '/models/repository/downloads',
            component: './management/downloads',
          },
          {
            path: '/models/repository/:modelType',
            component: './management/repository/[modelType]',
          },
          {
            name: 'detail',
            hideInMenu: true,
            path: '/models/repository/:modelType/:modelName',
            component: './management/repository/[modelDetail]',
          },
        ],
      },
      {
        name: 'register',
        path: '/models/register',
        component: './management/register',
      },
      {
        name: 'catalog',
        path: '/models/catalog',
        component: './management/catalog',
      },
      {
        // 兼容旧入口；权限按 repository 判定，独立 /models/downloads 会 403
        path: '/models/downloads',
        redirect: '/models/repository/downloads',
      },
      {
        name: 'instances',
        path: '/models/instances',
        routes: [
          {
            path: '/models/instances',
            component: './management/instance',
          },
          {
            // 无 path 布局：详情/日志/对话共享壳，Tab 切换不重挂标题与导航
            component: './management/instance/workspace',
            hideInMenu: true,
            routes: [
              {
                // name 用 detail → menu.models.instances.detail（勿写 instances.detail，会拼成重复段）
                name: 'detail',
                locale: 'menu.models.instances.detail',
                path: '/models/instances/detail',
                component: './management/instance/detail',
                hideInMenu: true,
              },
              {
                // 面包屑/标题保持「实例详情」，不因对话 Tab 改名
                name: 'detail',
                locale: 'menu.models.instances.detail',
                path: '/models/instances/chat',
                component: './management/instanceDetail',
                hideInMenu: true,
              },
              {
                name: 'detail',
                locale: 'menu.models.instances.detail',
                path: '/models/instances/logs',
                component: './management/instance/logs',
                hideInMenu: true,
              },
            ],
          },
        ],
      },
      {
        // 兼容 /list 入口
        path: '/models/instances/list',
        redirect: '/models/instances',
        hideInMenu: true,
      },
      // {
      //   name: 'drawingTool',
      //   path: '/management/drawingTool',
      //   component: './management/drawingTool',
      // },
    ],
  },
  {
    path: '/tasks',
    name: 'tasks',
    icon: 'ControlOutlined',
    routes: [
      {
        path: '/tasks',
        redirect: '/tasks/finetune',
      },
      {
        name: 'finetune',
        path: '/tasks/finetune',
        component: './management/finetune',
      },
      {
        name: 'finetune.create',
        hideInMenu: true,
        path: '/tasks/finetune/create',
        component: './management/finetune/create',
      },
      {
        name: 'finetune.edit',
        hideInMenu: true,
        path: '/tasks/finetune/:id',
        component: './management/finetune/$id',
      },
      {
        name: 'batch',
        path: '/tasks/batch',
        component: './management/batch',
      },
    ],
  },
  {
    path: '/monitor',
    name: 'monitor',
    icon: 'dashboard',
    routes: [
      {
        path: '/monitoring',
        redirect: '/monitor/cluster',
      },
      {
        path: '/monitor',
        redirect: '/monitor/cluster',
      },
      {
        // 默认：节点（Supervisor + 显卡矩阵）
        name: 'cluster',
        path: '/monitor/cluster',
        component: './monitoring/cluster',
      },
      {
        name: 'cluster.hardware',
        path: '/monitor/cluster/hardware',
        component: './monitoring/cluster',
        hideInMenu: true,
      },
      {
        // 旧节点路径 → 默认集群页
        path: '/monitor/cluster/nodes',
        redirect: '/monitor/cluster',
      },
      {
        // 原算力路径 → 节点
        path: '/monitor/cluster/gpu',
        redirect: '/monitor/cluster',
      },
      {
        // 总览已并入节点
        path: '/monitor/cluster/overview',
        redirect: '/monitor/cluster',
      },
      {
        name: 'instances',
        path: '/monitor/instances',
        component: './monitoring/instances',
      },
      {
        name: 'traffic',
        path: '/monitor/traffic',
        component: './monitoring/traffic',
      },
      {
        name: 'traffic.detail',
        path: '/monitor/traffic/:id',
        component: './monitoring/traces/[id]',
        hideInMenu: true,
      },
      {
        name: 'logs',
        path: '/monitor/logs',
        component: './monitoring/log',
      },
      {
        name: 'hami',
        path: '/monitor/hami',
        component: './monitoring/hami',
      },
    ],
  },
  {
    path: '/admin',
    name: 'admin',
    icon: 'ProfileOutlined',
    routes: [
      {
        path: '/admin',
        redirect: '/admin/users',
      },
      {
        name: 'users',
        path: '/admin/users',
        component: './admin/users',
      },
      {
        name: 'roles',
        path: '/admin/roles',
        component: './admin/roles',
      },
      {
        name: 'secretKey',
        path: '/admin/secretKey',
        component: './admin/secretKey',
      },
    ],
  },
  {
    // 无侧栏入口：手输 URL。页内 canManageSystemSettings 门控。
    path: '/internal/flags',
    name: 'internal.flags',
    component: './internal/flags',
    hideInMenu: true,
  },
  {
    path: '/',
    redirect: '/dashboard',
  },
  {
    path: '/403',
    component: './403',
  },
  {
    path: '*',
    component: './404',
  },
];
