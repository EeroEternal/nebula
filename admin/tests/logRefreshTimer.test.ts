import { createLogRefreshTimer } from '@/utils/logRefreshTimer';

describe('createLogRefreshTimer', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('keeps a single interval across restart and stops on unmount', () => {
    const timer = createLogRefreshTimer();
    let ticks = 0;
    timer.start(() => {
      ticks += 1;
    }, 1000);
    timer.start(() => {
      ticks += 1;
    }, 1000);
    jest.advanceTimersByTime(1000);
    expect(ticks).toBe(1);
    expect(timer.isActive()).toBe(true);

    timer.stop();
    jest.advanceTimersByTime(3000);
    expect(ticks).toBe(1);
    expect(timer.isActive()).toBe(false);
  });
});
