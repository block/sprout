import { useState, useMemo, useRef } from "react";
import {
  BellRing,
  Check,
  KeyRound,
  MonitorCog,
  Moon,
  Search,
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
import { ACCENT_COLORS, useTheme } from "@/shared/theme/ThemeProvider";
import { SYNTAX_THEMES, isLightTheme } from "@/shared/theme/theme-loader";
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

function formatThemeLabel(name: string): string {
  return name
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function ThemeSettingsCard() {
  const { setTheme, themeName, accentColor, setAccentColor } = useTheme();
  const [search, setSearch] = useState("");
  const didScrollRef = useRef(false);
  const activeRef = (node: HTMLButtonElement | null) => {
    if (node && !didScrollRef.current) {
      didScrollRef.current = true;
      node.scrollIntoView({ block: "center" });
    }
  };

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    if (!q) return SYNTAX_THEMES;
    return SYNTAX_THEMES.filter((name) => name.includes(q));
  }, [search]);

  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-theme"
    >
      <div className="mb-3 min-w-0">
        <h2 className="text-sm font-semibold tracking-tight">Appearance</h2>
        <p className="text-sm text-muted-foreground">
          Choose a theme for Sprout. Light and dark mode is auto-detected.
        </p>
      </div>

      <div className="relative mb-3">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <input
          className="w-full rounded-lg border border-border/70 bg-background/70 py-2 pl-9 pr-3 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search themes..."
          type="text"
          value={search}
        />
      </div>

      <div className="max-h-72 overflow-y-auto rounded-lg border border-border/70 bg-background/70">
        {filtered.length === 0 ? (
          <p className="px-3 py-4 text-center text-sm text-muted-foreground">
            No themes match your search.
          </p>
        ) : (
          filtered.map((name) => {
            const isActive = themeName === name;
            const light = isLightTheme(name);

            return (
              <button
                aria-pressed={isActive}
                className={cn(
                  "flex w-full items-center gap-3 px-3 py-2 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                  isActive
                    ? "bg-primary/10 text-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
                data-testid={`theme-option-${name}`}
                key={name}
                onClick={() => setTheme(name)}
                ref={isActive ? activeRef : undefined}
                type="button"
              >
                {light ? (
                  <Sun className="h-4 w-4 shrink-0" />
                ) : (
                  <Moon className="h-4 w-4 shrink-0" />
                )}
                <span className="flex-1 truncate">
                  {formatThemeLabel(name)}
                </span>
                {isActive && (
                  <Check className="h-4 w-4 shrink-0 text-primary" />
                )}
              </button>
            );
          })
        )}
      </div>

      <div className="mt-4">
        <h3 className="mb-2 text-sm font-medium">Accent Color</h3>
        <div className="flex gap-2">
          {ACCENT_COLORS.map((color) => (
            <button
              className={cn(
                "flex h-7 w-7 items-center justify-center rounded-full transition-transform hover:scale-110",
                accentColor === color.value &&
                  "ring-2 ring-ring ring-offset-2 ring-offset-background",
              )}
              data-testid={`accent-color-${color.name.toLowerCase()}`}
              key={color.value}
              onClick={() => setAccentColor(color.value)}
              style={{ backgroundColor: color.value }}
              title={color.name}
              type="button"
            >
              {accentColor === color.value && (
                <Check className="h-3.5 w-3.5 text-white" />
              )}
            </button>
          ))}
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
