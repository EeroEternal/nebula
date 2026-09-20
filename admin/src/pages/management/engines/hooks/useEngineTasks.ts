import { useEffect, useRef, useState } from 'react';
import { App } from 'antd';

import {
  cancelEnginePull,
  clearEnginePullTasks,
  deleteEnginePull,
  EnginePullProgress,
  getEnginePullProgress,
  listEnginePullTasks,
  retryEnginePull,
} from '@/services/engineImages';
import {
  humanizeEngineImageError,
  stringifyDetail,
} from '@/utils/formatApiError';
import { lGet } from '@/utils/intl';

const MAX_TASKS = 30;

export function useEngineTasks(onSettled?: () => void) {
  const { message, modal } = App.useApp();
  const [tasks, setTasks] = useState<EnginePullProgress[]>([]);
  const [hydrated, setHydrated] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const toastedRef = useRef<Set<string>>(new Set());
  const onSettledRef = useRef(onSettled);
  onSettledRef.current = onSettled;
  const tasksRef = useRef(tasks);
  tasksRef.current = tasks;

  const activeTasks = tasks.filter(
    (t) => t.status === 'pending' || t.status === 'running',
  );

  const enqueue = (progress: EnginePullProgress) => {
    setTasks((prev) =>
      [progress, ...prev.filter((t) => t.task_id !== progress.task_id)].slice(
        0,
        MAX_TASKS,
      ),
    );
  };

  const handleCancel = async (taskId: string) => {
    try {
      const res = await cancelEnginePull(taskId);
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.cancelFailed')),
        );
        return;
      }
      const progress = res.data as EnginePullProgress;
      setTasks((prev) =>
        prev.map((t) => (t.task_id === taskId ? progress : t)),
      );
    } catch (e) {
      message.error(
        stringifyDetail((e as { data?: { detail?: unknown } })?.data?.detail) ||
          String(lGet('models.engines.cancelFailed')),
      );
    }
  };

  const handleRetry = async (taskId: string) => {
    try {
      const res = await retryEnginePull(taskId);
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.retryFailed', 'Retry failed')),
        );
        return;
      }
      const progress = res.data as EnginePullProgress;
      toastedRef.current.delete(taskId);
      enqueue(progress);
      message.success(
        progress.kind === 'migrate'
          ? String(lGet('models.engines.continueOk'))
          : String(lGet('models.engines.retryOk', 'Pull restarted')),
      );
    } catch (e) {
      message.error(
        stringifyDetail((e as { data?: { detail?: unknown } })?.data?.detail) ||
          String(lGet('models.engines.retryFailed', 'Retry failed')),
      );
    }
  };

  const hydrateFromList = async () => {
    const res = await listEnginePullTasks();
    if (res?.success && Array.isArray(res.data?.tasks)) {
      setTasks((res.data.tasks as EnginePullProgress[]).slice(0, MAX_TASKS));
    }
  };

  const handleClear = () => {
    if (!tasksRef.current.length) return;
    modal.confirm({
      title: lGet('models.engines.clearTasks'),
      content: lGet('models.engines.clearTasksConfirm'),
      okText: lGet('models.engines.clearTasks'),
      okButtonProps: { danger: true },
      cancelText: lGet('models.engines.cancel'),
      onOk: async () => {
        try {
          const res = await clearEnginePullTasks();
          if (!res?.success) {
            message.error(
              stringifyDetail(res?.data?.detail) ||
                String(lGet('models.engines.clearTasksFailed')),
            );
            return;
          }
          setTasks([]);
          toastedRef.current.clear();
          try {
            await hydrateFromList();
          } catch {
            /* keep empty optimistic list */
          }
          const failed = Number(res.data?.failed || 0);
          if (failed > 0) {
            message.warning(
              String(
                lGet('models.engines.clearTasksPartial', undefined, {
                  deleted: Number(res.data?.deleted || 0),
                  failed,
                }),
              ),
            );
          } else {
            message.success(String(lGet('models.engines.clearTasksOk')));
          }
        } catch (e) {
          message.error(
            stringifyDetail(
              (e as { data?: { detail?: unknown } })?.data?.detail,
            ) || String(lGet('models.engines.clearTasksFailed')),
          );
        }
      },
    });
  };

  const handleDelete = async (taskId: string) => {
    try {
      const res = await deleteEnginePull(taskId);
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.deleteTaskFailed')),
        );
        return;
      }
      setTasks((prev) => prev.filter((t) => t.task_id !== taskId));
      toastedRef.current.delete(taskId);
      message.success(String(lGet('models.engines.deleteTaskOk')));
    } catch (e) {
      message.error(
        stringifyDetail((e as { data?: { detail?: unknown } })?.data?.detail) ||
          String(lGet('models.engines.deleteTaskFailed')),
      );
    }
  };

  // Hydrate from backend list so refresh keeps history.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await listEnginePullTasks();
        if (cancelled) return;
        if (res?.success && Array.isArray(res.data?.tasks)) {
          const list = (res.data.tasks as EnginePullProgress[]).slice(
            0,
            MAX_TASKS,
          );
          setTasks(list);
          for (const t of list) {
            if (
              t.status === 'succeeded' ||
              t.status === 'failed' ||
              t.status === 'cancelled'
            ) {
              toastedRef.current.add(t.task_id);
            }
          }
        }
      } catch {
        /* keep empty */
      } finally {
        if (!cancelled) setHydrated(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!hydrated) return undefined;
    if (activeTasks.length === 0) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return undefined;
    }
    if (pollRef.current) {
      return undefined;
    }

    let locked = false;
    pollRef.current = setInterval(async () => {
      if (locked) return;
      locked = true;
      try {
        const current = tasksRef.current.filter(
          (t) => t.status === 'pending' || t.status === 'running',
        );
        if (!current.length) return;
        const updates = await Promise.all(
          current.map(async (t) => {
            try {
              const res = await getEnginePullProgress(t.task_id);
              if (res?.success && res.data) {
                return res.data as EnginePullProgress;
              }
            } catch {
              /* keep previous */
            }
            return t;
          }),
        );
        let settled = false;
        setTasks((prev) => {
          const map = new Map(updates.map((u) => [u.task_id, u]));
          return prev.map((t) => map.get(t.task_id) || t);
        });
        for (const u of updates) {
          if (
            (u.status === 'succeeded' ||
              u.status === 'failed' ||
              u.status === 'cancelled') &&
            !toastedRef.current.has(u.task_id)
          ) {
            toastedRef.current.add(u.task_id);
            settled = true;
            if (u.status === 'succeeded') {
              message.success(
                u.kind === 'migrate'
                  ? lGet('models.engines.taskMigrateDone')
                  : u.kind === 'delete'
                    ? lGet('models.engines.taskDeleteDone')
                    : lGet('models.engines.taskPullDone'),
              );
            } else if (u.status === 'failed') {
              message.error(
                humanizeEngineImageError(u.error || u.message) ||
                  lGet('models.engines.pullFailed'),
              );
            }
          }
        }
        if (settled) {
          onSettledRef.current?.();
        }
      } finally {
        locked = false;
      }
    }, 1500);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [activeTasks.length, hydrated, message]);

  return {
    tasks,
    activeTasks,
    enqueue,
    handleCancel,
    handleRetry,
    handleDelete,
    handleClear,
  };
}
