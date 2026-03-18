import { isTauri } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export type DesktopNotificationPermissionState =
  | NotificationPermission
  | "unsupported";

type DesktopNotificationPayload = {
  body?: string;
  title: string;
};

function hasNotificationApi() {
  return typeof window !== "undefined" && "Notification" in window;
}

export async function getDesktopNotificationPermissionState(): Promise<DesktopNotificationPermissionState> {
  if (!hasNotificationApi()) {
    return "unsupported";
  }

  if (window.Notification.permission !== "default") {
    return window.Notification.permission;
  }

  if (!isTauri()) {
    return "default";
  }

  try {
    return (await isPermissionGranted()) ? "granted" : "default";
  } catch {
    return "default";
  }
}

export async function requestDesktopNotificationAccess(): Promise<DesktopNotificationPermissionState> {
  if (!hasNotificationApi()) {
    return "unsupported";
  }

  return requestPermission();
}

export async function sendDesktopNotification(
  payload: DesktopNotificationPayload,
): Promise<boolean> {
  if ((await getDesktopNotificationPermissionState()) !== "granted") {
    return false;
  }

  sendNotification({
    body: payload.body,
    title: payload.title,
  });
  return true;
}
