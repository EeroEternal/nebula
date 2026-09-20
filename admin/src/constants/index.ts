import { ModalFormProps } from '@ant-design/pro-components';
import type { GlobalConfig } from '@/types/global';

export const DATE_FORMAT = 'YYYY-MM-DD';
export const DATETIME_FORMAT = 'YYYY-MM-DD HH:mm:ss';

export const ENABLE_MODEL_TIP = 'enableModelTip';

/**
 * REQUEST METHOD CONSTANTS
 */
export enum METHOD_CONSTANTS {
  GET = 'GET',
  POST = 'POST',
  PUT = 'PUT',
  DELETE = 'DELETE',
}

/**
 * ALL TABLE COLUMN of status
 * @constructor
 */
export const STATUS_MAPPING = () => {
  return [
    {
      text: '已启用',
      value: 1,
    },
    {
      text: '已禁用',
      value: 0,
    },
  ];
};

/**
 * ALL TABLE COLUMN of status enum
 * @constructor
 */
export const STATUS_ENUM = () => {
  return {
    true: { text: '已启用', status: 'Success' },
    false: { text: '已禁用', status: 'Error' },
  };
};

export const RESPONSE_CODE = {
  SUCCESS: 2000,
  ERROR: 1,
};

/**
 * the form layout of public
 */
export const FORM_LAYOUT_PUBLIC = {
  labelCol: { span: 5 },
  wrapperCol: { span: 15 },
};

export const FORM_LAYOUT_LABEL_6 = {
  labelCol: { span: 6 },
  wrapperCol: { span: 18 },
};
/**
 * the modal form layout of public
 */
export const MODAL_FORM_STYLE = {
  width: '55%',
  style: {
    maxHeight: '70vh',
    overflowY: 'auto',
  },
};

export const PRO_LIST_CARD_META = {
  title: {},
  subTitle: {},
  type: {},
  avatar: {},
  content: {},
  description: {},
  actions: {
    cardActionProps: 'actions',
  },
};

export const PRO_LIST_CARD_OPTIONS = {
  search: false,
  metas: PRO_LIST_CARD_META,
  size: 'small',
  pagination: {
    defaultPageSize: 15,
    hideOnSinglePage: true,
  },
  grid: { gutter: 24, column: 5 },
};

/**
 * the protable layout of public
 */
export const PROTABLE_OPTIONS_PUBLIC = {
  pagination: {
    defaultPageSize: 10,
    hideOnSinglePage: true,
    showQuickJumper: false,
    showSizeChanger: true,
    position: ['bottomCenter'],
  },
  ghost: false,
  rowKey: 'id',
  size: 'small',
  scroll: {
    y: 'auto',
    x: 1100,
  },
  search: {
    labelWidth: 80, // must be number
    span: 6,
  },
};

/**
 * the modal layout of public
 */
export const NORMAL_MODAL_OPTIONS = {
  width: '50%',
  styles: {
    body: { padding: '20px 10px 10px' },
  },
  destroyOnClose: true,
  maskClosable: false,
};

/**
 * the modal layout of public
 */
export const MODAL_FORM_OPTIONS: ModalFormProps = {
  width: '50%',
};

/**
 * the modal layout of public
 */
export const NORMAL_TABLE_OPTIONS = {
  pagination: {
    defaultPageSize: 6,
    hideOnSinglePage: true,
  },
  rowKey: 'id',
  style: {
    scrollY: 'auto',
    scrollX: 'auto',
  },
};

export const SWITCH_OPTIONS = () => {
  return {
    checkedChildren: '已启用',
    unCheckedChildren: '已禁用',
  };
};

export const DIALECT = {
  JAVA: 'java',
  FLINK_SQL: 'flinksql',
  LOG: 'log',
  XML: 'xml',
  MD: 'md',
  MDX: 'mdx',
  MARKDOWN: 'markdown',
  SCALA: 'scala',
  PYTHON: 'py',
  PYTHON_LONG: 'python',
  YML: 'yml',
  YAML: 'yaml',
  CONF: 'conf',
  SH: 'sh',
  BASH: 'bash',
  CMD: 'cmd',
  SHELL: 'shell',
  JSON: 'json',
  SQL: 'sql',
  JAVASCRIPT: 'javascript',
  FLINKJAR: 'flinkjar',
  JAR: 'jar',
  ZIP: 'zip',
  TAR: 'tar',
  TAR_GZ: 'gz',
  FLINKSQLENV: 'flinksqlenv',
  MYSQL: 'mysql',
  ORACLE: 'oracle',
  SQLSERVER: 'sqlserver',
  POSTGRESQL: 'postgresql',
  CLICKHOUSE: 'clickhouse',
  DORIS: 'doris',
  HIVE: 'hive',
  PHOENIX: 'phoenix',
  STARROCKS: 'starrocks',
  PRESTO: 'presto',
};

export const RUN_MODE = {
  LOCAL: 'local',
  STANDALONE: 'standalone',
  YARN_SESSION: 'yarn-session',
  YARN_PER_JOB: 'yarn-per-job',
  YARN_APPLICATION: 'yarn-application',
  KUBERNETES_SESSION: 'kubernetes-session',
  KUBERNETES_APPLICATION: 'kubernetes-application',
  KUBERNETES_APPLICATION_OPERATOR: 'kubernetes-application-operator',
};
/** 日志-功能模块 */
export enum LOG_MODULE_TYPE {
  users = '用户管理',
  models = '模型管理',
  instances = '实例管理',
  secrets = '密钥管理',
  roles = '角色管理',
  tasks = '微调任务',
  caches = '缓存管理',
  device = '设备管理',
}
/** 日志-操作类型 */
export enum LOG_OP_TYPE {
  read = '查看',
  list = '列表',
  start = '启动',
  stop = '停止',
  register = '注册',
  unregister = '取消注册',
  add = '创建',
  modify = '修改',
  delete = '删除',
  signin = '登入',
  logout = '登出',
}

export const strategyOptions = [
  {
    label: 'waiting',
    value: 'waiting',
  },
  {
    label: 'abort',
    value: 'abort',
  },
];

export const NODE_TYPE_OPTIONS = [
  {
    value: 'supervisor',
    label: 'supervisor',
  },
  {
    value: 'worker',
    label: 'worker',
  },
];

export const LOGIN_PATH = '/login';
export const DASHBOARD_PATH = '/dashboard';

export const LANGUAGE_PRIORITY = { 'zh-CN': 1, 'zh-TW': 2, 'en-US': 3, 'ko-KR': 4, 'ja-JP': 5 };
/** 收起侧边栏 */
export const COLLAPSE_SIDER = ['/management/drawingTool'];
/** 隐藏面包屑 */
export const HIDDEN_BREADCRUMB = ['/management/drawingTool', '/dashboard'];

/** 菜单权限（对话链路始终可见；未配置 xtrace 时页内空态引导） */
export const ROUTER_ACCESS_MAP: { pathPrefix: string; accessKey: keyof GlobalConfig }[] = [];

/** 获取全部 带有分页的列表的参数 */
export const ALL_LIST_PAGES_PARAMS = {
  curPageNum: -1,
  numPerPage: -1,
};

export const IO_HREF = 'https://model.xinference.io/';
export const IO_USER_CENTER_HREF = 'https://model.xinference.io/user-center';
export const IO_USER_KEY_HREF = 'https://model.xinference.io/user-center/key';
export const IO_MODELS_HREF = 'https://model.xinference.io/models';
export const IO_MODELS_EXAMPLE_QWEN3 = 'https://model.xinference.io/models/detail/qwen3';

export enum SETTING_MODAL_TABS {
  DATABASE = 'Database',
  AUTH = 'Auth',
  MONITORING = 'Monitoring',
  LICENSE = 'License',
  MODEL_HUB = 'ModelHub',
}
/** 图表颜色 */
export const CHART_COLORS = [
  'hsl(var(--chart-1))',
  'hsl(var(--chart-2))',
  'hsl(var(--chart-3))',
  'hsl(var(--chart-4))',
  'hsl(var(--chart-5))',
  'hsl(var(--chart-6))',
  'hsl(var(--chart-7))',
  'hsl(var(--chart-8))',
  'hsl(var(--chart-9))',
  'hsl(var(--chart-10))',
];
/** 字节转GB */
export const BYTES_IN_GB = Math.pow(1024, 3);

/** 不限 */
export const UNLIMITED = 'all';

export const TOOLTIP_STYLE = {
  backgroundColor: '#fff',
  border: 'none',
  borderRadius: '12px',
  boxShadow: '0 4px 16px -4px hsl(var(--text-default) / 0.1)',
  color: 'hsl(var(--text-default))',
};
