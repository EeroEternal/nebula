jest.mock('@/utils/intl', () => {
  const messages: Record<string, string> = {
    'error.status.401': '登录已失效',
    'error.status.403': '没有权限',
    'error.status.404': '资源不存在',
    'error.status.413': '请求体过大',
    'error.status.500': '服务器错误',
    'error.network': '网络异常',
    'error.retry.hint': '可稍后重试',
    'error.proxy.disconnected': '后端未连接或隧道已断开，请检查服务后重试。',
    'error.engine.notFound': '镜像不存在或地址错误',
    'error.engine.migrate': '引擎迁移包未传到目标节点（跨机需传输 tar，不是镜像不存在）',
  };
  const l = (id: string) => messages[id] || id;
  return { l, lGet: l };
});

import {
  stringifyDetail,
  formatApiError,
  extractRequestId,
  buildErrorDescription,
  humanizeEngineImageError,
  readResponseDetail,
  readThrownMessage,
  readRequestFailure,
  thrownName,
} from '@/utils/formatApiError';

describe('stringifyDetail', () => {
  it('returns plain string', () => {
    expect(stringifyDetail('hello')).toBe('hello');
  });

  it('does not dump FastAPI validation array as JSON', () => {
    const detail = [
      { loc: ['body', 'name'], msg: 'field required', type: 'value_error.missing' },
      { loc: ['body', 'age'], msg: 'value is not a valid integer', type: 'type_error.integer' },
    ];
    const text = stringifyDetail(detail);
    expect(text).toContain('name');
    expect(text).toContain('field required');
    expect(text).not.toMatch(/^\s*\[/);
  });

  it('rewrites proxy disconnect message', () => {
    expect(stringifyDetail('Error occurred while trying to proxy to ...')).toMatch(/后端未连接/);
  });
});

describe('readResponseDetail', () => {
  it('reads data.detail then nested data.data.detail', () => {
    expect(readResponseDetail({ data: { detail: 'top' } })).toBe('top');
    expect(readResponseDetail({ data: { data: { detail: 'nested' } } })).toBe('nested');
  });

  it('falls back to message fields', () => {
    expect(readResponseDetail({ data: { message: 'msg' } })).toBe('msg');
    expect(readResponseDetail({ message: 'plain' })).toBe('plain');
  });
});

describe('readThrownMessage', () => {
  it('prefers userError.description', () => {
    expect(
      readThrownMessage({
        userError: { description: '  readable  ' },
        errmsg: 'raw',
        message: 'err',
      }),
    ).toBe('readable');
  });

  it('falls back to errmsg then message', () => {
    expect(readThrownMessage({ errmsg: 'e1', message: 'e2' })).toBe('e1');
    expect(readThrownMessage({ message: 'e2' })).toBe('e2');
  });
});

describe('readRequestFailure', () => {
  it('prefers envelope detail over message', () => {
    expect(readRequestFailure({ data: { detail: 'd' }, message: 'm' })).toBe('d');
    expect(readRequestFailure({ message: 'm' })).toBe('m');
  });
});

describe('thrownName', () => {
  it('reads Error.name and object name', () => {
    expect(thrownName(new Error('x'))).toBe('Error');
    expect(thrownName({ name: 'AbortError' })).toBe('AbortError');
    expect(thrownName('nope')).toBe('');
  });
});

describe('formatApiError', () => {
  it('formats 401 for login recovery', () => {
    const err = formatApiError(401, { detail: 'token expired', request_id: 'req-1' });
    expect(err.title).toBe('登录已失效');
    expect(err.description).toContain('token expired');
    expect(err.requestId).toBe('req-1');
    expect(err.retryable).toBe(false);
  });

  it('formats 403 / 404', () => {
    expect(formatApiError(403, { detail: 'nope' }).title).toBe('没有权限');
    expect(formatApiError(404, {}).title).toBe('资源不存在');
  });

  it('formats 413 body too large', () => {
    expect(formatApiError(413, {}).title).toBe('请求体过大');
  });

  it('marks 5xx as retryable', () => {
    const err = formatApiError(500, { detail: 'boom' });
    expect(err.retryable).toBe(true);
    expect(err.title).toBe('服务器错误');
  });

  it('handles network status 0', () => {
    const err = formatApiError(0, {});
    expect(err.title).toBe('网络异常');
    expect(err.retryable).toBe(true);
  });

  it('never puts raw object JSON as primary description', () => {
    const err = formatApiError(400, { detail: { foo: 'bar', baz: 1 } });
    expect(err.description).not.toMatch(/^\{/);
  });
});

describe('humanizeEngineImageError', () => {
  it('does not map exported tar miss to image-not-found', () => {
    expect(
      humanizeEngineImageError(
        'exported tar not found at shared path /data/powerllm/engines/tmp/migrate-abc.tar',
      ),
    ).toContain('跨机需传输 tar');
    expect(humanizeEngineImageError('manifest unknown')).toBe(
      '镜像不存在或地址错误',
    );
  });

  it('does not map bare FastAPI Not Found to image-not-found', () => {
    expect(humanizeEngineImageError('Not Found')).toBe('Not Found');
    const err = formatApiError(404, { detail: 'Not Found' });
    expect(err.title).toBe('资源不存在');
    expect(err.description).not.toContain('镜像不存在');
  });

  it('still maps docker image miss', () => {
    expect(humanizeEngineImageError('Error: No such image: vllm/vllm-openai:latest')).toBe(
      '镜像不存在或地址错误',
    );
  });
});

describe('extractRequestId / buildErrorDescription', () => {
  it('extracts request_id', () => {
    expect(extractRequestId({ request_id: 'abc' })).toBe('abc');
  });

  it('appends request id and retry hint', () => {
    const desc = buildErrorDescription({
      title: 't',
      description: 'reason',
      requestId: 'r1',
      retryable: true,
      status: 500,
    });
    expect(desc).toContain('reason');
    expect(desc).toContain('Request ID: r1');
    expect(desc).toContain('可稍后重试');
  });
});
