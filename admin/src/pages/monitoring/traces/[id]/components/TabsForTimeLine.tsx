import { Tree, Space, Tag, Drawer } from 'antd';
import { LeftSquareOutlined } from '@ant-design/icons'
import { FC, useMemo } from 'react';
import type { TreeProps } from 'antd/lib/tree';
import { useSetState } from 'ahooks'
import { ObservationsItem } from '../index'
import ChatTrace from './ChatTrace';
/** 
 * maxValue： 最大刻度 
 * divisions 等分几份 
 * */
function generateTimeArray(maxValue: number, divisions: number) {
  const result = [];
  const step = maxValue / (divisions - 1);  // 计算每份的步长
  for (let i = 0; i < divisions; i++) {
    result.push((i * step).toFixed(2) + 's');  // 生成每个时间点并格式化为两位小数
  }
  return result;
}
interface TabsForTimeLineProps {
  observations: ObservationsItem[]
}
interface TreeDataItem extends ObservationsItem {
  key: string;
  title: React.ReactNode;
  children: TreeDataItem[];
}
const TabsForTimeLine: FC<TabsForTimeLineProps> = ({ observations }) => {
  const maxLatency = Math.max(...(observations || []).map(item => item.latency));
  const maxScale = Math.ceil(maxLatency / 2) * 2;
  const scaleList = generateTimeArray(maxScale, 9);
  const [{ drawerOpen, activeTraceInfo }, setState] = useSetState({
    drawerOpen: false,
    activeTraceInfo: {} as ObservationsItem
  })
  const treedata = useMemo(() => {
    let lastNode: TreeDataItem | null = null;
    const result: TreeDataItem[] = [];
    (observations || []).forEach((item) => {
      // 当前节点
      const newNode = {
        ...item,
        key: item.id,
        title: <Space size={4} className="group"><Tag>{item.type}</Tag>{item.name}<LeftSquareOutlined className='invisible group-hover:visible' /></Space>,
        children: []
      };
      if (lastNode) {
        lastNode.children.push(newNode);
      } else {
        result.push(newNode);
      }
      lastNode = newNode;
    });
    return result;
  }, [])
  const onSelect: TreeProps['onSelect'] = (_, info) => {
    setState({
      drawerOpen: true,
      activeTraceInfo: info.node as unknown as ObservationsItem
    })
  }
  return (
    <>
      <div className='flex gap-x-[10px]'>
        <div>
          <div className='font-semibold text-[18px] mb-[10px]'>Trace Timeline</div>
          <Tree
            treeData={treedata}
            defaultExpandAll
            showLine
            onSelect={onSelect}
          />
        </div>
        <div className='grow mr-[42px]'>
          <div className='h-[28px] w-full flex mb-[10px] relative'>
            {scaleList.map((item, index) =>
              <div key={item} className='flex-1 flex gap-x-[4px] items-center py-[4px] absolute top-0' style={{ left: index * 12.5 + '%' }}>
                <div className='w-[2px] h-[22px] bg-[#999]' />
                {item}
              </div>
            )}
          </div>
          <div className='flex flex-col gap-y-[4px]'>
            {observations.map(item => (
              <div
                className='rounded-[4px] bg-primary h-[24px] text-right pr-[4px] text-[#fff] leading-[24px]'
                style={{ width: `${(item.latency / maxScale) * 100}%` }}
              >
                {item.latency && item.latency.toFixed(2)}s
              </div>
            ))}
          </div>
        </div>
      </div>
      <Drawer title="Detail view" open={drawerOpen} width={600} onClose={() => setState({ drawerOpen: false })}>
        <ChatTrace dataSource={activeTraceInfo} />
      </Drawer>
    </>

  )
}
export default TabsForTimeLine;