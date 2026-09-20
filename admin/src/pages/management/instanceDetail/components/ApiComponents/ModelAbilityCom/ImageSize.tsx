import { Slider, InputNumber, Button } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import { useState, useEffect, FC } from 'react';
import { l } from '@/utils/intl';

const SEPARATOR = 'x';
const MAX_SIZE = 2048;
const MIN_SIZE = 64;
interface ImageSizeItemProps {
  label: string;
  value: number;
  onChange: (v: number) => void;
}
const ImageSizeItem: FC<ImageSizeItemProps> = ({ label, value, onChange }) => {
  return (
    <div className="flex-1">
      <div className="flex justify-between items-center">
        <span>{label}</span>
        <InputNumber
          value={value}
          max={MAX_SIZE}
          min={MIN_SIZE}
          onChange={(v) => onChange(v as number)}
        />
      </div>
      <div className="flex items-center gap-x-[4px]">
        <div className="mt-[8px] text-[#999]">{MIN_SIZE}</div>
        <Slider
          value={value}
          max={MAX_SIZE}
          min={MIN_SIZE}
          className="mb-0 flex-1"
          onChange={(v) => onChange(v)}
        />
        <div className="mt-[8px] text-[#999]">{MAX_SIZE}</div>
      </div>
    </div>
  );
};
interface ImageSizeProps {
  value?: string;
  onChange?: (v: string) => void;
}
const ImageSize: FC<ImageSizeProps> = ({ value, onChange }) => {
  const [width, setWidth] = useState(1024);
  const [height, setHeight] = useState(1024);
  const handleWidthChange = (v: number) => {
    setWidth(v);
    onChange?.(`${v}${SEPARATOR}${height}`);
  };
  const handleHeightChange = (v: number) => {
    setHeight(v);
    onChange?.(`${width}${SEPARATOR}${v}`);
  };
  const handleExchange = () => {
    if (width !== height) {
      onChange?.(`${height}${SEPARATOR}${width}`);
    }
  };
  useEffect(() => {
    if (value) {
      const [width, height] = value.split(SEPARATOR);
      setWidth(Number(width));
      setHeight(Number(height));
    }
  }, [value]);
  return (
    <div className="flex items-center gap-x-[16px] w-full">
      <ImageSizeItem
        label={l('model.running.image.width')}
        value={width}
        onChange={handleWidthChange}
      />
      <Button type="primary" icon={<RetweetOutlined />} onClick={handleExchange} />
      <ImageSizeItem
        label={l('model.running.image.height')}
        value={height}
        onChange={handleHeightChange}
      />
    </div>
  );
};
export default ImageSize;
