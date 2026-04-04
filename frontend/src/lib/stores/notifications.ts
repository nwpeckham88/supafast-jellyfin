import { writable } from 'svelte/store';

export type NotificationLevel = 'info' | 'success' | 'error';

export interface NotificationItem {
  id: string;
  level: NotificationLevel;
  message: string;
}

function createNotificationsStore() {
  const { subscribe, update } = writable<NotificationItem[]>([]);

  function push(level: NotificationLevel, message: string) {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
    update((items) => [...items, { id, level, message }]);

    setTimeout(() => {
      update((items) => items.filter((item) => item.id !== id));
    }, 5000);
  }

  return {
    subscribe,
    info(message: string) {
      push('info', message);
    },
    success(message: string) {
      push('success', message);
    },
    error(message: string) {
      push('error', message);
    },
    dismiss(id: string) {
      update((items) => items.filter((item) => item.id !== id));
    }
  };
}

export const notificationsStore = createNotificationsStore();
