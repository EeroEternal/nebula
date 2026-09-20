import type { RadioGroupProps } from 'antd';
import { Radio } from 'antd';
import classNames from 'classnames';
import React from 'react';

interface RadioButtonGroupProps extends RadioGroupProps {
  className?: string;
}

/** 选项组：对齐 Demo PillTabs 选中态 */
const RadioButtonGroup: React.FC<RadioButtonGroupProps> = ({
  options = [],
  value,
  onChange,
  className = '',
  ...reset
}) => {
  return (
    <Radio.Group
      {...reset}
      value={value}
      onChange={onChange}
      onBlur={() => {}}
      className={classNames(
        'inline-flex flex-wrap gap-0.5 rounded-md bg-[var(--c-surface-2)] p-[3px]',
        className,
      )}
    >
      {options.map((opt) => {
        const label = typeof opt === 'object' ? opt.label : opt;
        const val = typeof opt === 'object' ? opt.value : opt;
        const checked = value === val;
        return (
          <Radio
            key={String(val)}
            value={val}
            className={classNames(
              '!mr-0 !px-3 !py-1.5 !rounded-sm !border-0 !text-[13px] !font-semibold transition-colors',
              checked
                ? '!bg-[var(--c-surface)] !text-[var(--c-navy)] shadow-card'
                : '!bg-transparent !text-[color:var(--c-ink-2)] hover:!text-[var(--c-navy)]',
            )}
          >
            {label}
          </Radio>
        );
      })}
    </Radio.Group>
  );
};

export default RadioButtonGroup;
