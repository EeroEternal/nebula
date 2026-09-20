import { resolveIntlArgs } from '../src/utils/intlArgs';

describe('resolveIntlArgs', () => {
  it('treats a 2nd-arg object as values (l(id, { n }))', () => {
    expect(resolveIntlArgs({ n: 2 })).toEqual({ values: { n: 2 } });
  });

  it('keeps string defaultMessage and 3rd-arg values', () => {
    expect(resolveIntlArgs('0', { count: 3 })).toEqual({
      defaultMessage: '0',
      values: { count: 3 },
    });
  });

  it('keeps undefined defaultMessage + values', () => {
    expect(resolveIntlArgs(undefined, { time: '1s' })).toEqual({
      defaultMessage: undefined,
      values: { time: '1s' },
    });
  });
});
