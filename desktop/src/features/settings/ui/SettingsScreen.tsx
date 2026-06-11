import type { DesktopNotificationPermissionState } from "@/features/notifications/hooks";
import type { NotificationSettings } from "@/features/notifications/hooks";
import type {
  SoundMode,
  SoundName,
  SoundSlot,
} from "@/features/notifications/lib/sound";
import type { SettingsSection } from "@/features/settings/ui/SettingsPanels";
import { SettingsView } from "@/features/settings/ui/SettingsView";

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
  onSetJobProgressSoundEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
  onSetNotifyWhileViewing: (enabled: boolean) => void;
  onSetSingleSound: (name: SoundName) => void;
  onSetSoundEnabled: (enabled: boolean) => void;
  onSetSoundForSlot: (slot: SoundSlot, name: SoundName) => void;
  onSetSoundMode: (mode: SoundMode) => void;
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
  onSetJobProgressSoundEnabled,
  onSetMentionNotificationsEnabled,
  onSetNeedsActionNotificationsEnabled,
  onSetNotifyWhileViewing,
  onSetSingleSound,
  onSetSoundEnabled,
  onSetSoundForSlot,
  onSetSoundMode,
  section,
}: SettingsScreenProps) {
  return (
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
      onSetJobProgressSoundEnabled={onSetJobProgressSoundEnabled}
      onSetMentionNotificationsEnabled={onSetMentionNotificationsEnabled}
      onSetNeedsActionNotificationsEnabled={
        onSetNeedsActionNotificationsEnabled
      }
      onSetNotifyWhileViewing={onSetNotifyWhileViewing}
      onSetSingleSound={onSetSingleSound}
      onSetSoundEnabled={onSetSoundEnabled}
      onSetSoundForSlot={onSetSoundForSlot}
      onSetSoundMode={onSetSoundMode}
      section={section}
    />
  );
}
