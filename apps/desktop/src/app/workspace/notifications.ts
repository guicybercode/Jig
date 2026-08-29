import {
  EMPTY_NOTIFICATIONS,
  type Notification,
  type NotificationKind,
} from "./types";

/** Builds a notification owned by the workspace store. */
export function createNotification(
  id: string,
  kind: NotificationKind,
  message: string,
): Notification {
  return { id, kind, message };
}

/** Appends a notification, keeping a short tail so the shell cannot grow without bound. */
export function appendNotification(
  notifications: readonly Notification[],
  notification: Notification,
): Notification[] {
  return [...notifications, notification].slice(-8);
}

/** Removes one notification by id. */
export function removeNotification(
  notifications: readonly Notification[],
  id: string,
): readonly Notification[] {
  const next = notifications.filter((item) => item.id !== id);
  return next.length === 0 ? EMPTY_NOTIFICATIONS : next;
}
