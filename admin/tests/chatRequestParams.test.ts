import {
  buildChatRequestParams,
  resolveChatMaxTokens,
} from '@/pages/management/instanceDetail/components/ApiComponents/Chat/chatRequestParams';

describe('buildChatRequestParams', () => {
  it('maps top_k slider to top_p and defaults thinking on', () => {
    expect(
      buildChatRequestParams(
        { temperature: 0.7, max_tokens: 256, top_k: 0.9 },
        { showThinking: true, maxTokensCap: 8192 },
      ),
    ).toEqual({
      temperature: 0.7,
      max_tokens: 256,
      top_p: 0.9,
      enable_thinking: true,
    });
  });

  it('omits thinking when the model has no reasoning ability', () => {
    expect(
      buildChatRequestParams(
        { max_tokens: 128, enable_thinking: true },
        { showThinking: false, maxTokensCap: 4096 },
      ),
    ).toEqual({ max_tokens: 128 });
  });

  it('prefers instance max_model_len over catalog context_length', () => {
    expect(
      resolveChatMaxTokens({
        context_length: 262144,
        kwargs: { max_model_len: 16000 },
      }),
    ).toBe(16000);
  });

  it('falls back to context_length then 8192', () => {
    expect(resolveChatMaxTokens({ context_length: 4096, kwargs: {} })).toBe(4096);
    expect(resolveChatMaxTokens({})).toBe(8192);
  });

  it('keeps enable_thinking false when the user turns it off', () => {
    expect(
      buildChatRequestParams(
        { enable_thinking: false },
        { showThinking: true, maxTokensCap: 8192 },
      ),
    ).toEqual({
      max_tokens: 512,
      enable_thinking: false,
    });
  });
});
