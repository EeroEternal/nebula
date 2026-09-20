import { ProFormRadio, ProFormSelect, ProFormSlider } from '@ant-design/pro-components';
import { useParams } from '@umijs/max';
import { useMemo } from 'react';

import { ModelType } from '@/constants/modelData';
import {
  FILTER_LANG_MAP,
  FILTER_MARKS,
  FILTER_MODEL_ABILITY_MAP,
  FILTER_MODEL_TYPE,
  FILTER_STATUS,
} from '@/constants/repository';
import { l, lGet } from '@/utils/intl';

const FilterParams = () => {
  const params = useParams();
  const { modelType } = params || {};
  const statusOptions = useMemo(
    () =>
      FILTER_STATUS.map((item) => ({
        label: lGet(`model.repository.status.${item}`),
        value: item,
      })),
    [],
  );
  const abilityOptions = useMemo(() => {
    if (!FILTER_MODEL_ABILITY_MAP[modelType as ModelType]) return [];
    return FILTER_MODEL_ABILITY_MAP[modelType as ModelType].map((item) => ({
      label: lGet(`global.model.ability.${item}`),
      value: item,
    }));
  }, [modelType]);
  const langOptions = useMemo(() => {
    if (!FILTER_LANG_MAP[modelType as ModelType]) return [];
    return FILTER_LANG_MAP[modelType as ModelType].map((item) => ({
      label: lGet(`global.lang.${item}`),
      value: item,
    }));
  }, [modelType]);
  const modelTypeOptions = useMemo(
    () =>
      FILTER_MODEL_TYPE.map((item) => ({ label: lGet(`global.model.type.${item}`), value: item })),
    [],
  );
  const isLLM = modelType === ModelType.LLM;
  const showAbilitySearch = [
    ModelType.LLM,
    ModelType.image,
    ModelType.audio,
    ModelType.video,
  ].includes(modelType as ModelType);
  const showLangSearch = [ModelType.LLM, ModelType.embedding, ModelType.rerank].includes(
    modelType as ModelType,
  );
  if (modelType === 'custom') {
    return (
      <ProFormRadio.Group
        name="custom_model_type"
        label={l('global.model.type')}
        options={modelTypeOptions}
      />
    );
  }
  return (
    <>
      <ProFormSelect
        name="cache_status"
        label={l('model.repository.status')}
        placeholder={l('model.repository.unlimited')}
        colProps={{ span: 6 }}
        options={statusOptions}
      />
      {showAbilitySearch && (
        <ProFormSelect
          name="model_ability"
          label={l('model.repository.modelAbility')}
          placeholder={l('model.repository.unlimited')}
          colProps={{ span: 6 }}
          options={abilityOptions}
        />
      )}
      {isLLM && (
        <ProFormSlider
          name="context_length"
          label={l('model.repository.context')}
          marks={FILTER_MARKS}
          step={10}
          colProps={{ span: 6 }}
          // transform={(value) => value * 1024}
        />
      )}
      {showLangSearch && (
        <ProFormSelect
          name="model_lang"
          label={l('model.repository.language')}
          placeholder={l('model.repository.unlimited')}
          colProps={{ span: 6 }}
          options={langOptions}
        />
      )}
    </>
  );
};

export default FilterParams;
