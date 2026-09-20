import { FC, useMemo, useState } from 'react';
import classNames from 'classnames';
import { Collapse, Segmented, Tag } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import { useSetState } from 'ahooks';
import JSONView from '@uiw/react-json-view';
import { isString } from 'lodash';
import { ReactMarkdown, IconFont, DrawerAttachment } from '@/components';
import type { AttachmentType } from '@/components/DrawerAttachment';
import { copyToClipboard, formatDisplayTime } from '@/utils';
import { l } from '@/utils/intl';
import type { ObservationsItem } from '../index';

interface ChatTraceRenderProps {
  dataSource: ObservationsItem;
}

type ContentPart = {
  type: string;
  text?: string;
  url?: string;
  mediaType?: AttachmentType;
};

const extractMediaUrl = (item: Record<string, unknown>): string | undefined => {
  const t = item?.type;
  if (typeof t !== 'string' || !t) return undefined;
  const raw = item[t];
  if (typeof raw === 'string' && raw) return raw;
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
    const obj = raw as Record<string, unknown>;
    if (typeof obj.url === 'string') return obj.url;
    if (t === 'input_audio' && obj.data) {
      const fmt = obj.format || 'wav';
      return `data:audio/${String(fmt)};base64,${String(obj.data)}`;
    }
  }
  if (typeof item.url === 'string' && item.url) return item.url;
  return undefined;
};

const mediaTypeOf = (type: string): AttachmentType | undefined => {
  if (type.startsWith('image')) return 'image';
  if (type.startsWith('video')) return 'video';
  if (type.startsWith('audio') || type === 'input_audio') return 'audio';
  return undefined;
};

const normalizeParts = (content: unknown): ContentPart[] | null => {
  if (!Array.isArray(content)) return null;
  return content.map((raw) => {
    const item = (raw || {}) as Record<string, unknown>;
    const type = String(item.type || 'text');
    if (type === 'text') {
      return { type: 'text', text: String(item.text ?? item.content ?? '') };
    }
    const url = extractMediaUrl(item);
    return {
      type,
      url,
      mediaType: mediaTypeOf(type),
      text: item.text == null ? undefined : String(item.text),
    };
  });
};

const formatJson = (value: unknown) => {
  try {
    if (typeof value === 'string') {
      return JSON.stringify(JSON.parse(value), null, 2);
    }
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value ?? '');
  }
};

type ToolCall = {
  id?: string;
  name?: string;
  arguments?: unknown;
  function?: { name?: string; arguments?: unknown };
};

const ToolCallsBlock: FC<{ toolCalls: unknown[]; pretty: boolean }> = ({
  toolCalls,
  pretty,
}) => {
  if (!Array.isArray(toolCalls) || !toolCalls.length) return null;
  return (
    <div className="flex flex-col gap-y-2 mt-2">
      {toolCalls.map((raw, i) => {
        const tc = (raw || {}) as ToolCall;
        const name = tc.function?.name || tc.name || `tool_${i}`;
        const args = tc.function?.arguments ?? tc.arguments ?? {};
        const body = pretty ? (
          <pre className="text-[12px] whitespace-pre-wrap m-0 font-mono">
            {formatJson(args)}
          </pre>
        ) : (
          <JSONView
            value={typeof args === 'string' ? { arguments: args } : args || {}}
            displayDataTypes={false}
            displayObjectSize={false}
          />
        );
        return (
          <div
            key={tc?.id || i}
            className="rounded border border-amber-200 bg-amber-50 px-3 py-2"
          >
            <div className="text-[12px] font-semibold mb-1">
              {l('monitor.traces.toolCall')}: {name}
              {tc?.id ? (
                <span className="text-muted font-normal ml-2">{tc.id}</span>
              ) : null}
            </div>
            {body}
          </div>
        );
      })}
    </div>
  );
};

interface ChartCardProps {
  title: string;
  content: unknown;
  toolCalls?: unknown[];
  reasoning?: string;
  isAnswer?: boolean;
  isSystem?: boolean;
  isTool?: boolean;
  forceJson?: boolean;
  pretty?: boolean;
  defaultCollapsed?: boolean;
}

const ChartCard: FC<ChartCardProps> = (props) => {
  const {
    title,
    isAnswer = false,
    isSystem = false,
    isTool = false,
    forceJson = false,
    pretty = true,
    content,
    toolCalls,
    reasoning,
    defaultCollapsed = false,
  } = props;
  const [showMark, setShowMark] = useState(true);
  const showJsonView = forceJson || !pretty;

  const renderPrettyBody = () => {
    const parts = normalizeParts(content);
    if (parts) {
      return (
        <div className="flex flex-col gap-y-[8px]">
          {parts.map((item, index) => {
            if (item.type === 'text') {
              return showMark ? (
                <ReactMarkdown key={`t-${index}`}>{item.text || ''}</ReactMarkdown>
              ) : (
                <div key={`t-${index}`} className="break-word whitespace-pre-wrap">
                  {item.text}
                </div>
              );
            }
            if (item.url && item.mediaType) {
              return (
                <DrawerAttachment
                  key={`m-${index}`}
                  type={item.mediaType}
                  url={item.url}
                />
              );
            }
            return (
              <pre key={`r-${index}`} className="text-[12px] m-0 whitespace-pre-wrap">
                {formatJson(item)}
              </pre>
            );
          })}
        </div>
      );
    }
    if (content == null || content === '') {
      return toolCalls?.length ? null : <span className="text-muted">—</span>;
    }
    if (typeof content === 'string') {
      return showMark ? (
        <ReactMarkdown>{content}</ReactMarkdown>
      ) : (
        <div className="break-word whitespace-pre-wrap">{content}</div>
      );
    }
    return (
      <JSONView
        value={content as object}
        style={{ wordBreak: 'break-word', lineHeight: '24px' }}
        displayDataTypes={false}
        displayObjectSize={false}
      />
    );
  };

  const renderContent = () => {
    if (showJsonView) {
      if (content == null || content === '') {
        if (toolCalls?.length) {
          return (
            <JSONView
              value={{ tool_calls: toolCalls, reasoning_content: reasoning }}
              displayDataTypes={false}
              displayObjectSize={false}
            />
          );
        }
        return <span className="text-muted">—</span>;
      }
      if (typeof content !== 'object') {
        return (
          <pre className="text-[12px] m-0 whitespace-pre-wrap">{String(content)}</pre>
        );
      }
      return (
        <JSONView
          value={content as object}
          style={{ wordBreak: 'break-word', lineHeight: '24px' }}
          displayDataTypes={false}
          displayObjectSize={false}
        />
      );
    }
    return (
      <>
        {reasoning ? (
          <Collapse
            size="small"
            className="mb-2"
            items={[
              {
                key: 'reasoning',
                label: l('monitor.traces.reasoning'),
                children: (
                  <div className="text-[12px] whitespace-pre-wrap text-secondary">
                    {reasoning}
                  </div>
                ),
              },
            ]}
          />
        ) : null}
        {renderPrettyBody()}
        <ToolCallsBlock toolCalls={toolCalls || []} pretty={pretty} />
      </>
    );
  };

  const handleCopy = () => {
    const payload =
      toolCalls?.length || reasoning
        ? { content, tool_calls: toolCalls, reasoning_content: reasoning }
        : content;
    const text =
      typeof payload === 'string' ? payload : JSON.stringify(payload, null, 2);
    copyToClipboard(text || '');
  };

  const card = (
    <div
      className={classNames('border rounded-[8px]', {
        'bg-[#f0fdf5]': isAnswer,
        'bg-[#f5f5f5]': isSystem,
        'bg-sky-50 border-sky-200': isTool,
      })}
    >
      <div className="border-b px-[12px] leading-[32px] flex justify-between">
        <span>{title}</span>
        <div className="flex items-center gap-x-[8px]">
          {!showJsonView && (
            <IconFont
              name="icon-markdown"
              className={classNames('text-[18px] hover:text-primary', {
                'text-primary': !showMark,
              })}
              onClick={() => setShowMark(!showMark)}
            />
          )}
          <CopyOutlined className="hover:text-primary" onClick={handleCopy} />
        </div>
      </div>
      <div className="p-[12px] min-w-0 [word-break:break-word]">{renderContent()}</div>
    </div>
  );

  if (isSystem || defaultCollapsed) {
    return (
      <Collapse
        size="small"
        defaultActiveKey={defaultCollapsed && !isSystem ? ['1'] : []}
        items={[
          {
            key: '1',
            label: isSystem
              ? l('monitor.traces.systemPrompt')
              : title,
            children: card,
          },
        ]}
      />
    );
  }
  return card;
};

type TraceMessage = {
  role?: string;
  content?: unknown;
  name?: string;
  tool_calls?: unknown[];
  reasoning_content?: string;
};

const normalizeAssistantOutput = (
  output: unknown,
): { content: unknown; toolCalls?: unknown[]; reasoning?: string } => {
  if (output == null) return { content: '' };
  if (typeof output === 'string') return { content: output };
  if (typeof output === 'object' && !Array.isArray(output)) {
    const o = output as TraceMessage & Record<string, unknown>;
    if (
      'content' in o ||
      'tool_calls' in o ||
      'reasoning_content' in o ||
      o.role === 'assistant'
    ) {
      return {
        content: o.content ?? '',
        toolCalls: o.tool_calls,
        reasoning: o.reasoning_content,
      };
    }
  }
  return { content: output };
};

const ChatTrace: FC<ChatTraceRenderProps> = (props) => {
  const { dataSource } = props;
  const {
    type,
    name,
    createdAt,
    latency = 0,
    model,
    usage,
    input = [],
    output,
    metadata,
    timeToFirstToken,
  } = dataSource || {};
  const [{ segmented }, setState] = useSetState({
    segmented: 'Pretty ✨',
  });
  const pretty = segmented === 'Pretty ✨';
  const metaObj =
    metadata != null && typeof metadata === 'object' && !Array.isArray(metadata)
      ? (metadata as Record<string, unknown>)
      : {};
  const modelLabel =
    model ||
    (typeof metaObj.model === 'string' ? metaObj.model : '') ||
    '';
  const rawParams =
    metaObj.raw_params && typeof metaObj.raw_params === 'object' && !Array.isArray(metaObj.raw_params)
      ? (metaObj.raw_params as Record<string, unknown>)
      : undefined;
  const toolsSchema = metaObj.tools || rawParams?.tools;
  const assistant = useMemo(() => normalizeAssistantOutput(output), [output]);

  return (
    <div className="grow">
      <div>
        <Tag>{type}</Tag>
        <span className="font-semibold text-[16px]">{name}</span>
      </div>
      <div className="text-[#666] mt-[4px] mb-[10px]">
        {formatDisplayTime(createdAt)}
      </div>
      <div className="flex flex-wrap gap-[10px] mb-[20px] text-[12px]">
        <span className="rounded-full border font-semibold px-[8px] py-[2px]">
          {l('monitor.traces.table.latency')}: {Number(latency || 0).toFixed(2)}
          s
        </span>
        {timeToFirstToken != null && (
          <span className="rounded-full border font-semibold px-[8px] py-[2px]">
            TTFT: {(Number(timeToFirstToken) * 1000).toFixed(0)}ms
          </span>
        )}
        <span className="rounded-full border font-semibold px-[8px] py-[2px]">
          {usage?.input} prompt → {usage?.output} completion (∑ {usage?.total})
        </span>
        {modelLabel ? (
          <span className="rounded-full border font-semibold px-[8px] py-[2px]">
            {modelLabel}
          </span>
        ) : null}
      </div>

      <Segmented
        value={segmented}
        onChange={(value) => setState({ segmented: String(value) })}
        options={['Pretty ✨', 'JSON']}
      />
      <div className="mt-[20px] flex flex-col gap-y-[8px] border rounded-[8px] p-[12px]">
        {Array.isArray(input) ? (
          <div className="flex flex-col gap-y-[8px]">
            {input.map((item: TraceMessage, index: number) => {
              const role = item?.role || 'message';
              const isAssistant = role === 'assistant';
              const nested = isAssistant
                ? normalizeAssistantOutput(item)
                : { content: item?.content, toolCalls: item?.tool_calls };
              return (
                <ChartCard
                  key={`${role}-${index}`}
                  title={
                    role === 'tool'
                      ? `${l('monitor.traces.toolResult')}${
                          item?.name ? `: ${item.name}` : ''
                        }`
                      : role
                  }
                  isAnswer={isAssistant}
                  isSystem={role === 'system'}
                  isTool={role === 'tool'}
                  pretty={pretty}
                  forceJson={!pretty}
                  content={nested.content}
                  toolCalls={nested.toolCalls}
                  reasoning={isAssistant ? item?.reasoning_content : undefined}
                />
              );
            })}
          </div>
        ) : (
          <ChartCard
            title="Input"
            content={input}
            forceJson
            pretty={pretty}
          />
        )}
        <ChartCard
          title={
            pretty && (isString(output) || assistant.toolCalls?.length)
              ? 'assistant'
              : 'Output'
          }
          isAnswer
          pretty={pretty}
          forceJson={!pretty && !isString(output) && !assistant.toolCalls}
          content={assistant.content}
          toolCalls={assistant.toolCalls}
          reasoning={assistant.reasoning}
        />
        {Array.isArray(toolsSchema) && toolsSchema.length > 0 ? (
          <Collapse
            size="small"
            items={[
              {
                key: 'tools',
                label: l('monitor.traces.toolDefs'),
                children: (
                  <JSONView
                    value={toolsSchema}
                    displayDataTypes={false}
                    displayObjectSize={false}
                  />
                ),
              },
            ]}
          />
        ) : null}
        <ChartCard title="Metadata" forceJson content={metaObj} pretty={pretty} />
      </div>
    </div>
  );
};
export default ChatTrace;
