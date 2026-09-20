import { InputNumberWithSlider } from '@/components';
import {
  SEED_FROM_OPTIONS,
  SEED_MIN,
  SUB_SEED_VALUE,
  SUBSEED_STRENGTH,
  SEED_RESIZE_FROM,
} from '@/constants/sdWebui';
import { l, lGet } from '@/utils/intl';
import { QuestionCircleOutlined } from '@ant-design/icons';
import { ProFormDigit, ProFormField, ProFormCheckbox } from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { Form, FormRule, Select, Tooltip } from 'antd';

const Seed = () => {
  const { form } = useModel('management.drawingTool.model');
  const seedAdvancedSet = Form.useWatch('seedAdvancedSet', form);
  const validateSeed = (_: FormRule, value: number) => {
    if (value === 0) {
      return Promise.reject(new Error(lGet('management.drawingTool.seedErrorTips') as string));
    }
    return Promise.resolve();
  };

  const handleDiceChange = (key: string) => {
    form.setFieldValue(key, -1);
  };
  return (
    <>
      <ProFormDigit
        label={l('management.drawingTool.seed')}
        name="seed"
        min={SEED_MIN}
        rules={[{ validator: validateSeed }]}
        colProps={{ span: 24 }}
        fieldProps={{
          precision: 0,
          addonBefore: (
            <Select
              style={{ width: 80 }}
              defaultValue={SEED_FROM_OPTIONS[0].value}
              options={SEED_FROM_OPTIONS}
            />
          ),
          addonAfter: (
            <div className="flex items-center gap-x-[20px] overflow-hidden">
              <Tooltip title={l('management.drawingTool.seedTips')}>
                <span
                  className="text-[20px] cursor-pointer shrink-0"
                  onClick={() => handleDiceChange('seed')}
                >
                  🎲
                </span>
              </Tooltip>
              <ProFormCheckbox noStyle name="seedAdvancedSet" colProps={{ span: 20 }}>
                {l('management.drawingTool.seedAdvancedSet')}{' '}
                <Tooltip title={l('management.drawingTool.seedSetTips')}>
                  <QuestionCircleOutlined className="text-[#8c8c8c]" />
                </Tooltip>
              </ProFormCheckbox>
            </div>
          ),
        }}
      />
      {seedAdvancedSet && (
        <div className="border w-full rounded-[6px] mb-[10px] p-[12px] pb-[4px] flex flex-wrap">
          <ProFormDigit
            label={l('management.drawingTool.subseed')}
            name="subseed"
            initialValue={SUB_SEED_VALUE}
            min={-Infinity}
            colProps={{ span: 12 }}
            fieldProps={{
              precision: 0,
              addonAfter: (
                <Tooltip title={l('management.drawingTool.seedTips')}>
                  <span
                    className="text-[20px] cursor-pointer"
                    onClick={() => handleDiceChange('subseed')}
                  >
                    🎲
                  </span>
                </Tooltip>
              ),
            }}
          />
          <ProFormField
            name="subseed_strength"
            initialValue={SUBSEED_STRENGTH.defaultValue}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...SUBSEED_STRENGTH}
              label={l('management.drawingTool.subseedStrength')}
            />
          </ProFormField>
          <ProFormField
            name="seed_resize_from_w"
            initialValue={SEED_RESIZE_FROM.defaultValue}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...SEED_RESIZE_FROM}
              label={l('management.drawingTool.seedResizeFromW')}
            />
          </ProFormField>
          <ProFormField
            name="seed_resize_from_h"
            initialValue={SEED_RESIZE_FROM.defaultValue}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...SEED_RESIZE_FROM}
              label={l('management.drawingTool.seedResizeFromH')}
            />
          </ProFormField>
        </div>
      )}
    </>
  );
};
export default Seed;
