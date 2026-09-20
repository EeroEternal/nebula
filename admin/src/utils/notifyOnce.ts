import { notification } from 'antd';

type NotifyPayload = {
  message: string;
  description?: string;
  key?: string;
  duration?: number;
};

/** Same title+description only surfaces once per page load. */
const shownKeys = new Set<string>();

function notifyKey(data: NotifyPayload): string {
  if (data.key != null && data.key !== '') {
    return String(data.key);
  }
  return `${data.message ?? ''}|${data.description ?? ''}`;
}

export function notifyErrorOnce(data: NotifyPayload): void {
  const key = notifyKey(data);
  if (shownKeys.has(key)) {
    return;
  }
  shownKeys.add(key);
  notification.error({
    message: data.message,
    description: data.description,
    key,
    duration: data.duration,
  });
}
