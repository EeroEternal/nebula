type EventType = 'showModal' | 'redirect' | 'showMessage' | 'notification';
type CallBack<T = unknown> = (data: T)=> void;
export type EventMap = {
  showModal: {  visible: boolean; modalName: string }; // 弹窗类型
  redirect: { path: string, message?: string }; // 跳转类型
  showMessage: {
    message: string;
    /** 默认 warning；API 业务错误用 error */
    type?: 'success' | 'error' | 'warning' | 'info';
  };
  notification: { message: string; description?: string }; // 服务异常通知（右侧）
}

export type ShowModalHandler = (data: EventMap['showModal']) => void;
export type RedirectHandler = (data: EventMap['redirect']) => void;
export type ShowMessageHandler = (data: EventMap['showMessage']) => void;
export type ShowNotificationHandler = (data: EventMap['notification']) => void;

class EventBus {
  private events: { [K in EventType]?: CallBack<EventMap[K]>[] } = {};
  
  // 订阅事件
  on<T extends EventType>(event: T, callback: CallBack<EventMap[T]>) {
    if (!this.events[event]) {
      this.events[event] = [];
    }
    this.events[event]!.push(callback);
  }

  // 取消订阅
  off<T extends EventType>(event: T, callback: CallBack<EventMap[T]>) {
    const list = this.events[event];
    if (list) {
      this.events[event] = list.filter((cb) => cb !== callback) as (typeof this.events)[T];
    }
  }

  // 发布事件
  emit<T extends EventType>(event: T, data: EventMap[T]) {
    if (this.events[event]) {
      this.events[event]!.forEach((callback) => callback(data));
    }
  }
}
const eventBus = new EventBus();
export default eventBus;
