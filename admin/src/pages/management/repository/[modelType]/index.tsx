import { Form } from 'antd';
import type { InputProps } from 'antd';
import { ProForm, ProFormField } from '@ant-design/pro-components';
import type { ProFormProps } from '@ant-design/pro-components';
import { useRequest } from 'ahooks';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Database } from 'lucide-react';
import { useParams, history, useLocation, useModel } from '@umijs/max';
import qs from 'qs';
import { debounce, isNil, omitBy } from 'lodash';
import { EmptyState, PageContainer, Pagination, PillTabs, SectionLoading } from '@/components';
import { l } from '@/utils/intl';
import { REPOSITORY_NAV_TABS, FILTER_MODEL_TYPE } from '@/constants/repository';
import { UNLIMITED } from '@/constants';
import request from '@/utils/request';
import { swrGet, swrInvalidate, swrPeek } from '@/utils/swrCache';
import ModelFilterPanel from '../components/NewModelFilterPanel';
import ModelList from '../components/NewModelList';
import UpdateModel from '../components/UpdateModel';
import { saveLastRepoListUrl } from '@/utils/repoListUrl';
import { applyRepoPins, loadRepoPins, repoPinKey, toggleRepoPin } from '../repoPin';
import type { ModelRepositoryListItem } from '@/types/Public/data';

const PAGE_SIZE = 12;
/** 仓库列表网络结果新鲜窗口；命中缓存时瞬时出数，过期后台刷新（与后端 MODEL_LIST_CACHE_TTL 对齐） */
const REPO_LIST_STALE_MS = 60_000;

type RepoListItem = ModelRepositoryListItem;

const isModelCached = (item: RepoListItem) =>
  !!(
    item?.exist_cache ||
    item?.cache_status ||
    (item?.model_specs || []).some(
      (spec) =>
        spec?.cache_status === true ||
        (Array.isArray(spec?.cache_status) && spec.cache_status.some(Boolean)),
    )
  );

/**
 * 排序：已缓存优先；组内按所选字段降序（更新时间 / 修改时间，暂与 updated_at 同源）
 */
const sortModelList = (list: RepoListItem[], sortBy: string) => {
  const key = sortBy === 'last_modified' ? 'last_modified' : 'updated_at';
  return [...list].sort((a, b) => {
    const ca = isModelCached(a) ? 1 : 0;
    const cb = isModelCached(b) ? 1 : 0;
    if (cb !== ca) return cb - ca;
    const ta = Number(a?.[key] ?? a?.updated_at ?? 0);
    const tb = Number(b?.[key] ?? b?.updated_at ?? 0);
    if (tb !== ta) return tb - ta;
    return String(a?.model_name || '').localeCompare(String(b?.model_name || ''));
  });
};

const buildSearchQuery = (values: Record<string, unknown>, isRegisterModel: boolean) => {
  const query = omitBy(
    {
      model_name: values.model_name || undefined,
      cache_status:
        !isRegisterModel && values.cache_status && values.cache_status !== UNLIMITED
          ? values.cache_status
          : undefined,
      model_ability:
        !isRegisterModel && values.model_ability && values.model_ability !== UNLIMITED
          ? values.model_ability
          : undefined,
      model_lang:
        !isRegisterModel && values.model_lang && values.model_lang !== UNLIMITED
          ? values.model_lang
          : undefined,
      sort_by: values.sort_by && values.sort_by !== 'updated_at' ? values.sort_by : undefined,
      curPageNum: Number(values.curPageNum) > 1 ? values.curPageNum : undefined,
    },
    (v) => isNil(v) || v === '',
  );
  return qs.stringify(query);
};

/** 网络缓存 key：不含分页/排序/名称检索（名称纯前端过滤） */
const buildRepoCacheKey = (
  modelType: string | undefined,
  isRegisterModel: boolean,
  filters: Record<string, unknown>,
) =>
  [
    'repo-list',
    isRegisterModel ? 'custom' : modelType || 'LLM',
    filters.cache_status ?? '',
    filters.model_ability ?? '',
    filters.model_lang ?? '',
    isRegisterModel ? 'custom' : 'builtin',
  ].join('|');

const filterByModelName = (list: RepoListItem[], modelName?: string) => {
  const q = String(modelName || '')
    .trim()
    .toLowerCase();
  if (!q) return list;
  return list.filter((item) => String(item?.model_name || '').toLowerCase().includes(q));
};

/** 拉全量列表（前端统一排序 + 分页；服务端分页无法做「已缓存优先」） */
const fetchRepoList = async (
  modelType: string | undefined,
  isRegisterModel: boolean,
  filters: Record<string, unknown>,
  modelName?: string,
) => {
  const base = {
    detailed: false,
    curPageNum: -1,
    numPerPage: -1,
    q: (modelName || '').trim() || undefined,
    model_name: (modelName || '').trim() || undefined,
    cache_status: filters.cache_status,
    model_ability: filters.model_ability,
    model_lang: filters.model_lang,
    is_builtin: isRegisterModel ? false : true,
  };

  if (isRegisterModel) {
    const chunks = await Promise.all(
      FILTER_MODEL_TYPE.map(async (t) => {
        const res = await request(`/model_registrations/${t}`, {
          params: {
            ...base,
            is_builtin: false,
            detailed: true,
          },
        });
        return (res?.data?.results || []).map((item: RepoListItem) => ({
          ...item,
          model_type: item.model_type || t,
        }));
      }),
    );
    const all = filterByModelName(chunks.flat(), modelName);
    return { results: all, count: all.length };
  }

  const res = await request(`/model_registrations/${modelType}`, { params: base });
  const results = res?.data?.results || [];
  return {
    results,
    count: Number(res?.data?.count ?? results.length),
  };
};

const ModelRepository = () => {
  const params = useParams();
  const location = useLocation();
  const { modelType } = params || {};
  const [form] = Form.useForm();
  const modelName = Form.useWatch('model_name', form);
  const curPageNum = Form.useWatch('curPageNum', form);
  const isRegisterModel = modelType === 'custom';
  const skipNextLocationSync = useRef(false);
  const allRef = useRef<RepoListItem[]>([]);
  /** 列表请求世代：切 Tab/筛选时递增，阻止过期响应写回错误类型的数据 */
  const listReqSeq = useRef(0);
  const [view, setView] = useState<{ results: RepoListItem[]; count: number }>({
    results: [],
    count: 0,
  });
  /** 当前 view 对应的 modelType；切 Tab 后未刷新前禁止展示上一类型卡片 */
  const [viewType, setViewType] = useState<string | undefined>(modelType);
  const [loading, setLoading] = useState(false);
  const [hasLoaded, setHasLoaded] = useState(false);

  const { initialState } = useModel('@@initialState');
  const pinUser =
    (initialState?.currentUser as { username?: string; name?: string } | undefined)
      ?.username ||
    (initialState?.currentUser as { username?: string; name?: string } | undefined)?.name ||
    'anonymous';
  const [pinnedKeys, setPinnedKeys] = useState<string[]>(() => loadRepoPins(pinUser));

  const { data: modelHubRes } = useRequest(() => request('/setting/model_hub'), {
    cacheKey: 'setting-model-hub',
    staleTime: 60_000,
  });
  const modelHubApiToken = modelHubRes?.data?.data?.api_token;

  const applyView = useCallback(
    (all: RepoListItem[], sort: string, page: number, name?: string, pins?: string[]) => {
      const filtered = filterByModelName(all, name);
      const sorted = applyRepoPins(
        sortModelList(filtered, sort),
        pins ?? pinnedKeys,
        modelType,
      );
      const start = (Math.max(page, 1) - 1) * PAGE_SIZE;
      setView({
        results: sorted.slice(start, start + PAGE_SIZE),
        count: sorted.length,
      });
    },
    [modelType, pinnedKeys],
  );

  useEffect(() => {
    const next = loadRepoPins(pinUser);
    setPinnedKeys(next);
    if (!allRef.current.length) return;
    applyView(
      allRef.current,
      form.getFieldValue('sort_by') || 'updated_at',
      Number(form.getFieldValue('curPageNum')) || 1,
      form.getFieldValue('model_name'),
      next,
    );
    // 只在登录用户切换时重载置顶；applyView 随 pinnedKeys 换身份，不可入依赖
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pinUser]);

  const resolveFilters = useCallback(() => {
    const { custom_model_type: _drop, sort_by: _s, curPageNum: _p, ...rest } =
      form.getFieldsValue();
    return {
      model_name: rest.model_name || undefined,
      cache_status: isRegisterModel
        ? undefined
        : rest.cache_status === 'cached'
          ? true
          : rest.cache_status === 'uncached'
            ? false
            : undefined,
      model_ability:
        isRegisterModel || rest.model_ability === UNLIMITED ? undefined : rest.model_ability,
      model_lang: isRegisterModel || rest.model_lang === UNLIMITED ? undefined : rest.model_lang,
    };
  }, [form, isRegisterModel]);

  const loadList = useCallback(
    async (opts?: { force?: boolean }) => {
      const seq = ++listReqSeq.current;
      const filters = resolveFilters();
      const nameQ = form.getFieldValue('model_name');
      // 缓存全量结果（不含分页/排序）；排序与分页在 applyView 本地完成
      const cacheKey = [
        buildRepoCacheKey(modelType, isRegisterModel, filters),
        String(nameQ || '').trim(),
      ].join('|');

      const apply = (payload: { results: RepoListItem[]; count: number }) => {
        if (seq !== listReqSeq.current) return;
        allRef.current = payload.results || [];
        setViewType(modelType);
        applyView(
          allRef.current,
          form.getFieldValue('sort_by') || 'updated_at',
          Number(form.getFieldValue('curPageNum')) || 1,
          form.getFieldValue('model_name'),
        );
        setHasLoaded(true);
      };

      const stillCurrent = () => {
        if (seq !== listReqSeq.current) return false;
        const f = resolveFilters();
        const n = form.getFieldValue('model_name');
        return (
          [
            buildRepoCacheKey(modelType, isRegisterModel, f),
            String(n || '').trim(),
          ].join('|') === cacheKey
        );
      };

      const fetcher = () =>
        fetchRepoList(modelType, isRegisterModel, filters, nameQ);

      const peek = !opts?.force
        ? swrPeek<{ results: RepoListItem[]; count: number }>(cacheKey)
        : undefined;
      if (peek) {
        apply(peek);
        setLoading(false);
        void swrGet(cacheKey, fetcher, {
          staleTime: REPO_LIST_STALE_MS,
          onUpdate: (fresh) => {
            if (stillCurrent()) apply(fresh);
          },
        });
        return;
      }

      setLoading(true);
      try {
        const { data } = await swrGet(cacheKey, fetcher, {
          staleTime: REPO_LIST_STALE_MS,
          force: opts?.force,
          onUpdate: (fresh) => {
            if (stillCurrent()) apply(fresh);
          },
        });
        if (stillCurrent()) apply(data);
      } finally {
        setLoading(false);
      }
    },
    [applyView, form, isRegisterModel, modelType, resolveFilters],
  );

  const syncUrlFromForm = () => {
    const values = form.getFieldsValue();
    const search = buildSearchQuery(values, isRegisterModel);
    const next = search ? `?${search}` : '';
    if (next === (location.search || '') || next === location.search) {
      return;
    }
    skipNextLocationSync.current = true;
    history.replace({
      pathname: `/models/repository/${modelType}`,
      search: search || undefined,
    });
    saveLastRepoListUrl(`/models/repository/${modelType}`, search);
  };

  /** 名称检索：服务端 q（DB AND / ilike） */
  const debouncedSearch = useMemo(
    () =>
      debounce(() => {
        void loadList();
        syncUrlFromForm();
      }, 280),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isRegisterModel, modelType, loadList],
  );
  useEffect(() => () => debouncedSearch.cancel(), [debouncedSearch]);

  const handleInputChange: InputProps['onChange'] = (e) => {
    form.setFieldsValue({
      curPageNum: 1,
      model_name: e.target.value,
    });
    debouncedSearch();
  };

  const handleValuesChange: ProFormProps['onValuesChange'] = (changed) => {
    const keys = Object.keys(changed || {});
    if (keys.length === 1 && keys[0] === 'curPageNum') {
      applyView(
        allRef.current,
        form.getFieldValue('sort_by') || 'updated_at',
        Number(changed.curPageNum) || 1,
        form.getFieldValue('model_name'),
      );
      syncUrlFromForm();
      return;
    }
    if (keys.length === 1 && keys[0] === 'sort_by') {
      form.setFieldsValue({ curPageNum: 1 });
      applyView(
        allRef.current,
        changed.sort_by || 'updated_at',
        1,
        form.getFieldValue('model_name'),
      );
      syncUrlFromForm();
      return;
    }
    form.setFieldsValue({ curPageNum: 1 });
    void loadList();
    syncUrlFromForm();
  };

  const handlePageChange = (page: number) => {
    form.setFieldsValue({ curPageNum: page });
    applyView(
      allRef.current,
      form.getFieldValue('sort_by') || 'updated_at',
      page,
      form.getFieldValue('model_name'),
    );
    syncUrlFromForm();
  };

  const handleTabChange = (key: string) => {
    if (key === 'downloads') {
      history.push('/models/repository/downloads');
      return;
    }
    history.push(`/models/repository/${key}`);
  };

  const onDeleteCustomModelCallBack = () => {
    swrInvalidate('repo-list');
    form.setFieldsValue({ model_name: undefined, curPageNum: 1 });
    void loadList({ force: true });
    syncUrlFromForm();
  };

  const onModelUpdateSuccess = (type: string) => {
    swrInvalidate('repo-list');
    if (type !== modelType) {
      history.push(`/models/repository/${type}`);
    } else {
      form.resetFields();
      void loadList({ force: true });
      syncUrlFromForm();
    }
  };

  const viewMatchesType = viewType === modelType;
  const { count, results } = view;
  const shownLoaded = viewMatchesType && hasLoaded;
  const shownCount = viewMatchesType ? count : 0;
  const shownResults = viewMatchesType ? results : [];

  const runderModelContent = () => {
    // 仅首屏无数据时转圈；刷新时保留旧列表（毫秒级感知）
    // 切 Tab 后 viewType 未对齐前一律转圈，避免闪出上一类型卡片
    if (!viewMatchesType || (loading && !hasLoaded)) {
      return <SectionLoading />;
    }
    if (shownLoaded && !shownCount) {
      return (
        <EmptyState className="py-20" customIcon={<Database size={48} className="text-muted" />} />
      );
    }
    if (!shownLoaded) {
      return <SectionLoading />;
    }
    return (
      <div className={loading ? 'opacity-70 transition-opacity duration-150' : undefined}>
        <ModelList
          modelType={modelType}
          modelList={shownResults}
          pinnedKeys={pinnedKeys}
          onTogglePin={(item) => {
            const type =
              modelType === 'custom'
                ? String(item.model_type || 'LLM')
                : String(modelType || item.model_type || 'LLM');
            const next = toggleRepoPin(pinUser, repoPinKey(type, item.model_name));
            setPinnedKeys(next);
            applyView(
              allRef.current,
              form.getFieldValue('sort_by') || 'updated_at',
              Number(form.getFieldValue('curPageNum')) || 1,
              form.getFieldValue('model_name'),
              next,
            );
          }}
          deleteCustomModelCallBack={onDeleteCustomModelCallBack}
        />
        {shownCount > PAGE_SIZE && (
          <Pagination
            page={curPageNum}
            total={shownCount}
            pageSize={PAGE_SIZE}
            onPageChange={handlePageChange}
          />
        )}
      </div>
    );
  };

  useEffect(() => {
    if (skipNextLocationSync.current) {
      skipNextLocationSync.current = false;
      return;
    }
    form.resetFields();
    let initParams: Record<string, unknown> = {
      curPageNum: 1,
      cache_status: UNLIMITED,
      model_ability: UNLIMITED,
      model_lang: UNLIMITED,
      sort_by: 'updated_at',
    };
    if (location.search && location.search.substring(1)) {
      initParams = {
        ...initParams,
        ...qs.parse(location.search.substring(1)),
      };
      if (initParams.curPageNum) {
        initParams.curPageNum = Number(initParams.curPageNum) || 1;
      }
    }
    form.setFieldsValue(initParams);
    saveLastRepoListUrl(`/models/repository/${modelType}`, location.search || '');
    // 切 Tab / URL 变化时立刻清空，避免短暂展示上一类型的卡片
    listReqSeq.current += 1;
    setHasLoaded(false);
    setView({ results: [], count: 0 });
    swrInvalidate('repo-list');
    void loadList({ force: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modelType, location.search]);

  return (
    <PageContainer
      title={l('menu.models.repository')}
      subTitle={l('models.repository.subTitle')}
    >
      <div className="flex flex-col w-full gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="overflow-x-auto min-w-0 pb-0.5">
            <PillTabs
              aria-label={l('models.repository.typeTabs')}
              value={modelType}
              options={REPOSITORY_NAV_TABS.map((item) => ({
                value: item,
                label:
                  item === 'downloads'
                    ? l('models.repository.downloadsTab')
                    : l(`global.model.type.${item}`),
              }))}
              onChange={(key) => handleTabChange(String(key))}
            />
          </div>
          <UpdateModel
            modelHubApiToken={modelHubApiToken}
            submitCallBack={onModelUpdateSuccess}
          />
        </div>

        <ProForm
          layout="vertical"
          form={form}
          submitter={false}
          autoFocusFirstInput={false}
          onValuesChange={handleValuesChange}
        >
          <ModelFilterPanel
            form={form}
            resultCount={shownLoaded ? shownCount : undefined}
            searchValue={modelName}
            searchPlaceholder={l('models.repository.searchModel')}
            onSearchChange={handleInputChange}
          />
          <ProFormField hidden name="curPageNum" />
          <ProFormField hidden name="model_name" />
        </ProForm>

        {runderModelContent()}
      </div>
    </PageContainer>
  );
};
export default ModelRepository;
