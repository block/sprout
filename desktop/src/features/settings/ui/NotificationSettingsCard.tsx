import {
  AtSign,
  BellRing,
  CircleAlert,
  Home as HomeIcon,
  type LucideIcon,
} from "lucide-react";

import type {
  DesktopNotificationPermissionState,
  NotificationSettings,
} from "@/features/notifications/hooks";
import { cn } from "@/shared/lib/cn";

function notificationStatusLabel(
  desktopEnabled: boolean,
  permission: DesktopNotificationPermissionState,
) {
  if (permission === "unsupported") {
    return "Unavailable";
  }

  if (permission === "denied") {
    return "Blocked";
  }

  return desktopEnabled ? "On" : "Off";
}

function notificationStatusClassName(
  desktopEnabled: boolean,
  permission: DesktopNotificationPermissionState,
) {
  if (permission === "unsupported" || permission === "denied") {
    return "border-destructive/30 bg-destructive/10 text-destructive";
  }

  if (desktopEnabled) {
    return "border-primary/30 bg-primary/10 text-primary";
  }

  return "border-border/80 bg-muted text-muted-foreground";
}

function NotificationPreferenceCard({
  description,
  disabled = false,
  enabled,
  icon: Icon,
  onToggle,
  testId,
  title,
}: {
  description: string;
  disabled?: boolean;
  enabled: boolean;
  icon: LucideIcon;
  onToggle: () => void;
  testId: string;
  title: string;
}) {
  return (
    <button
      aria-pressed={enabled}
      className={cn(
        "flex min-h-24 flex-col items-start justify-between rounded-xl border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        enabled
          ? "border-primary bg-primary/10 text-foreground"
          : "border-border/80 bg-background/60 text-muted-foreground hover:bg-accent hover:text-accent-foreground",
      )}
      data-testid={testId}
      disabled={disabled}
      onClick={onToggle}
      type="button"
    >
      <div className="flex items-center gap-2">
        <Icon className="h-4 w-4" />
        <span className="font-medium text-foreground">{title}</span>
      </div>
      <p className="text-sm text-muted-foreground">{description}</p>
    </button>
  );
}

export function NotificationSettingsCard({
  isUpdatingDesktopNotifications,
  notificationErrorMessage,
  notificationPermission,
  notificationSettings,
  onSetDesktopNotificationsEnabled,
  onSetHomeBadgeEnabled,
  onSetMentionNotificationsEnabled,
  onSetNeedsActionNotificationsEnabled,
}: {
  isUpdatingDesktopNotifications: boolean;
  notificationErrorMessage: string | null;
  notificationPermission: DesktopNotificationPermissionState;
  notificationSettings: NotificationSettings;
  onSetDesktopNotificationsEnabled: (enabled: boolean) => Promise<boolean>;
  onSetHomeBadgeEnabled: (enabled: boolean) => void;
  onSetMentionNotificationsEnabled: (enabled: boolean) => void;
  onSetNeedsActionNotificationsEnabled: (enabled: boolean) => void;
}) {
  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-notifications"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold tracking-tight">
            Notifications
          </h2>
          <p className="text-sm text-muted-foreground">
            Zero by default. Keep channel noise inside Home, then opt in to
            desktop alerts only for the items that truly need you.
          </p>
        </div>
        <span
          className={cn(
            "inline-flex items-center rounded-full border px-3 py-1 text-xs font-semibold uppercase tracking-[0.18em]",
            notificationStatusClassName(
              notificationSettings.desktopEnabled,
              notificationPermission,
            ),
          )}
          data-testid="notifications-desktop-state"
        >
          {notificationStatusLabel(
            notificationSettings.desktopEnabled,
            notificationPermission,
          )}
        </span>
      </div>

      <div className="mt-4 grid gap-2 md:grid-cols-2">
        <NotificationPreferenceCard
          description={
            notificationSettings.desktopEnabled
              ? "Native desktop alerts are enabled for the categories you have armed below."
              : "Request OS permission and surface new mentions or needs-action items outside the app."
          }
          disabled={isUpdatingDesktopNotifications}
          enabled={notificationSettings.desktopEnabled}
          icon={BellRing}
          onToggle={() => {
            void onSetDesktopNotificationsEnabled(
              !notificationSettings.desktopEnabled,
            );
          }}
          testId="notifications-desktop-toggle"
          title={
            isUpdatingDesktopNotifications ? "Requesting..." : "Desktop alerts"
          }
        />
        <NotificationPreferenceCard
          description="Show a Home badge for mentions and needs-action items in the sidebar."
          enabled={notificationSettings.homeBadgeEnabled}
          icon={HomeIcon}
          onToggle={() => {
            onSetHomeBadgeEnabled(!notificationSettings.homeBadgeEnabled);
          }}
          testId="notifications-home-badge-toggle"
          title="Home badge"
        />
        <NotificationPreferenceCard
          description="Alert when someone tags your pubkey in a channel you can access."
          enabled={notificationSettings.mentions}
          icon={AtSign}
          onToggle={() => {
            onSetMentionNotificationsEnabled(!notificationSettings.mentions);
          }}
          testId="notifications-mentions-toggle"
          title="@Mentions"
        />
        <NotificationPreferenceCard
          description="Alert for reminders and workflow approvals that are waiting on you."
          enabled={notificationSettings.needsAction}
          icon={CircleAlert}
          onToggle={() => {
            onSetNeedsActionNotificationsEnabled(
              !notificationSettings.needsAction,
            );
          }}
          testId="notifications-needs-action-toggle"
          title="Needs action"
        />
      </div>

      {notificationErrorMessage ? (
        <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {notificationErrorMessage}
        </p>
      ) : null}

      <p className="mt-4 text-sm text-muted-foreground">
        The Home badge is an in-app signal. The app icon badge follows unread
        channels, and desktop alerts only fire for new feed items after you
        enable them.
      </p>
    </section>
  );
}
