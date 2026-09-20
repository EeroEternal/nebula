export type LogRefreshTimer = {
  start: (fn: () => void, ms: number) => void;
  stop: () => void;
  isActive: () => boolean;
};

/** One interval handle; start replaces any previous timer. */
export function createLogRefreshTimer(): LogRefreshTimer {
  let id: ReturnType<typeof setInterval> | null = null;
  return {
    start(fn: () => void, ms: number) {
      if (id != null) {
        clearInterval(id);
      }
      id = setInterval(fn, ms);
    },
    stop() {
      if (id != null) {
        clearInterval(id);
        id = null;
      }
    },
    isActive() {
      return id != null;
    },
  };
}
