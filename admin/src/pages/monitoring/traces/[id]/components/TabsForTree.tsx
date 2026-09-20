import { FC, useEffect, useState } from 'react';
import { Tree, Tag, Space } from 'antd';
import type { TreeProps } from 'antd/lib/tree';
import ChatTrace from './ChatTrace';
import { ObservationsItem } from '../index';

interface TabsForTreeProps {
  observations: ObservationsItem[];
}
interface TreeDataItem extends ObservationsItem {
  key: string;
  title: React.ReactNode;
  children: TreeDataItem[];
}
const TabsForTree: FC<TabsForTreeProps> = ({ observations }) => {
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [treedata, setTreedata] = useState<TreeDataItem[]>([]);
  const [activeTraceInfo, setActiveTraceInfo] = useState<ObservationsItem>({} as TreeDataItem);
  const onSelect: TreeProps['onSelect'] = (selectedKeys, info) => {
    setSelectedKeys(selectedKeys as string[]);
    setActiveTraceInfo(info.node as unknown as TreeDataItem);
  };
  useEffect(() => {
    let lastNode: TreeDataItem | null = null;
    const result: TreeDataItem[] = [];
    (observations || []).forEach((item) => {
      // 当前节点
      const newNode = {
        ...item,
        key: item.id,
        title: (
          <div className="p-[4px] flex flex-col">
            <Space size={4}>
              <Tag>{item.type}</Tag>
              {item.name}
            </Space>
            <Space size={4} className="text-[#666]">
              {`${item.latency.toFixed(2)}s`}
              <div className="whitespace-nowrap">{`${item?.usage?.input} → ${item?.usage?.output} (∑ ${item?.usage?.total})`}</div>
            </Space>
          </div>
        ),
        children: [],
      };
      if (lastNode) {
        lastNode.children.push(newNode);
      } else {
        result.push(newNode);
      }
      lastNode = newNode;
    });

    if (result?.[0]?.key) {
      setSelectedKeys([result[0].key]);
      setActiveTraceInfo(result[0]);
    }
    setTreedata(result);
  }, [observations]);
  return (
    <div className="flex gap-x-[20px] justify-between">
      <ChatTrace dataSource={activeTraceInfo} />
      {treedata.length > 0 && (
        <Tree
          className="shrink-0"
          treeData={treedata}
          defaultExpandAll
          showLine
          selectedKeys={selectedKeys}
          onSelect={onSelect}
        />
      )}
    </div>
  );
};
export default TabsForTree;
