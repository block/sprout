import * as React from "react";

import type { DesktopNotificationPermissionState } from "@/features/notifications/hooks";
import type { NotificationSettings } from "@/features/notifications/hooks";
import type { SettingsSection } from "@/features/settings/ui/SettingsPanels";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const SettingsView = React.lazy(async () => {
  const module = await import("@/features/settings/ui/SettingsView");
  return { default: module.SettingsView };
});

type SettingsScreenProps = {
  currentPubkey?: string;
  fallbackDisplayName?: string;
  isUpdatingDesktopNotifications: boolean;
  notificationErrorMessage: string | null;
  notificationPermission: DesktopNotificationPermissionState;
  notificationSettings: NotificationSettings;
  onClose: () => void;
  onSectionChange: (section: SettingsSection) => void;
  onSetDesktopNotificationsEnabled: (enabled: boolean) => Promise<boolean>;
  onSetHomeBadgeEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
  section: SettingsSection;
};

export function SettingsScreen({
  currentPubkey,
  fallbackDisplayName,
  isUpdatingDesktopNotifications,
  notificationErrorMessage,
  notificationPermission,
  notificationSettings,
  onClose,
  onSectionChange,
  onSetDesktopNotificationsEnabled,
  onSetHomeBadgeEnabled,
  onSetMentionNotificationsEnabled,
  onSetNeedsActionNotificationsEnabled,
  section,
}: SettingsScreenProps) {
  return (
    <React.Suspense
      fallback={<ViewLoadingFallback label="Loading settings..." />}
    >
      <SettingsView
        currentPubkey={currentPubkey}
        fallbackDisplayName={fallbackDisplayName}
        isUpdatingDesktopNotifications={isUpdatingDesktopNotifications}
        notificationErrorMessage={notificationErrorMessage}
        notificationPermission={notificationPermission}
        notificationSettings={notificationSettings}
        onClose={onClose}
        onSectionChange={onSectionChange}
        onSetDesktopNotificationsEnabled={onSetDesktopNotificationsEnabled}
        onSetHomeBadgeEnabled={onSetHomeBadgeEnabled}
        onSetMentionNotificationsEnabled={onSetMentionNotificationsEnabled}
        onSetNeedsActionNotificationsEnabled={
          onSetNeedsActionNotificationsEnabled
        }
        section={section}
      />
    </React.Suspense>
  );
}
