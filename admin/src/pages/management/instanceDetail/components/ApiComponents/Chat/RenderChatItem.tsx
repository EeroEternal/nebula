import { Button, Spin, Input, Space } from 'antd';
import type { InputRef } from 'antd';
import { useSetState } from 'ahooks';
import classNames from 'classnames';
import {
  AliwangwangFilled,
  UserOutlined,
  CopyOutlined,
  SlackOutlined,
  UpOutlined,
  EditOutlined,
} from '@ant-design/icons';
import { FC, useRef } from 'react';

import { ReactMarkdown, IconFont, DrawerAttachment } from '@/components';
import { copyToClipboard, isNumber, sleep } from '@/utils';
import { ChatItem } from './index';
import { l } from '@/utils/intl';

interface RenderChatItemProps {
  data: ChatItem;
  currentIndex: number;
  onEdit: (newRecord: ChatItem, recordIndex: number) => void;
}
const RenderChatItem: FC<RenderChatItemProps> = ({ data, currentIndex, onEdit }) => {
  const { role, content, loading, success, fileInfo, thinkingContent, thinkingCompleted, usage } =
    data;
  const [{ showMark, thinkingOpen, isEditing, questionValue }, setState] = useSetState({
    showMark: true,
    thinkingOpen: true,
    isEditing: false,
    questionValue: content,
  });
  const questionInputRef = useRef<InputRef>(null);
  const handleEdit = async () => {
    setState({ isEditing: true, questionValue: content });
    await sleep(100);
    questionInputRef.current?.focus?.({
      cursor: 'end',
    });
  };
  const handleCancel = () => {
    setState({ isEditing: false, questionValue: content });
  };
  const handleQuestionChange = (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    setState({ questionValue: e.target.value });
  };
  const handleSend = () => {
    setState({ isEditing: false });
    onEdit({ ...data, content: questionValue }, currentIndex);
  };
  const handleCopy = () => {
    copyToClipboard(content);
  };
  const renderFileInfo = () => {
    if (!fileInfo?.url) return null;
    return (
      <DrawerAttachment
        type={fileInfo.type}
        url={fileInfo.url}
        className={classNames({
          '!w-[300px] h-[40px]': fileInfo.type === 'audio',
        })}
      />
    );
  };
  if (role === 'user') {
    return (
      <div className="justify-end flex gap-x-[10px] fade-in  ml-[42px]">
        <div className="flex flex-col items-end w-full gap-y-[8px] group cursor-pointer">
          {isEditing ? (
            <>
              <div className="bg-[var(--c-primary-light)] w-2/3 border rounded-[8px] border-primary p-[10px] flex flex-col items-end">
                <Input.TextArea
                  ref={questionInputRef}
                  variant="borderless"
                  value={questionValue}
                  onChange={handleQuestionChange}
                  autoSize={{ minRows: 4, maxRows: 15 }}
                  placeholder={l('model.running.placeholder')}
                />
                <Space>
                  <Button shape="round" onClick={handleCancel}>
                    {l('global.actions.cancel')}
                  </Button>
                  <Button
                    shape="round"
                    disabled={questionValue === content || !questionValue}
                    type="primary"
                    onClick={handleSend}
                  >
                    {l('global.actions.send')}
                  </Button>
                </Space>
              </div>
              {renderFileInfo()}
            </>
          ) : (
            <>
              {renderFileInfo()}
              <div className="p-[8px] bg-primary text-[#fff] rounded-[10px] rounded-tr-none">
                {content}
              </div>
              <div className="invisible group-hover:visible">
                <Button
                  className="border-[#fff]"
                  shape="circle"
                  icon={<CopyOutlined />}
                  onClick={handleCopy}
                />
                <Button
                  className="border-[#fff]"
                  shape="circle"
                  icon={<EditOutlined />}
                  onClick={handleEdit}
                />
              </div>
            </>
          )}
        </div>
        <div className="shrink-0 bg-[#f0f0f0] w-[32px] h-[32px] rounded-full flex items-center justify-center">
          <UserOutlined />
        </div>
      </div>
    );
  }
  if (role === 'system') {
    return (
      <div className="justify-start flex gap-x-[10px] fade-in max-w-[calc(100%-42px)]">
        <div className="shrink-0 bg-[#f0f0f0] w-[32px] h-[32px] rounded-full flex items-center justify-center text-[11px] text-muted">
          SYS
        </div>
        <div className="p-[8px] bg-[#f5f5f5] rounded-[10px] rounded-tl-none">
          <div className="text-[12px] text-muted mb-[4px]">system</div>
          {content}
        </div>
      </div>
    );
  }
  const handleThinking = () => {
    setState({ thinkingOpen: !thinkingOpen });
  };
  const renderThinking = () => {
    if (!thinkingContent?.trim()) return null;
    return (
      <div>
        <div
          className="w-fit flex items-center justify-center gap-x-[4px] p-[8px] bg-[#f5f5f5] rounded-[8px] cursor-pointer hover:bg-[#ededed]"
          onClick={handleThinking}
        >
          <Spin indicator={<SlackOutlined spin={!thinkingCompleted} />} size="small" />
          <span>
            {thinkingCompleted
              ? l('model.running.chat.thinkingCompleted')
              : `${l('model.running.chat.thinking')}...`}
          </span>
          <UpOutlined
            className={`ml-[4px] text-[12px] transition-out rotate-${thinkingOpen ? 0 : 180}`}
          />
        </div>
        {thinkingOpen && (
          <div className="flex gap-x-[10px] mt-[10px] text-[#999] break-word">
            <div className="border-l-[2px] border-[#e5e5e5]" />
            <ReactMarkdown>{thinkingContent}</ReactMarkdown>
          </div>
        )}
      </div>
    );
  };

  const renderAnswerFooter = () => {
    if (!success) return null;
    const tokens =
      isNumber(usage?.prompt_tokens) &&
      isNumber(usage?.completion_tokens) &&
      isNumber(usage?.total_tokens)
        ? `${usage?.prompt_tokens} → ${usage?.completion_tokens} (∑ ${usage?.total_tokens})`
        : '';
    // 输出内容为空时
    if (usage?.completion_tokens === 0) {
      return <div>{tokens}</div>;
    }
    return (
      <div className="pt-[10px] border-t flex gap-x-[10px] justify-between">
        <div>{!!(usage?.total_tokens && usage.total_tokens > 0) && tokens}</div>
        <div className="flex items-center">
          <IconFont
            name="icon-markdown"
            className={classNames('text-[18px] mr-[10px] hover:text-primary', {
              'text-primary': !showMark,
            })}
            onClick={() => setState({ showMark: !showMark })}
          />
          <CopyOutlined className="hover:text-primary" onClick={() => copyToClipboard(content)} />
        </div>
      </div>
    );
  };
  return (
    <div className="justify-start flex gap-x-[10px] max-w-[calc(100%-42px)]">
      <div className="shrink-0 bg-[#f0f0f0] w-[32px] h-[32px] rounded-full flex items-center justify-center">
        <AliwangwangFilled />
      </div>
      <div className="flex flex-col gap-y-[10px]">
        {renderThinking()}
        <div className="p-[8px] bg-[var(--c-primary-light)] rounded-[10px] rounded-tl-none w-fit">
          {loading && !content ? (
            <Spin size="small" className="px-[10px]" />
          ) : (
            <div className="flex flex-col gap-y-[10px]">
              {content
                ? showMark
                  ? <ReactMarkdown>{content}</ReactMarkdown>
                  : <div>{content}</div>
                : null}
              {renderFileInfo()}
              {renderAnswerFooter()}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default RenderChatItem;
