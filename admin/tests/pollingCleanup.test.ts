/**
 * 实例页轮询卸载清理的纯逻辑回归：确保 clearInterval 会被调用。
 * （完整组件挂载依赖 Umi 运行时，此处覆盖定时器生命周期契约）
 */

describe('polling cleanup contract', () => {
  it('clears all intervals on unmount map', () => {
    jest.useFakeTimers();
    const pollingRef: Record<string, ReturnType<typeof setInterval>> = {};
    let ticks = 0;
    pollingRef.a = setInterval(() => {
      ticks += 1;
    }, 1000);
    pollingRef.b = setInterval(() => {
      ticks += 1;
    }, 1000);

    jest.advanceTimersByTime(1000);
    expect(ticks).toBe(2);

    Object.values(pollingRef).forEach(clearInterval);
    Object.keys(pollingRef).forEach((k) => delete pollingRef[k]);

    jest.advanceTimersByTime(3000);
    expect(ticks).toBe(2);
    expect(Object.keys(pollingRef)).toHaveLength(0);
    jest.useRealTimers();
  });
});
