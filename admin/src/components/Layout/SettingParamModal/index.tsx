import { useState } from 'react';
import { Modal, Tabs, Form, Row, Col, Spin, message, App, Card } from 'antd';
import type { TabsProps } from 'antd';
import {
  ProFormText,
  ProFormTextArea,
  ProFormDigit,
  ProTable,
  ProFormList,
  ProForm,
} from '@ant-design/pro-components';
import { SafetyCertificateFilled, CloseCircleTwoTone } from '@ant-design/icons';
import { Trash2, Undo2, RefreshCw, Copy } from 'lucide-react';
import { clearCache, useRequest } from 'ahooks';
import { useModel, history } from '@umijs/max';
import { pick, size } from 'lodash';
import cn from 'classnames';

import request from '@/utils/request';
import { l, lGet } from '@/utils/intl';
import { IconFont, IconButton } from '@/components';
import { deleteLocal, copyToClipboard, canManageSystemSettings } from '@/utils';
import PermissionDenied from '@/components/PermissionDenied';
import { LOGIN_PATH, IO_HREF, IO_USER_CENTER_HREF, IO_USER_KEY_HREF, SETTING_MODAL_TABS } from '@/constants';
import type { ModelInitialState } from '@/types/global';

const PROMETHEUS_URL_DEFAULT = 'http://localhost:9090';
const GRAFANA_URL_DEFAULT = 'http://127.0.0.1:3000';

type SettingsTabDetail = {
  database_password_configured?: boolean;
  auth_secret_configured?: boolean;
  langfuse_secret_configured?: boolean;
  langfuse_public_configured?: boolean;
  licenses?: Record<string, unknown>[];
  bind_network?: {
    mac_address?: string;
    interface?: string;
    ip_address?: string;
  };
  mac_address?: string;
};

/** 已配置密钥：占位「已隐藏」；未配置：占位「请输入」 */
const secretPlaceholder = (configured?: boolean) =>
  configured
    ? l('app.settings.modal.secret.placeholder.hidden')
    : l('app.settings.modal.secret.placeholder.empty');

const SettingsTabIntro = ({ text }: { text: string }) => (
  <div className="settings-tab-intro">{text}</div>
);

const ServiceLinkStatus = ({
  connected,
  loading,
}: {
  connected?: boolean | null;
  loading?: boolean;
}) => {
  if (loading || connected == null) {
    return (
      <span className="settings-monitor-status is-checking">
        {l('app.settings.modal.monitoring.status.checking')}
      </span>
    );
  }
  return (
    <span
      className={cn(
        'settings-monitor-status',
        connected ? 'is-connected' : 'is-disconnected',
      )}
    >
      <span className="settings-monitor-status-dot" />
      {connected
        ? l('app.settings.modal.monitoring.status.connected')
        : l('app.settings.modal.monitoring.status.disconnected')}
    </span>
  );
};

const settingConfigForActiveTab = {
  [SETTING_MODAL_TABS.DATABASE]: {
    detailApiUrl: '/setting/database',
    saveApiUrl: '/setting/database',
    formKeys: [
      'database_name',
      'database_dialect',
      'database_url',
      'database_port',
      'database_username',
      'database_password',
      'database_driver',
    ],
  },
  [SETTING_MODAL_TABS.AUTH]: {
    detailApiUrl: '/setting/auth',
    saveApiUrl: '/setting/auth',
    formKeys: ['auth_algorithm', 'auth_token_default_expire', 'auth_secret_key'],
  },
  [SETTING_MODAL_TABS.MONITORING]: {
    detailApiUrl: '/setting/langfuse',
    saveApiUrl: '/setting/langfuse',
    formKeys: [
      'langfuse_host',
      'langfuse_secret_key',
      'langfuse_public_key',
      'prometheus_url',
      'grafana_url',
    ],
  },
  [SETTING_MODAL_TABS.LICENSE]: {
    detailApiUrl: '/license',
    saveApiUrl: '/license',
    formKeys: ['mac_address', 'expire_time', 'auth_license_key'],
  },
  [SETTING_MODAL_TABS.MODEL_HUB]: {
    detailApiUrl: '/setting/model_hub',
    saveApiUrl: '/setting/model_hub',
    formKeys: ['api_token'],
  },
};
const SettingParamModal = () => {
  const [form] = Form.useForm();
  const { modal } = App.useApp();
  const { initialState, setInitialState } = useModel('@@initialState');
  const { settingModalVisible, settingModalActiveTab } = initialState as ModelInitialState;
  const canSystemSettings = canManageSystemSettings(initialState?.currentUser);
  const canLoadActiveTab =
    settingModalActiveTab === SETTING_MODAL_TABS.MODEL_HUB || canSystemSettings;
  const [monitorStatus, setMonitorStatus] = useState<{
    xtrace: boolean | null;
    prometheus: boolean | null;
    grafana: boolean | null;
  }>({ xtrace: null, prometheus: null, grafana: null });
  const [databaseStatus, setDatabaseStatus] = useState<boolean | null>(null);

  const { loading: monitorStatusLoading, run: probeMonitorStatus } = useRequest(
    async (values?: Record<string, unknown>) => {
      const v = values || form.getFieldsValue();
      return request('/setting/monitoring/status', {
        method: 'post',
        data: {
          langfuse_host: v.langfuse_host,
          langfuse_secret_key: v.langfuse_secret_key,
          langfuse_public_key: v.langfuse_public_key,
          prometheus_url: v.prometheus_url,
          grafana_url: v.grafana_url,
        },
      });
    },
    {
      manual: true,
      onSuccess: (res: { success?: boolean; data?: { data?: Record<string, unknown> } }) => {
        if (!res?.success) {
          setMonitorStatus({ xtrace: false, prometheus: false, grafana: false });
          return;
        }
        const data = (res?.data?.data || {}) as {
          xtrace?: { connected?: boolean };
          prometheus?: { connected?: boolean };
          grafana?: { connected?: boolean };
        };
        setMonitorStatus({
          xtrace: !!data?.xtrace?.connected,
          prometheus: !!data?.prometheus?.connected,
          grafana: !!data?.grafana?.connected,
        });
      },
      onError: () => {
        setMonitorStatus({ xtrace: false, prometheus: false, grafana: false });
      },
    },
  );

  const { loading: databaseStatusLoading, run: probeDatabaseStatus } = useRequest(
    async (values?: Record<string, unknown>) => {
      const v = values || form.getFieldsValue();
      return request('/setting/database/status', {
        method: 'post',
        data: {
          database_dialect: v.database_dialect,
          database_driver: v.database_driver,
          database_name: v.database_name,
          database_password: v.database_password,
          database_port: v.database_port,
          database_url: v.database_url,
          database_username: v.database_username,
        },
      });
    },
    {
      manual: true,
      onSuccess: (res: {
        success?: boolean;
        data?: { data?: { database?: { connected?: boolean } } };
      }) => {
        if (!res?.success) {
          setDatabaseStatus(false);
          return;
        }
        setDatabaseStatus(!!res?.data?.data?.database?.connected);
      },
      onError: () => {
        setDatabaseStatus(false);
      },
    },
  );

  const {
    loading: initLoading,
    data,
    run: init,
  } = useRequest(
    async () => {
      if (settingModalActiveTab === SETTING_MODAL_TABS.MONITORING) {
        const [langfuseRes, envRes] = await Promise.all([
          request('/setting/langfuse'),
          request('/setting/env'),
        ]);
        return { success: langfuseRes.success && envRes.success, langfuseRes, envRes };
      }
      return request(settingConfigForActiveTab[settingModalActiveTab].detailApiUrl);
    },
    {
      refreshDeps: [settingModalActiveTab, canLoadActiveTab],
      ready: !!settingModalActiveTab && settingModalVisible && canLoadActiveTab,
      onSuccess: (res: {
        success?: boolean;
        langfuseRes?: { data?: { data?: Record<string, unknown> } };
        envRes?: { data?: { data?: { env?: Record<string, string> } } };
        data?: { data?: Record<string, unknown> };
      }) => {
        if (!res?.success) return;
        if (settingModalActiveTab === SETTING_MODAL_TABS.MONITORING) {
          const langfuseData = res.langfuseRes?.data?.data || {};
          const langfuse = pick(langfuseData, [
            'langfuse_host',
            'langfuse_secret_key',
            'langfuse_public_key',
          ]);
          // Do not bind masked secrets into the form; empty = keep existing on save.
          if (langfuseData.langfuse_secret_configured) {
            langfuse.langfuse_secret_key = undefined;
          }
          if (langfuseData.langfuse_public_configured) {
            langfuse.langfuse_public_key = undefined;
          }
          const envData = res.envRes?.data?.data?.env || {};
          const prometheusUrl =
            envData.POWERLLM_PROMETHEUS_URL || PROMETHEUS_URL_DEFAULT;
          const grafanaUrl = envData.POWERLLM_GRAFANA_URL || GRAFANA_URL_DEFAULT;
          const values = {
            ...langfuse,
            prometheus_url: prometheusUrl,
            grafana_url: grafanaUrl,
          };
          form.setFieldsValue(values);
          setMonitorStatus({ xtrace: null, prometheus: null, grafana: null });
          probeMonitorStatus(values);
          return;
        }
        const detail = res?.data?.data || {};
        const initValues = pick(
          detail,
          settingConfigForActiveTab[settingModalActiveTab].formKeys,
        );
        if (detail.auth_secret_configured) {
          initValues.auth_secret_key = undefined;
        }
        if (detail.database_password_configured) {
          initValues.database_password = undefined;
        }
        form.setFieldsValue(initValues);
        if (settingModalActiveTab === SETTING_MODAL_TABS.DATABASE) {
          setDatabaseStatus(null);
          probeDatabaseStatus({ ...detail, ...initValues });
        }
      },
    },
  );

  const tabDetail: SettingsTabDetail =
    settingModalActiveTab === SETTING_MODAL_TABS.MONITORING
      ? ((data?.langfuseRes?.data?.data || {}) as SettingsTabDetail)
      : ((data?.data?.data || {}) as SettingsTabDetail);

  const refreshLicenseStatus = () => {
    // 保存/删除后：刷新设置页列表 + 全局横幅（/status.license），无需整页 reload
    void request('/status', { skipNotification: true })
      .then((res) => {
        const licenseStatus = res?.data?.data?.license || res?.data?.license;
        if (licenseStatus) {
          setInitialState(
            (prev) => ({ ...prev, licenseStatus } as ModelInitialState),
          );
        }
      })
      .catch(() => undefined);
    init();
    form.setFieldsValue({ license_keys: [{}] });
  };

  const { loading: saveLoading, run: saveSetting } = useRequest(
    (data, options?: { method: string }) =>
      request(settingConfigForActiveTab[settingModalActiveTab].saveApiUrl, {
        method: options?.method || 'put',
        data,
      }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          if (
            settingModalActiveTab === SETTING_MODAL_TABS.LICENSE &&
            size(res?.data?.data?.invalid_licenses)
          ) {
            modal.confirm({
              icon: <CloseCircleTwoTone twoToneColor="#ff4d4f" />,
              title: lGet('global.license.updateErrorTitle'),
              content: (
                <div>
                  {res.data.data.invalid_licenses.map(
                    (item: { license_key: string; prompt: string }) => (
                      <div key={item.license_key}>
                        <div>{item.license_key}</div>
                        <div className="text-danger">{item.prompt}</div>
                      </div>
                    ),
                  )}
                </div>
              ),
              cancelText: lGet('global.license.updateErrorCancel'),
              cancelButtonProps: {
                icon: <Undo2 size={14} />,
              },
              onCancel: () => {
                refreshLicenseStatus();
              },
              okText: lGet('global.license.updateErrorOk'),
              okButtonProps: {
                icon: <RefreshCw size={14} />,
              },
              onOk: () => {
                // 部分 license 已生效：刷新本页数据与全局横幅
                refreshLicenseStatus();
              },
            });
            return;
          }

          if (settingModalActiveTab === SETTING_MODAL_TABS.AUTH) {
            setInitialState(
              (prev) => ({ ...prev, settingModalVisible: false } as ModelInitialState),
            );
            message.success(lGet('global.message.editSuccess.loginAgain'));
            deleteLocal('user');
            history.push(LOGIN_PATH);
            return;
          }
          message.success(lGet('global.message.editSuccess'));
          // 保留当前 Settings 菜单，仅重新拉取配置（不再整页 reload）
          if (settingModalActiveTab === SETTING_MODAL_TABS.MODEL_HUB) {
            clearCache('setting-model-hub');
          }
          if (settingModalActiveTab === SETTING_MODAL_TABS.LICENSE) {
            refreshLicenseStatus();
            return;
          }
          init();
          if (settingModalActiveTab === SETTING_MODAL_TABS.DATABASE) {
            probeDatabaseStatus(form.getFieldsValue());
          }
        }
      },
    },
  );

  const { loading: deletingLicense, run: deleteLicense } = useRequest(
    (payload: { license_keys?: string[]; clear_all?: boolean }) =>
      request('/license', { method: 'delete', data: payload }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res?.success === false) {
          message.error(
            res?.data?.detail ||
              res?.message ||
              String(lGet('global.message.deleteFailed')),
          );
          return;
        }
        message.success(lGet('global.license.deleteOk'));
        refreshLicenseStatus();
      },
    },
  );

  const { loading: saveMonitoringLoading, run: saveMonitoring } = useRequest(
    async (values: Record<string, unknown>) => {
      const langfusePayload = pick(values, [
        'langfuse_host',
        'langfuse_secret_key',
        'langfuse_public_key',
      ]);
      const prometheusUrl =
        (typeof values.prometheus_url === 'string' ? values.prometheus_url.trim() : '') ||
        PROMETHEUS_URL_DEFAULT;
      const grafanaUrl =
        (typeof values.grafana_url === 'string' ? values.grafana_url.trim() : '') ||
        GRAFANA_URL_DEFAULT;
      const [langfuseRes, envRes] = await Promise.all([
        request('/setting/langfuse', { method: 'put', data: langfusePayload }),
        request('/setting/env', {
          method: 'put',
          data: {
            env: {
              POWERLLM_PROMETHEUS_URL: prometheusUrl,
              POWERLLM_GRAFANA_URL: grafanaUrl,
            },
          },
        }),
      ]);
      return { langfuseRes, envRes };
    },
    {
      manual: true,
      onSuccess: (res) => {
        if (res?.langfuseRes?.success && res?.envRes?.success) {
          message.success(lGet('global.message.editSuccess'));
          const values = form.getFieldsValue();
          init();
          probeMonitorStatus(values);
          return;
        }
        message.error(lGet('global.message.updateError'));
      },
    },
  );

  const tabsItems = [
    {
      key: SETTING_MODAL_TABS.DATABASE,
      label: l('app.settings.modal.database'),
      icon: <IconFont name="icon-database-2-fill" />,
      children: (
        <Spin spinning={initLoading}>
          <SettingsTabIntro text={l('app.settings.modal.database.desc')} />
          <div className="settings-monitor">
            <Card
              size="small"
              title={
                <span className="settings-monitor-card-title">
                  {l('app.settings.modal.database.connection')}
                  <span className="settings-monitor-card-sub">
                    {l('app.settings.modal.database.connection.sub')}
                  </span>
                </span>
              }
              extra={
                <ServiceLinkStatus
                  connected={databaseStatus}
                  loading={databaseStatusLoading}
                />
              }
              className="settings-monitor-card"
            >
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="database_name"
                    label={l('app.settings.modal.database.name')}
                  />
                </Col>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="database_dialect"
                    label={l('app.settings.modal.database.dialect')}
                  />
                </Col>
              </Row>
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="database_url"
                    label={l('app.settings.modal.database.url')}
                  />
                </Col>
                <Col span={12}>
                  <ProFormDigit
                    rules={[{ required: true }]}
                    name="database_port"
                    label={l('app.settings.modal.database.port')}
                  />
                </Col>
              </Row>
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="database_username"
                    label={l('app.settings.modal.database.username')}
                  />
                </Col>
                <Col span={12}>
                  <ProFormText.Password
                    fieldProps={{
                      placeholder: secretPlaceholder(
                        tabDetail?.database_password_configured,
                      ),
                    }}
                    rules={[
                      {
                        required: !tabDetail?.database_password_configured,
                      },
                      {
                        validator: async (_, value) => {
                          if (!value) return;
                          if (value === '********') return;
                          if (String(value).length < 6) {
                            throw new Error('Password must be at least 6 characters long');
                          }
                        },
                      },
                    ]}
                    name="database_password"
                    label={l('app.settings.modal.database.password')}
                  />
                </Col>
              </Row>
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="database_driver"
                    label={l('app.settings.modal.database.driver')}
                  />
                </Col>
              </Row>
            </Card>
          </div>
        </Spin>
      ),
    },
    {
      key: SETTING_MODAL_TABS.AUTH,
      label: l('app.settings.modal.auth'),
      icon: <IconFont name="icon-auth" />,
      children: (
        <Spin spinning={initLoading}>
          <SettingsTabIntro text={l('app.settings.modal.auth.desc')} />
          <ProFormText
            rules={[{ required: true }]}
            name="auth_algorithm"
            label={l('app.settings.modal.auth.algorithm')}
          />
          <ProFormDigit
            rules={[{ required: true }]}
            name="auth_token_default_expire"
            label={l('app.settings.modal.auth.tokenDefaultExpire')}
          />
          <ProFormTextArea
            fieldProps={{
              autoSize: { minRows: 2 },
              placeholder: secretPlaceholder(tabDetail?.auth_secret_configured),
            }}
            rules={[
              {
                required: !tabDetail?.auth_secret_configured,
              },
            ]}
            name="auth_secret_key"
            label={l('app.settings.modal.auth.secretKey')}
          />
        </Spin>
      ),
    },
    {
      key: SETTING_MODAL_TABS.MONITORING,
      label: l('app.settings.modal.monitoring'),
      icon: <IconFont name="icon-icon" />,
      children: (
        <Spin spinning={initLoading}>
          <SettingsTabIntro text={l('app.settings.modal.monitoring.desc')} />
          <div className="settings-monitor">
            <Card
              size="small"
              title={
                <span className="settings-monitor-card-title">
                  {l('app.settings.modal.monitoring.xtrace')}
                  <span className="settings-monitor-card-sub">
                    {l('app.settings.modal.monitoring.xtrace.sub')}
                  </span>
                </span>
              }
              extra={
                <ServiceLinkStatus
                  connected={monitorStatus.xtrace}
                  loading={monitorStatusLoading}
                />
              }
              className="settings-monitor-card"
            >
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="langfuse_host"
                    label={l('app.settings.modal.monitoring.xtrace.host')}
                  />
                </Col>
                <Col span={12}>
                  <ProFormText
                    fieldProps={{
                      placeholder: secretPlaceholder(
                        tabDetail?.langfuse_secret_configured,
                      ),
                    }}
                    rules={[
                      {
                        required: !tabDetail?.langfuse_secret_configured,
                      },
                    ]}
                    name="langfuse_secret_key"
                    label={l('app.settings.modal.monitoring.xtrace.secretKey')}
                  />
                </Col>
              </Row>
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    fieldProps={{
                      placeholder: secretPlaceholder(
                        tabDetail?.langfuse_public_configured,
                      ),
                    }}
                    rules={[
                      {
                        required: !tabDetail?.langfuse_public_configured,
                      },
                    ]}
                    name="langfuse_public_key"
                    label={l('app.settings.modal.monitoring.xtrace.publicKey')}
                  />
                </Col>
              </Row>
            </Card>
            <Card
              size="small"
              title={
                <span className="settings-monitor-card-title">
                  {l('app.settings.modal.monitoring.prometheus')}
                  <span className="settings-monitor-card-sub">
                    {l('app.settings.modal.monitoring.prometheus.sub')}
                  </span>
                </span>
              }
              extra={
                <ServiceLinkStatus
                  connected={monitorStatus.prometheus}
                  loading={monitorStatusLoading}
                />
              }
              className="settings-monitor-card"
            >
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="prometheus_url"
                    label={l('app.settings.modal.monitoring.prometheus.url')}
                    placeholder={PROMETHEUS_URL_DEFAULT}
                  />
                </Col>
              </Row>
            </Card>
            <Card
              size="small"
              title={
                <span className="settings-monitor-card-title">
                  {l('app.settings.modal.monitoring.grafana')}
                  <span className="settings-monitor-card-sub">
                    {l('app.settings.modal.monitoring.grafana.sub')}
                  </span>
                </span>
              }
              extra={
                <ServiceLinkStatus
                  connected={monitorStatus.grafana}
                  loading={monitorStatusLoading}
                />
              }
              className="settings-monitor-card"
            >
              <Row gutter={16}>
                <Col span={12}>
                  <ProFormText
                    rules={[{ required: true }]}
                    name="grafana_url"
                    label={l('app.settings.modal.monitoring.grafana.url')}
                    placeholder={GRAFANA_URL_DEFAULT}
                  />
                </Col>
              </Row>
            </Card>
          </div>
        </Spin>
      ),
    },
    {
      key: SETTING_MODAL_TABS.LICENSE,
      label: l('app.settings.modal.license'),
      icon: <SafetyCertificateFilled />,
      children: (
        <Spin
          spinning={initLoading || deletingLicense}
          className="license-settings-panel"
          wrapperClassName="license-settings-panel"
        >
          {/* 列表+网口可滚动；新增 License 固定在底部始终可见 */}
          <div className="license-settings-panel__body">
            <div className="license-settings-panel__scroll">
              <SettingsTabIntro text={l('app.settings.modal.license.desc')} />
              <ProTable
                search={false}
                options={false}
                bordered={true}
                className="license-settings-table"
                columns={[
                  {
                    dataIndex: 'license_key',
                    title: l('global.license.number'),
                    width: 56,
                    render: (_, record, index) => index + 1,
                  },
                  {
                    dataIndex: 'license_key',
                    title: 'License',
                    render: (dom) => (
                      <div className="w-[100px] flex items-center">
                        <div className="truncate flex-1">{dom}</div>
                        <IconButton
                          className="!w-6 !h-6 hover:text-primary hover:bg-primary/15"
                          onClick={() => copyToClipboard(dom as string)}
                        >
                          <Copy size={14} />
                        </IconButton>
                      </div>
                    ),
                  },
                  {
                    dataIndex: 'expire_time',
                    title: l('global.license.validityPeriod'),
                    render: (time, record) => {
                      if (!time) return null;
                      return record.is_expired
                        ? l('global.license.expired')
                        : time;
                    },
                  },
                  {
                    dataIndex: 'auth_type',
                    title: l('global.license.licenseType'),
                    valueEnum: {
                      gpu: l('global.license.licenseType.gpu'),
                      worker: l('global.license.licenseType.worker'),
                    },
                  },
                  {
                    dataIndex: 'auth_count',
                    title: l('global.license.quantity'),
                  },
                  {
                    title: l('global.license.delete'),
                    width: 72,
                    render: (_, record) => (
                      <IconButton
                        className="!w-6 !h-6 rounded-md group"
                        aria-label={String(lGet('global.license.delete'))}
                        onClick={() => {
                          modal.confirm({
                            title: lGet('global.license.delete'),
                            content: lGet('global.license.deleteConfirm'),
                            okButtonProps: { danger: true },
                            onOk: () =>
                              deleteLicense({
                                license_keys: [String(record.license_key)],
                              }),
                          });
                        }}
                      >
                        <Trash2
                          size={14}
                          className="group-hover:text-danger text-muted"
                        />
                      </IconButton>
                    ),
                  },
                ]}
                dataSource={tabDetail?.licenses || []}
                pagination={false}
                size="small"
                locale={{
                  emptyText: (
                    <div className="py-3 text-muted text-sm">
                      {l('global.data.empty')}
                    </div>
                  ),
                }}
              />
              <div className="mt-3 mb-1 rounded-md border border-[color:var(--c-border-light)] bg-[var(--c-surface-2)]/40 px-3 py-2.5">
                <div className="text-xs text-muted mb-2">
                  {l('global.license.bindNetwork')}
                </div>
                {tabDetail?.bind_network?.mac_address ||
                tabDetail?.mac_address ? (
                  <div className="flex flex-wrap gap-2">
                    {(tabDetail?.bind_network?.interface ||
                    tabDetail?.bind_network?.ip_address
                      ? [
                          {
                            label: l('global.license.interface'),
                            value: tabDetail?.bind_network?.interface || '-',
                          },
                          {
                            label: l('global.license.mac'),
                            value:
                              tabDetail?.bind_network?.mac_address ||
                              tabDetail?.mac_address ||
                              '-',
                          },
                          {
                            label: l('global.license.ip'),
                            value: tabDetail?.bind_network?.ip_address || '-',
                          },
                        ]
                      : [
                          {
                            label: l('global.license.nodeId'),
                            value:
                              tabDetail?.bind_network?.mac_address ||
                              tabDetail?.mac_address ||
                              '-',
                          },
                        ]
                    ).map((item) => (
                      <button
                        key={item.label}
                        type="button"
                        className="inline-flex items-center gap-1.5 rounded border border-[color:var(--c-border-light)] bg-white px-2 py-1 text-xs text-default hover:border-primary hover:text-primary cursor-pointer"
                        title={String(lGet('global.actions.copy'))}
                        onClick={() => {
                          if (!item.value || item.value === '-') return;
                          copyToClipboard(String(item.value));
                        }}
                      >
                        <span className="text-muted">{item.label}</span>
                        <span className="font-mono">{item.value}</span>
                        <Copy size={12} className="text-muted shrink-0" />
                      </button>
                    ))}
                  </div>
                ) : (
                  <div className="text-xs text-muted">
                    {l('global.license.bindNetwork.empty')}
                  </div>
                )}
              </div>
            </div>
            <div className="license-settings-panel__footer">
              <ProFormList
                label={l('global.license.addLicense')}
                initialValue={[{}]}
                name="license_keys"
                copyIconProps={false}
                creatorButtonProps={false}
                deleteIconProps={false}
                actionRender={() => []}
                min={1}
                max={1}
                className="w-full-pro-form-list !mb-0"
              >
                {() => (
                  <ProFormText
                    name="value"
                    fieldProps={{ className: 'w-[400px]' }}
                  />
                )}
              </ProFormList>
            </div>
          </div>
        </Spin>
      ),
    },
    {
      key: SETTING_MODAL_TABS.MODEL_HUB,
      label: l('app.settings.modal.modelHub'),
      icon: <IconFont name="icon-powerllm" />,
      children: (
        <Spin spinning={initLoading}>
          <SettingsTabIntro text={l('app.settings.modal.modelHub.desc')} />
          <div className="settings-tab-intro mb-[20px]">
            {l('app.settings.modal.modelHub.tips', undefined, {
              modelHub: (
                <a
                  href={IO_HREF}
                  target="_blank"
                  className="text-primary hover:text-primary/60"
                  rel="noreferrer"
                >
                  {l('app.settings.modal.modelHub')}
                </a>
              ),
              userCenter: (
                <a
                  href={IO_USER_CENTER_HREF}
                  target="_blank"
                  className="text-primary hover:text-primary/60"
                  rel="noreferrer"
                >
                  {l('app.settings.modal.modelHub.userCenter')}
                </a>
              ),
              key: (
                <a
                  href={IO_USER_KEY_HREF}
                  target="_blank"
                  className="text-primary hover:text-primary/60"
                  rel="noreferrer"
                >
                  {l('app.settings.modal.modelHub.key')}
                </a>
              ),
            })}
          </div>
          <ProFormTextArea
            fieldProps={{
              autoSize: { minRows: 2 },
            }}
            name="api_token"
            label={l('app.settings.modal.modelHub.apiToken')}
            rules={[{ required: true }]}
            extra={l('app.settings.modal.modelHub.apiToken.extra')}
          />
        </Spin>
      ),
    },
  ];

  const closeModal = () => {
    setInitialState((prev) => ({ ...prev, settingModalVisible: false } as ModelInitialState));
  };
  const tabChange: TabsProps['onChange'] = (key) => {
    setInitialState(
      (prev) =>
        ({ ...prev, settingModalActiveTab: key as SETTING_MODAL_TABS } as ModelInitialState),
    );
  };
  const trimStringValues = (obj: Record<string, unknown>) => {
    return Object.fromEntries(
      Object.entries(obj).map(([key, value]) => [
        key,
        typeof value === 'string' ? value.trim() : value,
      ]),
    );
  };
  const onFinish = () => {
    if (!canLoadActiveTab) return;
    form.validateFields().then((values) => {
      // 去除空格；密钥类缺省为空串，表示服务端「保持原值」
      const newValues = trimStringValues({
        ...values,
        auth_secret_key: values.auth_secret_key ?? '',
        database_password: values.database_password ?? '',
        langfuse_secret_key: values.langfuse_secret_key ?? '',
        langfuse_public_key: values.langfuse_public_key ?? '',
      });
      if (settingModalActiveTab === SETTING_MODAL_TABS.LICENSE) {
        const license_keys = values.license_keys
          .map((item: { value: string }) => item.value && item.value.trim())
          .filter(Boolean);
        if (!license_keys.length) return;
        saveSetting({ license_keys }, { method: 'post' });
      } else if (settingModalActiveTab === SETTING_MODAL_TABS.MONITORING) {
        saveMonitoring(newValues);
      } else {
        saveSetting(newValues);
      }
    });
  };

  return (
    <Modal
      width={720}
      open={settingModalVisible}
      title={l('app.settings.modal.title')}
      okText={l('global.actions.save')}
      onCancel={closeModal}
      onOk={canLoadActiveTab ? onFinish : undefined}
      okButtonProps={{
        loading: saveLoading || saveMonitoringLoading,
        disabled: !canLoadActiveTab,
        style: canLoadActiveTab ? undefined : { display: 'none' },
      }}
      footer={(originNode, { CancelBtn }) => (
        <div className="flex items-center justify-between">
          <span
            className="text-muted text-xs cursor-pointer select-all"
            title={
              initialState?.versionRevision
                ? l('app.settings.modal.version.copyHint')
                : undefined
            }
            onClick={() => {
              const rev = initialState?.versionRevision;
              if (!rev) return;
              copyToClipboard(rev);
              message.success(l('app.settings.modal.version.copied'));
            }}
          >
            {initialState?.version}
          </span>
          {/* 确定在左、取消在右（与全局 Modal 约定一致） */}
          <div className="flex flex-row-reverse gap-2">
            {canLoadActiveTab ? originNode : <CancelBtn />}
          </div>
        </div>
      )}
    >
      <ProForm
        layout={settingModalActiveTab === SETTING_MODAL_TABS.LICENSE ? 'horizontal' : 'vertical'}
        form={form}
        autoComplete="off"
        submitter={false}
      >
        <Tabs
          destroyOnHidden
          className="antd-modal-tabs-left"
          activeKey={settingModalActiveTab}
          onChange={tabChange}
          tabPosition="left"
          items={tabsItems.map((item) =>
            item.key === SETTING_MODAL_TABS.MODEL_HUB || canSystemSettings
              ? item
              : {
                  ...item,
                  children: <PermissionDenied scene="settings" />,
                },
          )}
        />
      </ProForm>
    </Modal>
  );
};
export default SettingParamModal;
