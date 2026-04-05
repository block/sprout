import {
  BellRing,
  KeyRound,
  MonitorCog,
  Moon,
  Stethoscope,
  Sun,
  UserRound,
  type LucideIcon,
} from "lucide-react";
import type {
  DesktopNotificationPermissionState,
  NotificationSettings,
} from "@/features/notifications/hooks";
import { TokenSettingsCard } from "@/features/tokens/ui/TokenSettingsCard";
import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { DoctorSettingsPanel } from "./DoctorSettingsPanel";
import { NotificationSettingsCard } from "./NotificationSettingsCard";
import { ProfileSettingsCard } from "./ProfileSettingsCard";

export type SettingsSection =
  | "profile"
  | "notifications"
  | "appearance"
  | "tokens"
  | "doctor";

export const DEFAULT_SETTINGS_SECTION: SettingsSection = "profile";

export type SettingsSectionDescriptor = {
  value: SettingsSection;
  label: string;
  icon: LucideIcon;
};

export type SettingsPanelProps = {
  currentPubkey?: string;
  fallbackDisplayName?: string;
  isUpdatingDesktopNotifications: boolean;
  notificationErrorMessage: string | null;
  notificationPermission: DesktopNotificationPermissionState;
  notificationSettings: NotificationSettings;
  onSetDesktopNotificationsEnabled: (enabled: boolean) => Promise<boolean>;
  onSetHomeBadgeEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
};

type ThemeOption = {
  value: "light" | "dark" | "system";
  label: string;
  icon: LucideIcon;
};

export const settingsSections: SettingsSectionDescriptor[] = [
  {
    value: "profile",
    label: "Profile",
    icon: UserRound,
  },
  {
    value: "notifications",
    label: "Notifications",
    icon: BellRing,
  },
  {
    value: "appearance",
    label: "Appearance",
    icon: MonitorCog,
  },
  {
    value: "tokens",
    label: "Tokens",
    icon: KeyRound,
  },
  {
    value: "doctor",
    label: "Doctor",
    icon: Stethoscope,
  },
];

const themeOptions: ThemeOption[] = [
  {
    value: "light",
    label: "Light",
    icon: Sun,
  },
  {
    value: "dark",
    label: "Dark",
    icon: Moon,
  },
  {
    value: "system",
    label: "System",
    icon: MonitorCog,
  },
];

function ThemeSettingsCard() {
  const { setTheme, theme } = useTheme();

  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-theme"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold tracking-tight">Appearance</h2>
          <p className="text-sm text-muted-foreground">
            Choose how Sprout looks on this device.
          </p>
        </div>

        <div className="inline-flex w-full flex-col gap-1 rounded-xl border border-border/70 bg-background/70 p-1 sm:w-auto sm:flex-row">
          {themeOptions.map(({ value, label, icon: Icon }) => {
            const isActive = theme === value;

            return (
              <button
                aria-pressed={isActive}
                className={cn(
                  "inline-flex items-center justify-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  isActive
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
                data-testid={`theme-option-${value}`}
                key={value}
                onClick={() => {
                  setTheme(value);
                }}
                type="button"
              >
                <Icon className="h-4 w-4" />
                <span>{label}</span>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}

export function renderSettingsSection(
  section: SettingsSection,
  props: SettingsPanelProps,
): React.ReactNode {
  switch (section) {
    case "profile":
      return (
        <ProfileSettingsCard
          currentPubkey={props.currentPubkey}
          fallbackDisplayName={props.fallbackDisplayName}
        />
      );
    case "notifications":
      return (
        <NotificationSettingsCard
          isUpdatingDesktopNotifications={props.isUpdatingDesktopNotifications}
          notificationErrorMessage={props.notificationErrorMessage}
          notificationPermission={props.notificationPermission}
          notificationSettings={props.notificationSettings}
          onSetDesktopNotificationsEnabled={
            props.onSetDesktopNotificationsEnabled
          }
          onSetHomeBadgeEnabled={props.onSetHomeBadgeEnabled}
          onSetMentionNotificationsEnabled={
            props.onSetMentionNotificationsEnabled
          }
          onSetNeedsActionNotificationsEnabled={
            props.onSetNeedsActionNotificationsEnabled
          }
        />
      );
    case "appearance":
      return <ThemeSettingsCard />;
    case "tokens":
      return <TokenSettingsCard currentPubkey={props.currentPubkey} />;
    case "doctor":
      return <DoctorSettingsPanel />;
    default: {
      const exhaustiveCheck: never = section;
      return exhaustiveCheck;
    }
  }
}
