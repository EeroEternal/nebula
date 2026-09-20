import { Popover, Tag } from 'antd';
import type { TagProps } from 'antd';
import { FC, ReactNode, useMemo } from 'react';
import { size } from 'lodash';
import classNames from 'classnames';
import { useParams } from '@umijs/max';
import { StatusTag } from '@/components';
import { l, lGet } from '@/utils/intl';
import type { ModelData } from '@/types/Public/data';
import { formatToKUnits } from '@/utils/fomatData';
import { ModelType } from '@/constants/modelData';

/** 类型/属性 Tag：neutral 浅底，无状态圆点 */
export const ModelTag: FC<TagProps> = ({ children, className = '', ...reset }) => {
  return (
    <Tag
      bordered={false}
      {...reset}
      className={classNames(
        'mr-0 rounded-full !px-2.5 !py-0 !leading-5 !text-[11px] !font-medium',
        '!bg-[var(--c-surface-2)] !text-[color:var(--c-ink-2)]',
        className,
      )}
    >
      {children}
    </Tag>
  );
};

type TagItem = { key: string; node: ReactNode; status?: 'success' };

interface ModelTagsProps {
  modelData: ModelData;
  cacheStatus?: boolean;
  /** 最多展示条数，其余进「+N」；0 表示不截断 */
  maxVisible?: number;
}

const ModelTags: FC<ModelTagsProps> = ({
  modelData = {} as ModelData,
  cacheStatus,
  maxVisible = 4,
}) => {
  const { modelType } = useParams();
  const isLLM = modelType === ModelType.LLM;
  const isEmbedding = modelType === ModelType.embedding;

  const langLabels = (langs?: string | string[]): TagItem[] => {
    if (!size(langs)) return [];
    if (typeof langs === 'string') {
      return [
        {
          key: `lang-${langs}`,
          node: l(`global.lang.${langs.toLocaleLowerCase()}`),
        },
      ];
    }
    return (langs || []).map((item) => {
      if (item.includes('languages supported')) {
        const value = item.split('languages supported')[0];
        return {
          key: `lang-${item}`,
          node: l('global.lang.supported', undefined, { value }),
        };
      }
      if (item === 'multilingual') {
        return { key: 'lang-multilingual', node: l('global.lang.multilingual') };
      }
      return { key: `lang-${item}`, node: l(`global.lang.${item}`) };
    });
  };

  const allTags = useMemo(() => {
    const items: TagItem[] = [];
    // 高价值优先：语言 → 主能力 → 上下文/维度 → 缓存
    items.push(...langLabels(modelData?.model_lang));
    items.push(...langLabels(modelData?.language));
    (modelData?.model_ability || []).forEach((item) => {
      items.push({
        key: `ability-${item}`,
        node: l(`global.model.ability.${item}`),
      });
    });
    if (isLLM && modelData?.context_length) {
      items.push({
        key: 'context',
        node: formatToKUnits(modelData.context_length),
      });
    }
    if (isEmbedding) {
      if (modelData?.max_tokens != null) {
        items.push({ key: 'max_tokens', node: `max_tokens: ${modelData.max_tokens}` });
      }
      if (modelData?.dimensions != null) {
        items.push({ key: 'dimensions', node: `dimensions: ${modelData.dimensions}` });
      }
    }
    if (cacheStatus) {
      items.push({
        key: 'cached',
        node: l('models.repository.cached'),
        status: 'success',
      });
    }
    // 去重（同文案）
    const seen = new Set<string>();
    return items.filter((t) => {
      const sig = String(t.node);
      if (seen.has(sig)) return false;
      seen.add(sig);
      return true;
    });
  }, [modelData, cacheStatus, isLLM, isEmbedding]);

  if (!allTags.length) return null;

  const limit = maxVisible > 0 ? maxVisible : allTags.length;
  const visible = allTags.slice(0, limit);
  const rest = allTags.slice(limit);

  const renderTag = (t: TagItem) =>
    t.status ? (
      <StatusTag key={t.key} tone={t.status}>
        {t.node}
      </StatusTag>
    ) : (
      <ModelTag key={t.key}>{t.node}</ModelTag>
    );

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {visible.map((t) => renderTag(t))}
      {rest.length > 0 ? (
        <Popover
          trigger="click"
          placement="bottomLeft"
          content={
            <div className="flex max-w-xs flex-wrap gap-1.5">{rest.map((t) => renderTag(t))}</div>
          }
        >
          <button
            type="button"
            className="cursor-pointer rounded-full border-0 bg-[var(--c-surface-2)] px-2.5 py-0 text-[11px] font-medium leading-5 text-[color:var(--c-ink-2)] hover:text-[var(--c-primary)]"
          >
            {lGet('models.repository.detail.moreTags', '+{n}', { n: rest.length })}
          </button>
        </Popover>
      ) : null}
    </div>
  );
};

export default ModelTags;
