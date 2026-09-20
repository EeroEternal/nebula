import { FC } from 'react';
import { CaretRightOutlined, QuestionCircleOutlined } from '@ant-design/icons';
import { ProFormCheckbox } from '@ant-design/pro-components';
import { Collapse, Tabs, Tooltip } from 'antd';
import { l } from '@/utils/intl';
import ADetailerUnit from './ADetailerUnit';
import ControlNetUnit from './ControlNetUnit';

const panelStyle = {
  borderRadius: 8,
  border: '1px solid #d9d9d9',
};
const panelStyles = {
  header: {
    padding: 10,
    fontWeight: 600,
  },
  body: {
    padding: '0 10px',
  },
};
const baseCollapsePanel = {
  style: panelStyle,
  styles: panelStyles,
  forceRender: true,
};

const ExtendCollapse: FC = () => {

  const ADetailerTabItems = [
    {
      key: 'unit1',
      label: `${l('management.drawingTool.adetailer.tab')}1`,
      children: <ADetailerUnit index={1}  />,
    },
    {
      key: 'unit2',
      label: `${l('management.drawingTool.adetailer.tab')}2`,
      children: <ADetailerUnit index={2} />,
    },
  ];
  const ControlNetTabItems = [
    {
      key: 'unit1',
      label: `${l('management.drawingTool.controlNet.tab')}1`,
      children: <ControlNetUnit index={0}  />,
    },
    {
      key: 'unit2',
      label: `${l('management.drawingTool.controlNet.tab')}2`,
      children: <ControlNetUnit index={1} />,
    },
    {
      key: 'unit3',
      label: `${l('management.drawingTool.controlNet.tab')}3`,
      children: <ControlNetUnit index={2} />,
    },
    {
      key: 'unit4',
      label: `${l('management.drawingTool.controlNet.tab')}4`,
      children: <ControlNetUnit index={3} />,
    },
  ];
  const collapseItems = [
    {
      ...baseCollapsePanel,
      key: 'ADetailer',
      label: (
        <span className="flex gap-x-[4px]">
          ADetailer
          <Tooltip title={l('management.drawingTool.adetailerTips')}>
            <QuestionCircleOutlined className="text-[#8c8c8c]" />
          </Tooltip>
        </span>
      ),
      children: (
        <>
          <ProFormCheckbox name={['alwayson_scripts', 'ADetailer', 'args', 0]}>
            {l('management.drawingTool.adetailer')}
          </ProFormCheckbox>  
          <Tabs animated className="w-full" type="card" items={ADetailerTabItems} />
        </>
      ),
    },
    {
      ...baseCollapsePanel,
      key: 'ControlNet',
      label: 'ControlNet',
      children: <Tabs animated className="w-full" type="card" items={ControlNetTabItems} />,
    },
  ];
  return (
    <Collapse
      bordered={false}
      className="w-full flex flex-col gap-y-[20px] bg-card mt-[10px]"
      expandIcon={({ isActive }) => <CaretRightOutlined rotate={isActive ? 90 : 0} />}
      expandIconPosition="end"
      items={collapseItems}
    />
  );
};
export default ExtendCollapse;
