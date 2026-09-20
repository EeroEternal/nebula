import { useParams, history, useModel, useLocation } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Button, Tag, App, Empty } from 'antd';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ExternalLink, Trash2, RefreshCw, List, BookOpen, Rocket } from 'lucide-react';
import qs from 'qs';
import { isEmpty } from 'lodash';
import classNames from 'classnames';

import { formatDisplayTime } from '@/utils';
import request from '@/utils/request';
import { l, lGet } from '@/utils/intl';
import type { ModelDetailRes } from '@/types/Public/data';
import type { ModelInitialState } from '@/types/global';
import { ModelType } from '@/constants/modelData';
import DeployModelInstance from '@/components/DeployModelInstance';
import { ReactMarkdown } from '@/components';
import { SETTING_MODAL_TABS } from '@/constants';
import { PageContainer, PillTabs } from '@/components';
import ModelTags from '../components/NewModalTags';
import ModelVersions from '../components/NewModelVersions';

const DESC_COLLAPSE_LEN = 120;

const tabFromSearch = (search: string) => {
  const sp = qs.parse((search || '').replace(/^\?/, ''));
  const raw =
    !isEmpty(sp) && sp.activeTab ? String(sp.activeTab) : 'versions';
  return raw === 'yaml' || raw === 'virtualenvs' ? 'deploy' : raw;
};

const ModelDetail = () => {
  const { setInitialState } = useModel('@@initialState');
  const { modal, message } = App.useApp();
  const params = useParams();
  const location = useLocation();
  const { modelType, modelName } = params || {};
  const searchParams = qs.parse((location.search || '').replace(/^\?/, ''));
  const initActiveTab = tabFromSearch(location.search || '');
  const [activeTab, setActiveTab] = useState(initActiveTab);
  const [descExpanded, setDescExpanded] = useState(false);
  /** 访问过的 Tab 保持挂载，避免反复进入重新拉接口 */
  const [mountedTabs, setMountedTabs] = useState<Record<string, boolean>>({
    readme: initActiveTab === 'readme',
    versions: initActiveTab === 'versions',
    deploy: initActiveTab === 'deploy',
  });
  // 同页 history.push(?activeTab=deploy) 时同步 Tab（版本列表「部署此版本」）
  useEffect(() => {
    const next = tabFromSearch(location.search || '');
    setActiveTab((prev) => (prev === next ? prev : next));
  }, [location.search]);
  useEffect(() => {
    setMountedTabs((prev) => (prev[activeTab] ? prev : { ...prev, [activeTab]: true }));
  }, [activeTab]);

  const { data: modelHubRes } = useRequest(() => request('/setting/model_hub'), {
    cacheKey: 'setting-model-hub',
    staleTime: 60_000,
  });
  const modelHubApiToken = modelHubRes?.data?.data?.api_token;

  const showModelHubModal = () => {
    setInitialState(
      (prev) =>
        ({
          ...prev,
          settingModalActiveTab: SETTING_MODAL_TABS.MODEL_HUB,
          settingModalVisible: true,
        } as ModelInitialState),
    );
  };

  const { data, run } = useRequest(
    () => request(`/model_registrations/${modelType}/${modelName}`),
    {
      ready: !!(modelType && modelName),
      cacheKey: `model-reg|${modelType || ''}|${modelName || ''}`,
      staleTime: 60_000,
    },
  );

  const modelDetail = (data?.data?.data || {}) as ModelDetailRes;
  const {
    model_version_count = 1,
    model_instance_count = 0,
    is_builtin,
    model_data,
    model_specs,
  } = modelDetail;
  const { updated_at, model_description = '', is_enterprise, readme } = model_data || {};
  const isRegister = is_builtin === false;
  const hasCache = (model_specs || []).some((ele) => ele.cache_status);
  const downloadHubs = modelDetail?.download_hubs || [];

  const deployInitialValues = useMemo(
    () => ({
      model_name: modelName,
      ...(typeof searchParams.model_version === 'string'
        ? { model_version: searchParams.model_version }
        : {}),
      ...(typeof searchParams.model_engine === 'string'
        ? { model_engine: searchParams.model_engine }
        : {}),
      ...(typeof searchParams.download_hub === 'string'
        ? { download_hub: searchParams.download_hub }
        : {}),
    }),
    [modelName, searchParams.model_version, searchParams.model_engine, searchParams.download_hub],
  );

  const { run: deleteModel, loading: deleteLoading } = useRequest(
    () => request(`/model_registrations/${modelType}/${modelName}`, { method: 'delete' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) history.go(-1);
      },
    },
  );
  const { run: updateModel, loading: updateLoading } = useRequest(
    (data) => request('/models/update_model_spec', { method: 'post', data }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          message.success(lGet('global.message.updateSuccess'));
          run();
        }
        if (!res.success && res?.data?.code === 10000) showModelHubModal();
      },
    },
  );

  const handleDeleteCustomModel = useCallback(() => {
    modal.confirm({
      title: `${lGet('models.repository.modelDeleteTips')}${modelName}`,
      onOk: () => deleteModel(),
      okButtonProps: {
        loading: deleteLoading,
      },
    });
  }, [modelName, deleteModel, deleteLoading, modal]);

  const handleUpadte = useCallback(async () => {
    if (!modelHubApiToken) {
      message.warning(lGet('models.repository.beforeUpdateTips'));
      showModelHubModal();
      return;
    }
    updateModel({
      model_type: modelType,
      model_name: modelName,
    });
  }, [modelHubApiToken, modelType, modelName, updateModel, message]);

  const tabsOptions = useMemo(
    () => [
      {
        value: 'readme',
        label: (
          <span className="inline-flex items-center gap-1.5">
            <BookOpen size={14} />
            {lGet('models.repository.detail.readme')}
          </span>
        ),
      },
      {
        value: 'versions',
        label: (
          <span className="inline-flex items-center gap-1.5">
            <List size={14} />
            {lGet('models.repository.detail.versions')}
          </span>
        ),
      },
      {
        value: 'deploy',
        label: (
          <span className="inline-flex items-center gap-1.5">
            <Rocket size={14} />
            {lGet('models.repository.detail.deploy')}
          </span>
        ),
      },
    ],
    [],
  );

  const descText = (model_description || '').trim();
  const descNeedsCollapse = descText.length > DESC_COLLAPSE_LEN;
  const descShown =
    !descNeedsCollapse || descExpanded
      ? descText
      : `${descText.slice(0, DESC_COLLAPSE_LEN).trim()}…`;

  const renderTabBody = () => (
    <>
      {mountedTabs.readme ? (
        <div style={{ display: activeTab === 'readme' ? undefined : 'none' }}>
          <div className="min-w-0 w-full">
            {readme ? (
              <ReactMarkdown parseHtml classnames="model-readme">
                {readme}
              </ReactMarkdown>
            ) : (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
            )}
          </div>
        </div>
      ) : null}
      {mountedTabs.versions ? (
        <div style={{ display: activeTab === 'versions' ? undefined : 'none' }}>
          <ModelVersions
            modelData={model_data}
            downloadHubs={downloadHubs}
            onDeleteCallBack={() => run()}
            embedded
          />
        </div>
      ) : null}
      {mountedTabs.deploy ? (
        <div style={{ display: activeTab === 'deploy' ? undefined : 'none' }}>
          <DeployModelInstance
            key={[
              deployInitialValues.model_version || '',
              deployInitialValues.model_engine || '',
              deployInitialValues.download_hub || '',
            ].join('|')}
            type="add"
            variant="embedded"
            initialValues={deployInitialValues}
            modelType={modelType as ModelType}
            modelData={model_data}
          />
        </div>
      ) : null}
    </>
  );

  return (
    <PageContainer showPageHeader={false} showBreadcrumb>
      <div className="w-full flex flex-col">
        {/* 头区：一行主轴 + 次要折叠，无厚白卡 */}
        <header className="pb-5 mb-0 border-b border-[color:var(--c-border-light)]">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 flex items-center gap-2 flex-wrap">
              <h1 className="m-0 text-2xl font-bold leading-tight text-[var(--c-ink)]">
                {modelName}
              </h1>
              {isRegister ? (
                <Tag
                  bordered={false}
                  className="!mr-0 !rounded-full !bg-[var(--c-surface-2)] !text-[color:var(--c-ink-2)] !text-[11px] !font-semibold"
                >
                  {l('models.repository.detail.modelSource.custom')}
                </Tag>
              ) : null}
              {is_enterprise ? (
                <Tag
                  bordered={false}
                  className="!mr-0 !rounded-full !bg-[var(--c-primary-light)] !text-[var(--c-primary)] !text-[11px] !font-semibold"
                >
                  {l('models.repository.detail.modelSource.enterprise')}
                </Tag>
              ) : null}
            </div>
            <div className="shrink-0 flex gap-2">
              {isRegister ? (
                <Button
                  type="primary"
                  danger
                  icon={<Trash2 size={14} />}
                  onClick={handleDeleteCustomModel}
                >
                  {l('global.actions.delete')}
                </Button>
              ) : (
                <Button
                  icon={<RefreshCw size={14} />}
                  loading={updateLoading}
                  onClick={handleUpadte}
                >
                  {l('global.actions.update')}
                </Button>
              )}
            </div>
          </div>

          <div className="mt-3">
            <ModelTags modelData={model_data} cacheStatus={hasCache} maxVisible={4} />
          </div>

          <div className="mt-2.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-[color:var(--c-ink-3)]">
            <span>
              {lGet('models.repository.detail.versions')} {model_version_count}
            </span>
            <span aria-hidden>·</span>
            {model_instance_count > 0 ? (
              <button
                type="button"
                className="inline-flex items-center gap-1 border-0 bg-transparent p-0 text-[12px] text-[var(--c-primary)] hover:underline cursor-pointer"
                onClick={() => history.push('/models/instances')}
              >
                {lGet('models.repository.detail.instances')} {model_instance_count}
                <ExternalLink size={11} aria-hidden />
              </button>
            ) : (
              <span>
                {lGet('models.repository.detail.instances')} {model_instance_count}
              </span>
            )}
            <span aria-hidden>·</span>
            <span>
              {lGet('models.repository.detail.updateTime')}{' '}
              {formatDisplayTime(updated_at ? updated_at * 1000 : undefined, 'YYYY-MM-DD HH:mm')}
            </span>
          </div>

          {descText && descText !== '-' ? (
            <p className="mt-2.5 mb-0 text-[13px] leading-relaxed text-[color:var(--c-ink-2)]">
              {descShown}
              {descNeedsCollapse ? (
                <button
                  type="button"
                  className="ml-1.5 border-0 bg-transparent p-0 text-[13px] text-[var(--c-primary)] hover:underline cursor-pointer"
                  onClick={() => setDescExpanded((v) => !v)}
                >
                  {descExpanded
                    ? lGet('models.repository.detail.collapse')
                    : lGet('models.repository.detail.expand')}
                </button>
              ) : null}
            </p>
          ) : null}
        </header>

        {/* Tab + 正文共用一张内容面 */}
        <section
          className={classNames(
            // overflow-visible：避免版本表「更多操作」下拉被裁切后无法点击
            'mt-4 overflow-visible rounded-[var(--r-lg)] border border-[color:var(--c-border-light)]',
            'bg-[var(--c-surface)] shadow-card',
          )}
        >
          <div className="px-5 sm:px-6 pt-1">
            <PillTabs
              block
              variant="underline"
              value={activeTab as string}
              options={tabsOptions}
              onChange={(key) => {
                const next = String(key);
                setActiveTab(next);
                const q = {
                  ...searchParams,
                  activeTab: next,
                };
                history.replace(
                  `/models/repository/${modelType}/${modelName}?${qs.stringify(q)}`,
                );
              }}
            />
          </div>
          <div className="px-5 sm:px-6 py-5">{renderTabBody()}</div>
        </section>
      </div>
    </PageContainer>
  );
};

export default ModelDetail;
