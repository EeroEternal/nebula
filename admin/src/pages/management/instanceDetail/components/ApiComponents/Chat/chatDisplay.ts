import type { ChatStreamResult } from '@/types/Public/data';

export const THINK_START_TAG = '<think>';
export const THINK_END_TAG = '</think>';

export type ChatRole = 'user' | 'assistant' | 'system';

export interface ChatUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface ChatDisplayItem {
  content: string;
  thinkingContent: string;
  thinkingCompleted: boolean;
  usage?: ChatUsage;
  /** True while an unclosed `<think>` is being streamed in content. */
  inThinkBlock?: boolean;
}

const THINK_BLOCK_RE = /<think>([\s\S]*?)<\/think>/gi;

/** Split `<think>...</think>` from model content. Unclosed `<think>` stays in thinking. */
export function splitThinkTags(text: string): { thinking: string; content: string } {
  if (!text) {
    return { thinking: '', content: '' };
  }
  let thinking = '';
  const withoutClosed = text.replace(THINK_BLOCK_RE, (_m, inner: string) => {
    thinking += inner;
    return '';
  });
  const openIdx = withoutClosed.toLowerCase().indexOf(THINK_START_TAG);
  if (openIdx >= 0) {
    thinking += withoutClosed.slice(openIdx + THINK_START_TAG.length);
    return { thinking, content: withoutClosed.slice(0, openIdx) };
  }
  return { thinking, content: withoutClosed };
}

type ReasoningPayload = {
  reasoning_content?: string | null;
  reasoning?: string | null;
};

/** Prefer non-empty ``reasoning_content``; else engine alias ``reasoning``. ``null`` is empty. */
export function pickReasoningText(payload?: ReasoningPayload | null): string {
  if (!payload) {
    return '';
  }
  const fromContent = payload.reasoning_content;
  if (typeof fromContent === 'string' && fromContent) {
    return fromContent;
  }
  const fromAlias = payload.reasoning;
  if (typeof fromAlias === 'string' && fromAlias) {
    return fromAlias;
  }
  return '';
}

export function applyChatStreamChunk(
  prev: ChatDisplayItem,
  chunk: ChatStreamResult,
  isStream: boolean,
): ChatDisplayItem {
  const choice = chunk?.choices?.[0];
  const streamPayload = choice?.delta as ReasoningPayload | undefined;
  const messagePayload = choice?.message as ReasoningPayload | undefined;
  const rawContent = (isStream ? choice?.delta?.content : choice?.message?.content) || '';
  const reasoningDelta = isStream
    ? pickReasoningText(streamPayload) || pickReasoningText(messagePayload)
    : pickReasoningText(messagePayload) || pickReasoningText(streamPayload);
  const closedByNull =
    !reasoningDelta &&
    (isStream
      ? streamPayload?.reasoning_content === null && !streamPayload?.reasoning
      : messagePayload?.reasoning_content === null && !messagePayload?.reasoning);

  const nextThinking = `${prev.thinkingContent || ''}${reasoningDelta}`;
  const nextContent = `${prev.content || ''}${rawContent}`;

  // Only flip to "thinking in progress" when the engine sent reasoning text.
  let thinkingCompleted = prev.thinkingCompleted;
  if (closedByNull) {
    thinkingCompleted = true;
  } else if (reasoningDelta.trim()) {
    thinkingCompleted = false;
  } else if (!nextThinking.trim()) {
    thinkingCompleted = true;
  } else if (nextContent) {
    thinkingCompleted = true;
  }

  return {
    content: nextContent,
    thinkingContent: nextThinking,
    thinkingCompleted,
    usage: chunk?.usage ?? prev.usage,
    inThinkBlock: false,
  };
}

/** Keep a single system prompt at the front; later system sends replace it. */
export function upsertSystemMessage<T extends { role: ChatRole }>(
  list: T[],
  systemItem: T,
): T[] {
  const rest = list.filter((item) => item.role !== 'system');
  return [systemItem, ...rest];
}

export function orderMessagesSystemFirst<T extends { role: string }>(messages: T[]): T[] {
  const systems = messages.filter((item) => item.role === 'system');
  const rest = messages.filter((item) => item.role !== 'system');
  return [...systems, ...rest];
}
