import { QuestionCircleOutlined } from '@ant-design/icons';
import { InputNumber, Slider, Tooltip } from 'antd';
import { FC } from 'react';

interface InputNumberWithSliderProps {
  label: React.ReactNode;
  tooltip?: React.ReactNode;
  value?: number;
  defaultValue?: number;
  max?: number;
  min?: number;
  precision?: number;
  step?: number;
  disabled?: boolean;
  onChange?: (v: number) => void;
}
const InputNumberWithSlider: FC<InputNumberWithSliderProps> = ({
  label,
  tooltip,
  defaultValue,
  value,
  max,
  min,
  step = 1,
  precision = 0,
  disabled = false,
  onChange,
}) => {
  return (
    <div className="flex-1">
      <div className="flex justify-between items-center">
        <span className="ant-form-item-label">
          <span className="flex gap-x-[4px] items-center">
            {label}
            {tooltip && (
              <Tooltip title={tooltip}>
                <QuestionCircleOutlined className="text-[#8c8c8c]" />
              </Tooltip>
            )}
          </span>
        </span>
        <InputNumber
          precision={precision}
          step={step}
          defaultValue={defaultValue}
          value={value}
          max={max}
          min={min}
          disabled={disabled}
          onChange={(v) => onChange?.(v as number)}
        />
      </div>
      <div className="flex items-center gap-x-[4px]">
        <div className="mt-[8px] text-muted">{min}</div>
        <Slider
          step={step}
          defaultValue={defaultValue}
          value={value}
          max={max}
          min={min}
          disabled={disabled}
          onChange={(v) => onChange?.(v)}
          className="mb-0 flex-1"
        />
        <div className="mt-[8px] text-muted">{max}</div>
      </div>
    </div>
  );
};
export default InputNumberWithSlider;
