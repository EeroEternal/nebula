import {
  applyChatStreamChunk,
  orderMessagesSystemFirst,
  splitThinkTags,
  upsertSystemMessage,
} from '@/pages/management/instanceDetail/components/ApiComponents/Chat/chatDisplay';
import type { ChatStreamResult } from '@/types/Public/data';

describe('splitThinkTags', () => {
  it('keeps plain text in content', () => {
    expect(splitThinkTags('你好世界')).toEqual({ thinking: '', content: '你好世界' });
  });

  it('splits a closed think block', () => {
    expect(splitThinkTags('<think>推理</think>答案')).toEqual({
      thinking: '推理',
      content: '答案',
    });
  });

  it('treats unclosed think as thinking', () => {
    expect(splitThinkTags('前缀<think>还在想')).toEqual({
      thinking: '还在想',
      content: '前缀',
    });
  });
});

describe('applyChatStreamChunk', () => {
  const empty = {
    content: '',
    thinkingContent: '',
    thinkingCompleted: true,
  };

  it('does not open thinking for plain deltas', () => {
    const next = applyChatStreamChunk(
      empty,
      {
        choices: [{ delta: { content: '你好' } }],
      } as ChatStreamResult,
      true,
    );
    expect(next.content).toBe('你好');
    expect(next.thinkingContent).toBe('');
    expect(next.thinkingCompleted).toBe(true);
  });

  it('opens thinking when engine sends delta.reasoning and reasoning_content is null', () => {
    const next = applyChatStreamChunk(
      empty,
      {
        choices: [
          {
            delta: {
              reasoning: '输出格式。',
              reasoning_content: null,
              content: '',
            },
          },
        ],
      } as ChatStreamResult,
      true,
    );
    expect(next.thinkingContent).toBe('输出格式。');
    expect(next.thinkingCompleted).toBe(false);
    expect(next.content).toBe('');
  });

  it('opens thinking only when reasoning_content has text', () => {
    const next = applyChatStreamChunk(
      empty,
      {
        choices: [{ delta: { reasoning_content: '想一下', content: '' } }],
      } as ChatStreamResult,
      true,
    );
    expect(next.thinkingContent).toBe('想一下');
    expect(next.thinkingCompleted).toBe(false);
  });

  it('keeps previous usage when later chunk has none', () => {
    const withUsage = applyChatStreamChunk(
      empty,
      {
        choices: [{ delta: { content: '答' } }],
        usage: { prompt_tokens: 3, completion_tokens: 1, total_tokens: 4 },
      } as ChatStreamResult,
      true,
    );
    const next = applyChatStreamChunk(
      withUsage,
      { choices: [{ delta: { content: '案' } }] } as ChatStreamResult,
      true,
    );
    expect(next.content).toBe('答案');
    expect(next.usage).toEqual({ prompt_tokens: 3, completion_tokens: 1, total_tokens: 4 });
  });

  it('keeps engine content including think tags', () => {
    let state = applyChatStreamChunk(
      empty,
      { choices: [{ delta: { content: '<think>' } }] } as ChatStreamResult,
      true,
    );
    state = applyChatStreamChunk(
      state,
      { choices: [{ delta: { content: '推理中' } }] } as ChatStreamResult,
      true,
    );
    state = applyChatStreamChunk(
      state,
      { choices: [{ delta: { content: '</think>答案' } }] } as ChatStreamResult,
      true,
    );
    expect(state.thinkingContent).toBe('');
    expect(state.content).toBe('<think>推理中</think>答案');
    expect(state.inThinkBlock).toBe(false);
    expect(state.thinkingCompleted).toBe(true);
  });

  it('closes thinking when reasoning_content is null', () => {
    const thinking = applyChatStreamChunk(
      empty,
      {
        choices: [{ delta: { reasoning_content: '推理' } }],
      } as ChatStreamResult,
      true,
    );
    const done = applyChatStreamChunk(
      thinking,
      {
        choices: [{ delta: { reasoning_content: null, content: '结论' } }],
      } as ChatStreamResult,
      true,
    );
    expect(done.thinkingCompleted).toBe(true);
    expect(done.content).toBe('结论');
  });
});

describe('system message helpers', () => {
  it('upserts a single system message at the front', () => {
    const list = [
      { role: 'user' as const, content: 'hi' },
      { role: 'system' as const, content: 'old' },
    ];
    const next = upsertSystemMessage(list, { role: 'system', content: 'new' });
    expect(next).toEqual([
      { role: 'system', content: 'new' },
      { role: 'user', content: 'hi' },
    ]);
  });

  it('orders system messages first', () => {
    const ordered = orderMessagesSystemFirst([
      { role: 'user' },
      { role: 'system' },
      { role: 'assistant' },
    ]);
    expect(ordered.map((item) => item.role)).toEqual(['system', 'user', 'assistant']);
  });
});
