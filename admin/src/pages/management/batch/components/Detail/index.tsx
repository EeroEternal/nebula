import type { TimelineProps } from 'antd';
import { isNumber } from 'lodash';
import { Drawer, Typography, Space, Tag, Timeline, Progress } from 'antd';
import { useEffect, useMemo, useState } from 'react';
import { Loader2, Clock } from 'lucide-react';

import { BATCH_STATUS_COLORS, BatchStatus } from '@/constants/batch';
import type { BatchListItem } from '@/types/Public/data';
import { calculatePercentage } from '@/utils';
import { l, lGet } from '@/utils/intl';

const { Paragraph, Text } = Typography;

type ModelDetailProps = {
  values: Partial<BatchListItem>;
  open?: boolean;
  onClose: () => void;
};
const timeKeys = ['expires_at', 'finalizing_at', 'failed_at', 'expired_at', 'cancelled_at'];
const ModelDetail: React.FC<ModelDetailProps> = ({
  values = {},
  open: propsOpen = false,
  onClose,
}) => {
  const [open, setOpen] = useState(false);
  const timelineItems = useMemo(() => {
    const items: TimelineProps['items'] = [
      {
        children: (
          <Space>
            {lGet('tasks.batch.createAt')}
            {values?.created_at ? <Text italic>{values?.created_at}</Text> : '-'}
          </Space>
        ),
      },
      {
        children: (
          <Space>
            {values?.in_progress_at
              ? lGet('tasks.batch.inProgressAt')
              : lGet('tasks.batch.inProgress')}
            {values?.in_progress_at ? (
              <Text italic>{values?.in_progress_at}</Text>
            ) : (
              <Loader2 size={14} className="animate-spin text-primary" />
            )}
          </Space>
        ),
      },
    ];
    timeKeys.forEach((item) => {
      if (values[item as keyof BatchListItem]) {
        const timeKey = item.slice(0, -3) + 'At';
        items.push({
          children: (
            <Space>
              <Text>{lGet(`tasks.batch.${timeKey}`)}</Text>
              <Text italic>{values[item as keyof BatchListItem]}</Text>
            </Space>
          ),
        });
      }
    });
    items.push({
      dot: <Clock size={16} />,
      color: values?.completed_at ? 'green' : 'gray',
      children: (
        <Space>
          <Text className={`${values?.completed_at ? 'text-default font-bold' : 'text-muted/50'}`}>
            {lGet('tasks.batch.completed')}
          </Text>
          {values?.completed_at && <Text italic>{values?.completed_at}</Text>}
        </Space>
      ),
    });
    return items;
  }, [values]);
  const handleClose = () => {
    setOpen(false);
    onClose();
  };
  useEffect(() => setOpen(propsOpen), [propsOpen]);
  return (
    <Drawer
      title={l('tasks.batch.detail')}
      width={'60%'}
      open={open}
      maskClosable
      onClose={handleClose}
    >
      <Space direction="vertical">
        <Text>
          {l('tasks.batch.endpoint')}：<span>{values?.endpoint}</span>
        </Text>
        <Text>
          {l('tasks.batch.completionWindow')}：<span>{values?.completion_window}</span>
        </Text>
        {values?.status && (
          <Text>
            {l('tasks.batch.status')}：
            <Tag
              bordered={false}
              color={
                BATCH_STATUS_COLORS[values?.status as keyof typeof BATCH_STATUS_COLORS] || 'default'
              }
            >
              {l(`tasks.batch.status.${BatchStatus[values?.status]}`)}
            </Tag>
          </Text>
        )}
        {values?.input_file_id && (
          <>
            <div>
              {l('tasks.batch.inputFileId')}：<span>{values?.input_file_id}</span>
            </div>
            <Paragraph copyable={{ text: values?.input_file_url }}>
              {l('tasks.batch.inputFileUrl')}： <span>{values?.input_file_url}</span>
            </Paragraph>
          </>
        )}

        {values?.output_file_id && (
          <>
            <div>
              {l('tasks.batch.outputFileId')}：<span>{values?.output_file_id}</span>
            </div>
            <Paragraph copyable={{ text: values?.output_file_url }}>
              {l('tasks.batch.outputFileUrl')}： <span>{values?.output_file_url}</span>
            </Paragraph>
          </>
        )}

        {values?.error_file_id && (
          <>
            <div>
              {l('tasks.batch.errorFileId')}：<span>{values?.error_file_id}</span>
            </div>
            <Paragraph copyable={{ text: values?.output_file_url }}>
              {l('tasks.batch.errorFileUrl')}： <span>{values?.error_file_url}</span>
            </Paragraph>
          </>
        )}
        <div className="flex items-center">
          <span className="shrink-0">{l('tasks.batch.progress')}：</span>
          {isNumber(values?.request_total) && isNumber(values?.request_completed) ? (
            <Progress
              percent={calculatePercentage(values?.request_completed, values?.request_total)}
              strokeColor={window.THEME_PRIMARY_COLOR}
              className="!leading-[0]"
            />
          ) : (
            '-'
          )}
        </div>
        <div>
          {l('tasks.batch.requestTotal')}：{values?.request_completed || 0} /{' '}
          {values?.request_total || 0}
        </div>
        <div className="mb-2">
          {l('tasks.batch.requestFailed')}：{values?.request_failed || 0}
        </div>
        <Timeline pending={false} items={timelineItems} />
      </Space>
    </Drawer>
  );
};
export default ModelDetail;
