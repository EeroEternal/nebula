import { Tag } from 'antd';
import { useCallback, FC } from 'react';
import { size } from 'lodash';
import { useParams } from '@umijs/max';
import { l } from '@/utils/intl';
import type { ModelData } from '@/types/Public/data';
import { formatToKUnits } from '@/utils/fomatData';
import { ModelType } from '@/constants/modelData';

interface ModelTagsProps {
  modelData: ModelData;
  cacheStatus?: boolean;
}
const ModelTags: FC<ModelTagsProps> = ({ modelData = {} as ModelData, cacheStatus }) => {
  const { modelType } = useParams();
  const isLLM = modelType === ModelType.LLM;
  const isEmbedding = modelType === ModelType.embedding;
  const getLangeTag = useCallback((langs?: string | string[]) => {
    if (!size(langs)) return null;
    if (typeof langs === 'string') {
      return (
        <Tag key={langs} color="blue">
          {l(`global.lang.${langs.toLocaleLowerCase()}`)}
        </Tag>
      );
    }
    return (langs || []).map((item) => {
      if (item.includes('languages supported')) {
        const value = item.split('languages supported')[0];
        return (
          <Tag key={item} color="blue">
            {l('global.lang.supported', undefined, { value })}
          </Tag>
        );
      }
      if (item === 'multilingual') {
        return (
          <Tag key={item} color="blue">
            {l('global.lang.multilingual')}
          </Tag>
        );
      }
      return (
        <Tag key={item} color="blue">
          {l(`global.lang.${item}`)}
        </Tag>
      );
    });
  }, []);
  return (
    <>
      {(modelData?.model_ability || []).map((item) => (
        <Tag key={item} color="green">
          {l(`global.model.ability.${item}`)}
        </Tag>
      ))}
      {getLangeTag(modelData?.model_lang)}
      {getLangeTag(modelData?.language)}
      {isLLM && (
        <Tag key="context" color="cyan">
          {formatToKUnits(modelData?.context_length)}
        </Tag>
      )}
      {isEmbedding && (
        <>
          <Tag color="cyan">max_tokens: {modelData?.max_tokens}</Tag>
          <Tag color="cyan">dimensions: {modelData?.dimensions}</Tag>
        </>
      )}
      {cacheStatus && <Tag color="blue">Cached</Tag>}
    </>
  );
};
export default ModelTags;
