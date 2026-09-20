import type { FormInstance } from 'antd';
import { Form, Input, Segmented, Select } from 'antd';
import type { InputProps } from 'antd';
import { useParams, history } from '@umijs/max';
import { ProFormSelect } from '@ant-design/pro-components';
import { FC, useMemo } from 'react';
import { ArrowDownWideNarrow, Plus, Search } from 'lucide-react';
import { FILTER_LANG_MAP, FILTER_MODEL_ABILITY_MAP } from '@/constants/repository';
import { UNLIMITED } from '@/constants';
import { ModelType } from '@/constants/modelData';
import { IconButton } from '@/components';
import { l, lGet } from '@/utils/intl';

export type ModelSortBy = 'updated_at' | 'last_modified';

interface ModelFilterPanelProps {
  form: FormInstance;
  /** 匹配结果数量，显示在第二行行首 */
  resultCount?: number;
  searchValue?: string;
  searchPlaceholder?: string;
  onSearchChange?: InputProps['onChange'];
}

const compactSelectClass =
  '!min-w-[120px] !h-8 [&_.ant-select-selector]:!h-8 [&_.ant-select-selector]:!rounded-md';
const compactItemClass = '!mb-0';
/** 32px 槽 + 去掉 affix 默认 9px 竖 padding，避免内层 input 只剩 ~12px 裁切 q/p/g */
const compactSearchClass =
  'w-full min-w-[200px] max-w-[280px] sm:w-56 lg:w-64 !h-8 !py-0 !items-center shrink-0 [&_.ant-input]:!h-auto [&_.ant-input]:!leading-5 [&_.ant-input]:!py-0 [&_.ant-input]:!overflow-visible';

const NewModelFilterPanel: FC<ModelFilterPanelProps> = ({
  resultCount,
  searchValue,
  searchPlaceholder,
  onSearchChange,
}) => {
  const params = useParams();
  const { modelType } = params || {};
  const isCustom = modelType === 'custom';
  const showAbilitySearch =
    !isCustom &&
    [ModelType.LLM, ModelType.image, ModelType.audio, ModelType.video].includes(
      modelType as ModelType,
    );
  const showLangSearch =
    !isCustom &&
    [ModelType.LLM, ModelType.embedding, ModelType.rerank].includes(modelType as ModelType);

  const statusOptions = useMemo(
    () => [
      { label: lGet('models.repository.filter.all'), value: UNLIMITED },
      { label: lGet('models.repository.cached'), value: 'cached' },
      { label: lGet('models.repository.notCached'), value: 'uncached' },
    ],
    [],
  );

  const abilityOptions = useMemo(() => {
    const list = FILTER_MODEL_ABILITY_MAP[modelType as ModelType] || [];
    return [
      {
        label: lGet('models.repository.filter.abilityAny'),
        value: UNLIMITED,
      },
      ...list.map((item) => ({
        label: lGet(`global.model.ability.${item}`),
        value: item,
      })),
    ];
  }, [modelType]);

  const langOptions = useMemo(() => {
    const list = FILTER_LANG_MAP[modelType as ModelType] || [];
    return [
      {
        label: lGet('models.repository.filter.langAny'),
        value: UNLIMITED,
      },
      ...list.map((item) => ({
        label: lGet(`global.lang.${item}`),
        value: item,
      })),
    ];
  }, [modelType]);

  const sortOptions = useMemo(
    () => [
      {
        label: lGet('models.repository.sort.updatedAt'),
        value: 'updated_at',
      },
      {
        label: lGet('models.repository.sort.lastModified'),
        value: 'last_modified',
      },
    ],
    [],
  );

  return (
    <div className="w-full rounded-lg border border-[color:var(--c-border-light)] bg-[var(--c-surface)] shadow-card px-3 py-2.5 flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        {!isCustom ? (
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2 min-w-0">
            <Form.Item name="cache_status" initialValue={UNLIMITED} className={compactItemClass}>
              <Segmented className="repo-cache-segmented" options={statusOptions} size="middle" />
            </Form.Item>

            {showAbilitySearch && (
              <ProFormSelect
                name="model_ability"
                options={abilityOptions}
                initialValue={UNLIMITED}
                formItemProps={{ className: compactItemClass }}
                fieldProps={{
                  className: compactSelectClass,
                  popupMatchSelectWidth: 200,
                }}
                allowClear={false}
              />
            )}
            {showLangSearch && (
              <ProFormSelect
                name="model_lang"
                options={langOptions}
                initialValue={UNLIMITED}
                formItemProps={{ className: compactItemClass }}
                fieldProps={{
                  className: compactSelectClass,
                  popupMatchSelectWidth: 150,
                }}
                allowClear={false}
              />
            )}
          </div>
        ) : null}

        <div className="ml-auto flex flex-wrap items-center justify-end gap-x-2 gap-y-2 min-w-0">
          <Form.Item name="sort_by" initialValue="updated_at" className={compactItemClass}>
            <Select
              className="!w-[128px] !h-8 [&_.ant-select-selector]:!h-8 [&_.ant-select-selector]:!rounded-md [&_.ant-select-selector]:!pl-2 [&_.ant-select-selection-item]:!text-[12px] [&_.ant-select-selection-item]:!pl-0"
              options={sortOptions}
              allowClear={false}
              popupMatchSelectWidth={160}
              prefix={<ArrowDownWideNarrow size={14} className="text-muted" aria-hidden />}
              aria-label={lGet('models.repository.sort.label')}
            />
          </Form.Item>
          {onSearchChange ? (
            <Input
              className={compactSearchClass}
              allowClear
              value={searchValue}
              placeholder={searchPlaceholder || l('models.repository.searchModel')}
              prefix={<Search className="text-muted mr-1" size={16} />}
              onChange={onSearchChange}
            />
          ) : null}
          {isCustom ? (
            <IconButton
              className="!w-8 !h-8 !rounded-md !text-[var(--c-primary)] hover:!bg-[var(--c-primary-light)]"
              aria-label={l('models.repository.registerModel')}
              onClick={() => history.push('/models/register')}
            >
              <Plus size={18} />
            </IconButton>
          ) : null}
        </div>
      </div>

      {typeof resultCount === 'number' ? (
        <div className="text-sm text-muted tabular-nums whitespace-nowrap">
          {l('models.repository.resultCount', { count: resultCount })}
        </div>
      ) : null}
    </div>
  );
};
export default NewModelFilterPanel;
