import { LockOutlined, UserOutlined } from '@ant-design/icons';
import { LoginForm, ProFormCheckbox, ProFormText } from '@ant-design/pro-components';
import {
  Helmet,
  SelectLang,
  setLocale,
  getLocale,
  useModel,
  history,
  useSearchParams,
} from '@umijs/max';
import { Form, message, Button, Alert, Spin, Tag } from 'antd';
import React, { useEffect, useState, KeyboardEvent } from 'react';
import { sortBy, size } from 'lodash';
import { useRequest } from 'ahooks';
import qs from 'qs';

import { readThrownMessage } from '@/utils/formatApiError';
import request from '@/utils/request';
import { setLocal, getLocal, capitalizeFirstLetter } from '@/utils';
import { l, lGet } from '@/utils/intl';
import type { ModelInitialState, OauthProviderItem } from '@/types/global';
import { LANGUAGE_PRIORITY } from '@/constants';
import { login } from '@/services/user';

const containerWidth = 360;
const requiredProviderKeys = ['client_id', 'authorize_url', 'name'];

const Login: React.FC = () => {
  const [form] = Form.useForm();
  const { setInitialState } = useModel('@@initialState');
  const language = getLocale();
  const [searchParams] = useSearchParams();
  const code = searchParams.get('code');
  const provider = searchParams.get('provider');
  const oauthState = searchParams.get('state');

  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [capsLockOn, setCapsLockOn] = useState(false);
  const [oauthError, setOauthError] = useState<string | null>(null);

  const productName = window.TITLE || 'PowerLLM';
  const envLabel = window.ENV_LABEL || '';
  const { data: versionRes } = useRequest(() => request('/cluster/version'));
  const appVersion =
    versionRes?.data?.display ||
    versionRes?.data?.version ||
    window.VERSION ||
    '';

  const { data } = useRequest(() => request('/sso/providers'), {
    onSuccess: (res) => {
      if (!res?.success) {
        const msg = res?.data?.detail || lGet('global.login.prviderError');
        setFormError(String(msg));
      }
    },
  });
  const providers = (data?.data?.data || []) as OauthProviderItem[];

  const changeLanguage = (lang: { key: string }) => {
    setLocale(lang.key, false);
  };

  const loginCallback = async (user: API.CurrentUser) => {
    try {
      const res = await request('/user/me', {
        Authorization: `${user?.token_type} ${user?.token}`,
      });
      const newUser = {
        ...user,
        ...(res?.data?.data || {}),
      };
      setLocale(newUser?.locale || language);
      setLocal('user', newUser);
      setInitialState((pre) => ({ ...pre, currentUser: newUser } as ModelInitialState));
      message.success(lGet('global.login.loginSuccess'));
      return history.push('/');
    } catch {
      setFormError(lGet('global.login.loginError') as string);
    }
  };

  const handleSubmit = async (values: API.LoginParams) => {
    const { autoLogin, ...params } = values;
    setFormError(null);
    setSubmitting(true);
    if (autoLogin) {
      params.token_expire_in_minutes = 10080;
    }
    try {
      const res = await login({
        ...params,
      });
      if (!res.success) {
        const detail = res?.data?.detail || lGet('global.login.loginError');
        setFormError(String(detail));
        return;
      }
      if (res.data.access_token) {
        await loginCallback({
          ...res.data,
          name: form.getFieldsValue().username,
          token: res.data.access_token,
        });
      }
    } catch (error: unknown) {
      setFormError(readThrownMessage(error) || (lGet('global.login.loginError') as string));
    } finally {
      setSubmitting(false);
    }
  };

  const { loading: providerLoading, run: getOAuthToken } = useRequest(
    () => request(`/sso/callback/${provider}`, { method: 'post', data: { code } }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res?.data?.access_token) {
          loginCallback({
            ...res.data,
            name: res?.data?.user_name,
            token: res.data.access_token,
          });
        } else {
          const detail = res?.data?.detail || lGet('global.login.loginError');
          setOauthError(String(detail));
          setFormError(String(detail));
        }
      },
      onError: (res) => {
        const detail = res?.message || lGet('global.login.loginError');
        setOauthError(String(detail));
        setFormError(String(detail));
      },
    },
  );

  const validateOauthKeys = (obj: OauthProviderItem) => {
    const rec = obj as unknown as Record<string, unknown>;
    const objKey = Object.keys(rec);
    return requiredProviderKeys
      .filter((item) => !(objKey.includes(item) && rec[item]))
      .join(',');
  };

  const handleOtherLoginIn = (item: OauthProviderItem) => {
    setFormError(null);
    const lackKeys = validateOauthKeys(item);
    if (!lackKeys) {
      const state = Math.random().toString(36).substring(2);
      sessionStorage.setItem('oauth_state', state);
      const params = {
        client_id: item.client_id,
        redirect_uri: location.origin + `/login?provider=${item.name}`,
        scope: item?.scope,
        response_type: 'code',
        state,
      };
      window.location.href = `${item.authorize_url}?${qs.stringify(params, {
        skipNulls: true,
      })}`;
    } else {
      setFormError(`${lGet('global.login.otherLoginLack')} ${lackKeys}`);
    }
  };

  const handleCapsLock = (e: KeyboardEvent<HTMLInputElement>) => {
    if (typeof e.getModifierState === 'function') {
      setCapsLockOn(e.getModifierState('CapsLock'));
    }
  };

  useEffect(() => {
    if (code && provider) {
      if (oauthState && oauthState !== sessionStorage.getItem('oauth_state')) {
        const msg = lGet('global.login.stateError') as string;
        setOauthError(msg);
        setFormError(msg);
        return;
      }
      getOAuthToken();
    }
  }, [code, provider, oauthState]);

  useEffect(() => {
    if (getLocal('user')) {
      history.replace('/');
    }
  }, []);

  const isOauthCallback = !!(code && provider) && providerLoading && !oauthError;

  // OAuth 回调全屏加载态
  if (isOauthCallback) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-surface-page px-4">
        <Helmet>
          <title>{lGet('menu.login')}</title>
        </Helmet>
        <Spin size="large" />
        <p className="mt-4 text-muted text-sm">
          {l('pages.login.oauthLoading')}
        </p>
        {provider && (
          <p className="mt-1 text-xs text-disabled">
            {capitalizeFirstLetter(provider)}
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="relative min-h-screen flex justify-center items-center overflow-hidden bg-surface-page">
      {/* 本地纯 CSS 品牌背景（无外部图） */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0"
        style={{
          background: `
            radial-gradient(ellipse 80% 60% at 10% 20%, hsl(var(--primary) / 0.18), transparent 55%),
            radial-gradient(ellipse 70% 50% at 90% 80%, hsl(var(--primary) / 0.12), transparent 50%),
            radial-gradient(ellipse 50% 40% at 50% 100%, hsl(var(--status-info) / 0.08), transparent 45%),
            hsl(var(--surface-page))
          `,
        }}
      />
      <div
        aria-hidden
        className="pointer-events-none absolute -top-24 -right-24 h-72 w-72 rounded-full bg-primary/10 blur-3xl"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute -bottom-32 -left-20 h-80 w-80 rounded-full bg-primary/5 blur-3xl"
      />

      <Helmet>
        <title>{lGet('menu.login')}</title>
      </Helmet>

      <div className="relative z-10 flex flex-col items-center w-full max-w-[420px] px-4 py-10">
        {/* 产品名 + 描述 + 环境/版本 */}
        <div className="mb-6 flex flex-col items-center text-center gap-2">
          <img
            className="h-14 w-auto"
            alt=""
            src={window.LOGO_BLUE || window.LOGO || '/logo-blue.png'}
          />
          <h1 className="text-2xl font-bold text-default !mb-0">{productName}</h1>
          <p className="text-sm text-muted max-w-sm">
            {l('pages.login.subtitle')}
          </p>
          {(envLabel || appVersion) && (
            <div className="flex flex-wrap items-center justify-center gap-2 mt-1">
              {envLabel && (
                <Tag className="!m-0 rounded-full" color="processing">
                  {envLabel}
                </Tag>
              )}
              {appVersion && (
                <Tag
                  className="!m-0 rounded-full"
                  title={versionRes?.data?.revision || ''}
                >
                  {l('pages.login.version')} {appVersion}
                </Tag>
              )}
            </div>
          )}
        </div>

        <div
          className="w-full rounded-2xl bg-card shadow-popover border border-border/50 px-6 py-6 sm:px-8"
          style={{ maxWidth: containerWidth + 64 }}
        >
          {(formError || oauthError) && (
            <Alert
              type="error"
              showIcon
              closable
              className="mb-4"
              message={formError || oauthError}
              onClose={() => {
                setFormError(null);
                setOauthError(null);
              }}
            />
          )}

          <LoginForm
            contentStyle={{ width: '100%', minWidth: 0 }}
            containerStyle={{ padding: 0 }}
            logo={false}
            title={false}
            subTitle={false}
            initialValues={{
              autoLogin: true,
            }}
            onFinish={async (values) => {
              await handleSubmit(values as API.LoginParams);
            }}
            form={form}
            submitter={{
              searchConfig: {
                submitText: l('global.login.login'),
              },
              submitButtonProps: {
                size: 'large',
                loading: submitting,
                block: true,
              },
            }}
          >
            <div className="space-y-1 pt-2">
              <ProFormText
                name="username"
                fieldProps={{
                  size: 'large',
                  prefix: <UserOutlined />,
                  autoComplete: 'username',
                  onChange: () => formError && setFormError(null),
                }}
                placeholder={l('pages.login.username.placeholder')}
                rules={[
                  {
                    required: true,
                    message: l('pages.login.username.required'),
                  },
                ]}
              />
              <ProFormText.Password
                name="password"
                fieldProps={{
                  size: 'large',
                  prefix: <LockOutlined />,
                  autoComplete: 'current-password',
                  onKeyDown: handleCapsLock,
                  onKeyUp: handleCapsLock,
                  onChange: () => formError && setFormError(null),
                }}
                placeholder={l('pages.login.password.placeholder')}
                rules={[
                  {
                    required: true,
                    message: l('pages.login.password.required'),
                  },
                ]}
              />
              {capsLockOn && (
                <Alert
                  type="warning"
                  showIcon
                  className="!mb-3"
                  message={l('pages.login.capsLock')}
                />
              )}
            </div>
            <div className="my-4">
              <ProFormCheckbox noStyle name="autoLogin">
                {l('pages.login.rememberMe')}
              </ProFormCheckbox>
            </div>
          </LoginForm>

          {!!size(providers) && (
            <div className="flex flex-col gap-y-2.5 mt-2">
              <div className="flex justify-between items-center gap-x-2 mb-1">
                <div className="w-full bg-border h-px" />
                <div className="whitespace-nowrap text-muted text-sm px-1">
                  {l('global.login.otherLogin')}
                </div>
                <div className="w-full bg-border h-px" />
              </div>
              {providers.map((item) => (
                <Button
                  size="large"
                  key={item.name}
                  onClick={() => handleOtherLoginIn(item)}
                  block
                  loading={item.name === provider && providerLoading}
                >
                  {capitalizeFirstLetter(item.name)}
                </Button>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="fixed top-5 right-5 z-20">
        <SelectLang
          postLocalesData={(list) =>
            sortBy(
              list,
              (item) => LANGUAGE_PRIORITY[item.lang as keyof typeof LANGUAGE_PRIORITY] || 5,
            )
          }
          key={language}
          onItemClick={changeLanguage}
        />
      </div>
    </div>
  );
};

export default Login;
