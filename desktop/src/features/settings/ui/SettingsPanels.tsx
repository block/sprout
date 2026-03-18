import {
  BellRing,
  CircleDot,
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
import {
  PresenceBadge,
  PresenceDot,
} from "@/features/presence/ui/PresenceBadge";
import { TokenSettingsCard } from "@/features/tokens/ui/TokenSettingsCard";
import type { PresenceStatus } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { DoctorSettingsPanel } from "./DoctorSettingsPanel";
import { NotificationSettingsCard } from "./NotificationSettingsCard";
import { ProfileSettingsCard } from "./ProfileSettingsCard";

export type SettingsSection =
  | "profile"
  | "presence"
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
  isPresenceLoading: boolean;
  isUpdatingPresence: boolean;
  notificationErrorMessage: string | null;
  notificationPermission: DesktopNotificationPermissionState;
  notificationSettings: NotificationSettings;
  onSetDesktopNotificationsEnabled: (enabled: boolean) => Promise<boolean>;
  onSetHomeBadgeEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
  onSetPresence: (status: PresenceStatus) => Promise<void>;
  presenceError: Error | null;
  presenceStatus: PresenceStatus;
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
    value: "presence",
    label: "Presence",
    icon: CircleDot,
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

const presenceOptions: Array<{
  value: PresenceStatus;
  label: string;
  description: string;
}> = [
  {
    value: "online",
    label: "Online",
    description:
      "Automatically active while you use the app and away when idle.",
  },
  {
    value: "away",
    label: "Away",
    description:
      "Forces this desktop session to appear idle until you change it.",
  },
  {
    value: "offline",
    label: "Offline",
    description: "Hides this desktop session and stops presence heartbeats.",
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

function PresenceStatusBadge({ status }: { status: PresenceStatus }) {
  return (
    <PresenceBadge data-testid="presence-current-status" status={status} />
  );
}

function PresenceSettingsCard({
  isLoading,
  isUpdating,
  onSetPresence,
  presenceError,
  presenceStatus,
}: {
  isLoading: boolean;
  isUpdating: boolean;
  onSetPresence: (status: PresenceStatus) => Promise<void>;
  presenceError: Error | null;
  presenceStatus: PresenceStatus;
}) {
  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-presence"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold tracking-tight">Presence</h2>
          <p className="text-sm text-muted-foreground">
            Choose how this desktop session appears on the relay.
          </p>
        </div>
        <PresenceStatusBadge status={presenceStatus} />
      </div>

      <div className="mt-4 grid gap-2 md:grid-cols-3">
        {presenceOptions.map((option) => {
          const isActive = presenceStatus === option.value;

          return (
            <button
              aria-pressed={isActive}
              className={cn(
                "flex min-h-24 flex-col items-start justify-between rounded-xl border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                isActive
                  ? "border-primary bg-primary/10 text-foreground"
                  : "border-border/80 bg-background/60 text-muted-foreground hover:bg-accent hover:text-accent-foreground",
              )}
              data-testid={`presence-option-${option.value}`}
              disabled={isLoading || isUpdating}
              key={option.value}
              onClick={() => {
                void onSetPresence(option.value);
              }}
              type="button"
            >
              <div className="flex items-center gap-2">
                <PresenceDot className="h-4 w-4" status={option.value} />
                <span className="font-medium text-foreground">
                  {option.label}
                </span>
              </div>
              <p className="text-sm text-muted-foreground">
                {option.description}
              </p>
            </button>
          );
        })}
      </div>

      {presenceError ? (
        <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {presenceError.message}
        </p>
      ) : null}

      <p
        className="mt-4 text-sm text-muted-foreground"
        data-testid="presence-help"
      >
        Sprout refreshes presence every minute while it is running. Online will
        switch to away after a few minutes of inactivity or when the app is
        hidden. The relay expires presence after 90 seconds.
      </p>
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
    case "presence":
      return (
        <PresenceSettingsCard
          isLoading={props.isPresenceLoading}
          isUpdating={props.isUpdatingPresence}
          onSetPresence={props.onSetPresence}
          presenceError={props.presenceError}
          presenceStatus={props.presenceStatus}
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
