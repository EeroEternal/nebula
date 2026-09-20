import { InputNumberWithSlider } from '@/components';
import {
  CLIP,
  TXT_2_IMG_INIT_VALUE,
  IMG_2_IMG_INIT_VALUE,
} from '@/constants/sdWebui';
import { l } from '@/utils/intl';
import { ProForm, ProFormSelect, ProFormText } from '@ant-design/pro-components';
import { Tabs } from 'antd';
import { useModel } from '@umijs/max';
import { ModelState } from './model';
import Txt2ImgTabPanel from './components/Txt2ImgTabPanel';
import Img2ImgTabPanel from './components/Img2ImgTabPanel';
import styles from './index.less';
import { useEffect } from 'react';

const AIDrawingTool = () => {
  const { initialState } = useModel('@@initialState');
  const { globalReady } = initialState || {};
  const { form, modelOptions, activeAbilityTabKey, initLoad, updateState } = useModel(
    'management.drawingTool.model',
  );
  const modelAbilityTabItems = [
    {
      key: 'txt2img',
      label: l('management.drawingTool.tabs.txt2img'),
      children: <Txt2ImgTabPanel />,
      destroyOnHidden: true,
    },
    {
      key: 'img2img',
      label: l('management.drawingTool.tabs.img2img'),
      children: <Img2ImgTabPanel />,
      destroyOnHidden: true,
    },
  ];
  const handleTabChange = (key: string) => {
    const model = form.getFieldValue('model');
    updateState({ activeAbilityTabKey: key as ModelState['activeAbilityTabKey'] });
    const values = {
      model,
      ...(key === 'img2img' ? IMG_2_IMG_INIT_VALUE : TXT_2_IMG_INIT_VALUE),
    };
    form.setFieldsValue(values);
  };

  useEffect(() => {
    if (globalReady) initLoad();
  }, [globalReady]);
  return (
    <div className="bg-card m-[-24px] p-[24px]">
      <ProForm
        className={styles['drawing-form']}
        form={form}
        submitter={false}
        grid
        autoFocusFirstInput={false}
        scrollToFirstError
        initialValues={TXT_2_IMG_INIT_VALUE}
      >
        <ProFormSelect
          label={l('management.drawingTool.model')}
          name="model"
          options={modelOptions}
          colProps={{ span: 8 }}
        />
        <ProFormText name={['override_settings', 'clip_skip']} colProps={{ span: 8 }}>
          <InputNumberWithSlider {...CLIP} label={l('management.drawingTool.clipSkip')} />
        </ProFormText>
        <Tabs
          animated
          className="w-full"
          activeKey={activeAbilityTabKey}
          onChange={handleTabChange}
          type="card"
          items={modelAbilityTabItems}
          destroyOnHidden={true}
        />
      </ProForm>
    </div>
  );
};
export default AIDrawingTool;
