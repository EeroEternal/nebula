import { Input, Button, Segmented, Switch, Tooltip } from 'antd';
import type { SwitchProps } from 'antd';
import { ArrowDown, Send, BrushCleaning } from 'lucide-react';
import { useRequest, useSetState } from 'ahooks';
import classNames from 'classnames';
import { useEffect, useRef, useState } from 'react';
import { isEmpty, isObject, get } from 'lodash';
import type { TextAreaRef } from 'antd/es/input/TextArea';
import { TextAreaProps } from 'antd/lib/input/TextArea';
import { l } from '@/utils/intl';
import { sleep } from '@/utils';
import {
  postEventStreamFetcher,
  PostEventStreamFetcherOptions,
  EventStreamController,
} from '@/utils/eventStream';
import { ChatStreamResult, ChatChoicesMessage } from '@/types/Public/data';
import { ModelAbility } from '@/constants/modelData';
import useScrollBottomDetection from '@/hooks/useScrollBottomDetection';
import RenderChatItem from './RenderChatItem';
import ChatTips from './ChatTips';
import UploadAttachment from './UploadAttachment';
import RenderAttachment from './RenderAttachment';
import ExtendParamsModal, { ChatExtendParams } from './ExtendParamsModal';
import TryToAPI from '../../TryToAPI';
import { useModel } from '@umijs/max';
import {
  applyChatStreamChunk,
  orderMessagesSystemFirst,
  upsertSystemMessage,
} from './chatDisplay';
import { buildChatRequestParams, resolveChatMaxTokens } from './chatRequestParams';
import request from '@/utils/request';

const { TextArea } = Input;

const roleTypeOptions = [
  { label: 'user', value: 'user' },
  { label: 'system', value: 'system' },
];
const mediaTypeMap = {
  audio: 'audio/mp3',
  image: 'image/jpeg',
  video: 'video/mp4',
} as const;
export interface ChatItem {
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinkingContent?: string;
  thinkingCompleted?: boolean;
  inThinkBlock?: boolean;
  loading: boolean;
  success?: boolean;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
  fileInfo?: FileInfo;
}
export interface FileInfo {
  url: string;
  name?: string;
  size?: string;
  type: 'image' | 'video' | 'audio';
}
interface ChatState {
  roleType: 'user' | 'system';
  textValue: string;
  loading: boolean;
  extendParams: ChatExtendParams;
  fileInfo?: FileInfo;
  chatController: EventStreamController | null;
}

const Chat = () => {
  const { instanceDetail, isStream, replicaId, updateState } = useModel(
    'management.instanceDetail.model',
  );
  const { model_ability, model_uid, peft_model_config, context_length, kwargs } =
    instanceDetail;
  const handleStreamChange: SwitchProps['onChange'] = (checked) =>
    updateState({ isStream: checked });
  const showAttachment = model_ability.includes(ModelAbility.vision);
  const showTools = model_ability.includes(ModelAbility.tools);
  const showThinking =
    model_ability.includes(ModelAbility.reasoning) ||
    model_ability.includes(ModelAbility.hybrid);
  const { data: modelDesc } = useRequest(
    () =>
      request(`/models/${encodeURIComponent(model_uid)}`, {
        skipNotification: true,
      }),
    { ready: !!model_uid, refreshDeps: [model_uid] },
  );
  const catalogLen = Number(
    modelDesc?.data?.context_length ??
      modelDesc?.context_length ??
      context_length,
  );
  const maxTokensCap = resolveChatMaxTokens({
    context_length: catalogLen,
    kwargs: kwargs as Record<string, unknown> | undefined,
  });
  const chatContentRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const textAreaRef = useRef<TextAreaRef & { input?: HTMLTextAreaElement }>(null);
  const showScrollToBottom = useScrollBottomDetection(scrollContainerRef, 200);

  // 避免 autoFocus 触发浏览器 scrollIntoView，切 Tab 时整页大幅跳动
  useEffect(() => {
    const el =
      textAreaRef.current?.resizableTextArea?.textArea ||
      textAreaRef.current?.input;
    el?.focus({ preventScroll: true });
  }, []);
  const loraOptions = (peft_model_config?.lora_list || []).map((item) => ({
    label: item.lora_name,
    value: item.lora_name,
  }));

  const [chatList, setChatList] = useState<ChatItem[]>([]);
  const [
    { roleType, textValue, extendParams, loading, fileInfo, chatController },
    setState,
  ] = useSetState<ChatState>({
    roleType: 'user',
    textValue: '',
    extendParams: {
      temperature: 1,
      max_tokens: Math.min(512, maxTokensCap),
      top_k: 1,
      enable_thinking: showThinking ? true : undefined,
      lora_name: loraOptions?.[0]?.value,
    },
    loading: false,
    fileInfo: undefined,
    chatController: null,
  });
  const handleRoleChange = (value: string | number) => {
    const next = value === 'system' ? 'system' : 'user';
    if (next === 'system') {
      const existing = chatList.find((item) => item.role === 'system');
      if (existing && !textValue) {
        setState({ roleType: 'system', textValue: existing.content });
      } else {
        setState({ roleType: 'system' });
      }
      return;
    }
    setState({ roleType: 'user' });
  };
  // 提取message中的媒体信息（base64）
  const transformFileInfoForResult = (message: ChatChoicesMessage) => {
    if (!message) return undefined;
    // 遍历媒体类型并检查是否存在
    for (const [type, mimeType] of Object.entries(mediaTypeMap)) {
      if (message[type as keyof typeof mediaTypeMap]) {
        if (get(message, [type, 'data'])) {
          return {
            type: type as FileInfo['type'],
            url: `data:${mimeType};base64,${get(message, [type, 'data'])}`,
          };
        }
        return undefined;
      }
    }
    return undefined;
  };

  const onData = async (chunk: ChatStreamResult) => {
    setChatList((prevList) => {
      const lastChatItem = prevList[prevList.length - 1];
      if (!lastChatItem) {
        return prevList;
      }
      const next = applyChatStreamChunk(
        {
          content: lastChatItem.content || '',
          thinkingContent: lastChatItem.thinkingContent || '',
          thinkingCompleted: lastChatItem.thinkingCompleted ?? true,
          usage: lastChatItem.usage,
          inThinkBlock: lastChatItem.inThinkBlock,
        },
        chunk,
        isStream,
      );
      const updatedLastItem = {
        ...lastChatItem,
        usage: next.usage,
        content: next.content,
        // 仅在「尚未收到任何思考/正文」时转圈；有内容后持续流式展示
        loading: next.content.length === 0 && next.thinkingContent.length === 0,
        success: true,
        thinkingContent: next.thinkingContent,
        thinkingCompleted: next.thinkingCompleted,
        inThinkBlock: next.inThinkBlock,
        fileInfo: transformFileInfoForResult(chunk?.choices?.[0]?.message as ChatChoicesMessage),
      };
      return [...prevList.slice(0, -1), updatedLastItem];
    });
    chatContentRef.current?.scrollIntoView({
      behavior: 'smooth',
      block: 'end',
    });
  };
  const onError: PostEventStreamFetcherOptions<ChatStreamResult>['onError'] = (msg) => {
    setChatList((prevList) => {
      return [
        ...prevList.slice(0, -1),
        {
          ...prevList[prevList.length - 1],
          content: msg,
          thinkingContent: '', // 有报错的话，把思考隐藏掉，只显示报错原因
          thinkingCompleted: true,
          loading: false,
          success: false,
        },
      ];
    });
  };
  const onEnd = () => {
    setChatList((prevList) => {
      const lastChatItem = prevList[prevList.length - 1];
      if (!lastChatItem || lastChatItem.role !== 'assistant') {
        return prevList;
      }
      return [
        ...prevList.slice(0, -1),
        { ...lastChatItem, thinkingCompleted: true, loading: false },
      ];
    });
    setState({ loading: false, chatController: null });
  };
  const pushChat = async (data: string | ChatItem, recordIndex?: number) => {
    const sendingRole = isObject(data) ? (data as ChatItem).role : roleType;
    const newItem: ChatItem = isObject(data)
      ? (data as ChatItem)
      : {
          role: sendingRole,
          content: data,
          loading: false,
          ...(sendingRole === 'user' && fileInfo ? { fileInfo } : {}),
        };
    const newChatList = isObject(data)
      ? sendingRole === 'system'
        ? upsertSystemMessage(chatList, newItem)
        : [...chatList.slice(0, recordIndex), newItem]
      : sendingRole === 'system'
        ? upsertSystemMessage(chatList, newItem)
        : [...chatList, newItem];
    setChatList(newChatList);
    await sleep(10);
    chatContentRef.current?.scrollIntoView({
      behavior: 'smooth',
      block: 'end',
    });
    // system：只写入一条置顶 system，发完自动切回 user，避免一直停在 system 导致后续对话不请求
    if (sendingRole === 'system') {
      setState({ textValue: '', roleType: 'user' });
      return;
    }
    setState({ textValue: '' });
    const newController = new EventStreamController();
    setState({ loading: true, fileInfo: undefined, chatController: newController });
    // 回答，content为空，设置loading
    setChatList([
      ...newChatList,
      {
        role: 'assistant',
        content: '',
        loading: true,
        thinkingCompleted: true,
      },
    ]);
    await sleep(10);
    chatContentRef.current?.scrollIntoView({
      behavior: 'smooth',
      block: 'end',
    });
    await postEventStreamFetcher<ChatStreamResult>(
      {
        url: '/chat/completions',
        data: {
          model: model_uid,
          ...buildChatRequestParams(extendParams, {
            showThinking,
            maxTokensCap,
          }),
          messages: orderMessagesSystemFirst(
            newChatList.map((item) => {
              if (item.role === 'user' && !isEmpty(item.fileInfo)) {
                return {
                  role: item.role,
                  content: [
                    { type: 'text', text: item.content },
                    {
                      type: `${item.fileInfo?.type}_url`,
                      [`${item.fileInfo?.type}_url`]: { url: item.fileInfo?.url },
                    },
                  ],
                };
              }
              return { role: item.role, content: item.content };
            }),
          ),
          stream: isStream,
          // vLLM rejects stream_options unless stream=true
          ...(isStream ? { stream_options: { include_usage: true } } : {}),
          replica_id: replicaId || undefined,
        },
        options: {
          onData,
          onError,
          onEnd,
        },
      },
      newController,
    );
  };
  const onEnter: TextAreaProps['onPressEnter'] = (e) => {
    // 解决回车换行，不要触发chat
    if (e.shiftKey && e.key === 'Enter') {
      return;
    }
    e.stopPropagation();
    e.preventDefault();
    // 不要和上方return条件合并
    if (loading || !e.currentTarget.value) {
      return;
    }
    pushChat(e.currentTarget.value);
  };
  // enter按钮
  const handleEnter = () => {
    if (loading || !textValue) return;
    pushChat(textValue);
  };
  const settingExtendParams = (values: ChatExtendParams) => {
    setState({
      extendParams: values,
    });
  };

  const onUploadChange = (fileInfo: FileInfo | undefined) => {
    setState({ fileInfo });
  };
  const deleteFile = () => {
    if (loading) {
      return;
    }
    setState({ fileInfo: undefined });
  };
  const handleClose = () => {
    if (!chatController) return;
    chatController.terminate(); // 调用 terminate 方法终止请求
    setChatList((prevList) => {
      return [
        ...prevList.slice(0, -1),
        {
          ...prevList[prevList.length - 1],
          thinkingCompleted: true,
          success: true,
          loading: false,
        },
      ];
    });
    setState({ chatController: null, loading: false });
  };
  const handleClear = () => {
    if (loading && chatController) {
      chatController.terminate();
    }
    setChatList([]);
  };
  const editRecord = (newRecord: ChatItem, recordIndex: number) => {
    if (loading) return;
    pushChat(newRecord, recordIndex);
  };
  const scrollToBottom = () => {
    chatContentRef.current?.scrollIntoView({
      behavior: 'smooth',
      block: 'end',
    });
  };
  return (
    <div
      className={classNames(
        'p-4 flex flex-col min-h-[320px]',
        // 预留顶栏/面包屑/SubNav/面板头与底部余量，避免对话区撑破视口导致整页滚动跳动
        'h-[calc(100dvh-420px)] max-h-[calc(100dvh-420px)]',
      )}
    >
      <div className="flex-grow overflow-y-auto" ref={scrollContainerRef}>
        <div className="mr-[10px] flex flex-col gap-y-[20px]">
          <ChatTips />
          {chatList.map((item, index) => (
            <RenderChatItem
              data={item}
              currentIndex={index}
              onEdit={editRecord}
              key={`${item.role}-${index}`}
            />
          ))}
        </div>
        <div ref={chatContentRef}></div>
      </div>
        <div className="border-t -mx-4 relative p-4 pb-0 flex flex-col gap-y-2">
          {showScrollToBottom && (
            <Button
              className="absolute left-[50%] bottom-[100%] mb-5 -ml-4 z-[100] bg-card"
              shape="circle"
              icon={<ArrowDown size={14} />}
              onClick={scrollToBottom}
            />
          )}
          {!!fileInfo && <RenderAttachment fileInfo={fileInfo} deleteFile={deleteFile} />}
          <TextArea
            ref={textAreaRef}
            size="large"
            variant="borderless"
            autoSize={{
              minRows: 2,
              maxRows: 4,
            }}
            placeholder={
              roleType === 'system'
                ? l('model.running.chat.systemPlaceholder')
                : l('model.running.placeholder')
            }
            value={textValue}
            onPressEnter={onEnter}
            onChange={(e) => setState({ textValue: e.target.value })}
          />
          <div className="flex justify-between items-center gap-2">
            <div className="flex gap-1 items-center min-w-0 flex-wrap">
              <Segmented options={roleTypeOptions} value={roleType} onChange={handleRoleChange} />
              <ExtendParamsModal
                loraOptions={loraOptions}
                showTools={showTools}
                showThinking={showThinking}
                maxTokens={maxTokensCap}
                initialValues={{
                  ...extendParams,
                  temperature: extendParams.temperature ?? 1,
                  top_k: extendParams.top_k ?? 1,
                  max_tokens: extendParams.max_tokens ?? Math.min(512, maxTokensCap),
                  enable_thinking: showThinking
                    ? (extendParams.enable_thinking ?? true)
                    : undefined,
                  tools:
                    extendParams.tools && typeof extendParams.tools === 'object'
                      ? JSON.stringify(extendParams.tools, null, 2)
                      : extendParams.tools,
                }}
                submit={settingExtendParams}
              />
              {showAttachment && (
                <UploadAttachment
                  fileInfo={fileInfo}
                  onChange={onUploadChange}
                  disabled={loading}
                />
              )}
              <Tooltip title={l('model.running.chat.clearChat')}>
                <Button
                  className="border-[#fff]"
                  shape="circle"
                  icon={<BrushCleaning size={14} className="rotate-45" />}
                  onClick={handleClear}
                />
              </Tooltip>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <TryToAPI size="small" />
              <div className="flex items-center gap-1">
                <span className="text-sm text-default whitespace-nowrap">
                  {l('models.instances.detail.stream')}
                </span>
                <Switch size="small" checked={isStream} onChange={handleStreamChange} />
              </div>
              {!!chatController ? (
                <Button type="primary" onClick={handleClose} shape="circle">
                  <div className="bg-card w-3 h-3 rounded-[2px]" />
                </Button>
              ) : (
                <Button
                  type="primary"
                  onClick={handleEnter}
                  loading={loading}
                  shape="circle"
                  icon={<Send size={14} />}
                />
              )}
            </div>
          </div>
        </div>
    </div>
  );
};
export default Chat;
