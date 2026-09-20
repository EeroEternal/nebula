import {
  getUsageLevel,
  getUsageStrokeColor,
  getUsageProgressClassName,
} from '../src/utils/usageColor';

describe('usageColor (design.md GPU thresholds)', () => {
  it('classifies normal / warning / critical bands', () => {
    expect(getUsageLevel(0)).toBe('normal');
    expect(getUsageLevel(79.9)).toBe('normal');
    expect(getUsageLevel(80)).toBe('warning');
    expect(getUsageLevel(95)).toBe('warning');
    expect(getUsageLevel(95.1)).toBe('critical');
    expect(getUsageLevel(100)).toBe('critical');
  });

  it('maps stroke colors to design tokens', () => {
    expect(getUsageStrokeColor(50)).toBe('var(--c-primary)');
    expect(getUsageStrokeColor(85)).toBe('var(--c-warning)');
    expect(getUsageStrokeColor(96)).toBe('var(--c-error)');
  });

  it('adds pulse class only for critical', () => {
    expect(getUsageProgressClassName(90)).toBe('');
    expect(getUsageProgressClassName(96)).toBe('progress-critical');
  });
});
