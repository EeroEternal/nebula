import { Checkbox, Select } from 'antd';
import type { SelectProps } from 'antd';
import type { CheckboxGroupProps } from 'antd/lib/checkbox/Group';
import { l } from '@/utils/intl';
import { LANGUAGES_OPTIONS, ALL_LANGUAGES_OPTIONS } from '@/constants/modelData';
import { FC } from 'react';

const baseLanguageOptionsValue = LANGUAGES_OPTIONS.map((item) => item.value);
function splitValue(value?: string[]) {
  if (!Array.isArray(value)) {
    return [[], []];
  }
  const selectedBase = [];
  const others = [];
  for (const lang of value) {
    if (baseLanguageOptionsValue.includes(lang)) {
      selectedBase.push(lang);
    } else {
      others.push(lang);
    }
  }

  return [selectedBase, others];
}
interface LanguageSelectProps {
  value?: string[];
  onChange?: (v: string[]) => void;
  defaultValue: string[];
  selectProps?: SelectProps;
}
const LanguageSelect: FC<LanguageSelectProps> = ({
  value: propsValue = [],
  onChange,
  defaultValue,
  selectProps = {},
}) => {
  const [checkboxValue, selectValue] = splitValue(propsValue);
  const changeModelLang: CheckboxGroupProps['onChange'] = (value) => {
    // 获取select的值
    const selectValue = propsValue.filter(
      (item: string) => !baseLanguageOptionsValue.includes(item),
    );
    onChange?.([...value, ...selectValue]);
  };
  const handleLanguageSelect: SelectProps['onChange'] = (value) => {
    // 获取checkbox被勾选的值
    const checkValue = baseLanguageOptionsValue.filter((item: string) => propsValue.includes(item));
    const newValue = Array.from(new Set([...checkValue, ...value]));
    onChange?.(newValue);
  };
  return (
    <div className="flex items-center">
      <Checkbox.Group
        className="shrink-0"
        options={LANGUAGES_OPTIONS}
        defaultValue={defaultValue}
        onChange={changeModelLang}
        value={checkboxValue}
      />
      <Select
        value={selectValue}
        {...selectProps}
        showSearch
        mode="multiple"
        placeholder={l('models.register.modelLanguagePlaceholder')}
        onChange={handleLanguageSelect}
        filterOption={(input, option) =>
          ((option?.label ?? '') as string).toLowerCase().includes(input.toLowerCase())
        }
        popupMatchSelectWidth={150}
        options={ALL_LANGUAGES_OPTIONS}
      />
    </div>
  );
};
export default LanguageSelect;
