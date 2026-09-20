import { SETTING_MODAL_TABS } from '@/constants';

export interface LicenseStatus {
  valid: boolean;
  state: 'valid' | 'expired' | 'invalid' | 'missing' | 'unknown';
  expire_at?: string | null;
  message?: string | null;
}

export interface GlobalConfig {
  monitor_address?: string;
  langfuse_url?: string;
  langfuse_token?: string;
  enable_langfuse?: boolean;
  show_setting_guide?: boolean;
}
export interface AppNotificationItem {
  id: string;
  type: 'deploy_ready' | 'supervisor_change' | 'runtime_mismatch';
  title: string;
  desc?: string;
  modelUid?: string;
  modelName?: string;
  createdAt: number;
}

export interface DeployReadyPromptState {
  modelUid: string;
  modelName?: string;
}

export interface ModelInitialState {
  settings: LayoutSettings;
  currentUser?: API.CurrentUser;
  globalConfig: GlobalConfig;
  loading: boolean;
  globalReady: boolean;
  licenseModalVisible: boolean;
  showSupervisorChangeTips: boolean;
  supervisorTransferTime?: string;
  licenseStatus?: LicenseStatus;
  collapsed: boolean;
  version?: string;
  /** Full git SHA from /v1/cluster/version for checkout / copy */
  versionRevision?: string;
  settingModalVisible: boolean;
  settingModalActiveTab: SETTING_MODAL_TABS;
  /** Top-bar notification center (persisted) */
  appNotifications?: AppNotificationItem[];
  /** Deploy-ready warmup prompt */
  deployReadyPrompt?: DeployReadyPromptState | null;
}

export interface OauthProviderItem {
  /** 标识当前 provider 的名称 */
  name: string;
  /** 客户端ID 唯一标识符 */
  client_id: string;
  /** 与 client_id 配套的密钥 */
  client_secret: string;
  /** 用户认证与授权时重定向到的 URL,用户在该页面进行登录/授权（只提供链接即可，不需拼接参数）*/
  authorize_url: string;
  /** 请求授权时需要的权限范围 (可选) */
  scope?: string;
  /** 请求的响应类型 */
  response_type?: 'code';
  /* 用于 token_endpoint 请求时表明授权类型 (可选，可默认为authorization_code)
  授权码模式: authorization_code、密码(风险高):password、凭证式(无前端，命令行应用):client_credentials、隐藏式(无后端):token、token更新:refresh_token **/
  grant_type?: 'authorization_code';
  /** 用于将授权码（code）换取 access token 的接口地址（只提供链接即可，不需拼接参数）*/
  token_endpoint: string;
  /**
   * 通过 access token 获取用户信息的接口地址,可选
   * (如果仅需要用户名，过期时间，则可以token_endpoint返回的access_token配合client_secret，来解析，此处配置可以忽略)
   */
  userinfo_endpoint?: string;
}
