import * as React from "react";
import { getVersion } from "@tauri-apps/api/app";
import { X } from "lucide-react";

import { useMyRelayMembershipQuery } from "@/features/relay-members/hooks";
import { cn } from "@/shared/lib/cn";
import {
  renderSettingsSection,
  settingsSections,
  type SettingsPanelProps,
  type SettingsSection,
} from "./SettingsPanels";

export {
  DEFAULT_SETTINGS_SECTION,
  type SettingsSection,
} from "./SettingsPanels";

type SettingsViewProps = SettingsPanelProps & {
  mode: "profile" | "preferences";
  onClose: () => void;
  onSectionChange: (section: SettingsSection) => void;
  section: SettingsSection;
};

function SettingsSectionButton({
  active,
  isLoaded,
  onSelect,
  section,
}: {
  active: boolean;
  isLoaded: boolean;
  onSelect: (section: SettingsSection) => void;
  section: (typeof settingsSections)[number];
}) {
  const Icon = section.icon;

  return (
    <button
      aria-pressed={active}
      className={cn(
        "group inline-flex min-w-fit items-center gap-2 rounded-lg border px-3 py-2 text-sm font-medium whitespace-nowrap motion-safe:transition-all motion-safe:duration-200 motion-safe:ease-out focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring",
        active
          ? "border-border bg-background text-foreground shadow-xs"
          : "border-transparent bg-transparent text-muted-foreground hover:bg-background/70 hover:text-foreground",
        isLoaded ? "opacity-100 translate-x-0" : "opacity-0 -translate-x-2",
      )}
      data-testid={`settings-nav-${section.value}`}
      onClick={() => onSelect(section.value)}
      type="button"
    >
      <Icon
        className={cn(
          "h-4 w-4 shrink-0 transition-colors",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="truncate">{section.label}</span>
    </button>
  );
}

export function SettingsView({
  currentPubkey,
  fallbackDisplayName,
  isUpdatingDesktopNotifications,
  notificationErrorMessage,
  notificationPermission,
  notificationSettings,
  mode,
  onClose,
  onSectionChange,
  onSetDesktopNotificationsEnabled,
  onSetHomeBadgeEnabled,
  onSetMentionNotificationsEnabled,
  onSetNeedsActionNotificationsEnabled,
  onSetSoundEnabled,
  section,
}: SettingsViewProps) {
  const myMembershipQuery = useMyRelayMembershipQuery();
  const visibleSections = React.useMemo(() => {
    const membership = myMembershipQuery.data;
    return settingsSections.filter((s) => {
      if (mode === "preferences" && s.value === "profile") {
        return false;
      }
      if (s.value === "relay-members") {
        return (
          membership != null &&
          (membership.role === "owner" || membership.role === "admin")
        );
      }
      return true;
    });
  }, [mode, myMembershipQuery.data]);

  const [isLoaded, setIsLoaded] = React.useState(false);
  const [appVersion, setAppVersion] = React.useState<string | null>(null);
  React.useEffect(() => {
    const frameId = window.requestAnimationFrame(() => setIsLoaded(true));
    return () => window.cancelAnimationFrame(frameId);
  }, []);
  React.useEffect(() => {
    void getVersion().then(setAppVersion);
  }, []);

  React.useEffect(() => {
    if (mode === "profile") {
      if (section !== "profile") {
        onSectionChange("profile");
      }
      return;
    }

    if (!visibleSections.some((entry) => entry.value === section)) {
      onSectionChange(visibleSections[0]?.value ?? "appearance");
    }
  }, [mode, onSectionChange, section, visibleSections]);

  const showSectionNav = mode === "preferences";

  React.useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div
      className={cn(
        "fixed inset-0 z-50 flex items-center justify-center motion-safe:transition-opacity motion-safe:duration-200",
        isLoaded ? "opacity-100" : "opacity-0",
      )}
      data-testid="settings-overlay"
    >
      <div
        aria-hidden="true"
        className="absolute inset-0 bg-background/80 backdrop-blur-sm"
        onClick={onClose}
      />

      {/* biome-ignore lint/a11y/useKeyWithClickEvents: Click stops propagation to backdrop; keyboard dismiss handled by Escape key. */}
      <div
        aria-labelledby="settings-title"
        aria-modal="true"
        className={cn(
          "relative mx-auto flex h-[min(600px,calc(100vh-2rem))] w-[calc(100%-4rem)] max-w-3xl flex-col overflow-hidden rounded-xl border border-border bg-background shadow-lg motion-safe:transition-all motion-safe:duration-200 motion-safe:ease-out",
          isLoaded ? "opacity-100 scale-100" : "opacity-0 scale-95",
        )}
        data-testid="settings-view"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
      >
        <header
          className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3"
          data-tauri-drag-region
        >
          <h2
            className="text-base font-semibold"
            data-testid="settings-title"
            id="settings-title"
          >
            {mode === "profile" ? "Profile" : "Settings"}
          </h2>
          <button
            aria-label="Close settings"
            className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            data-testid="settings-close"
            onClick={onClose}
            type="button"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div
          className={cn(
            "grid min-h-0 flex-1 grid-rows-[auto_minmax(0,1fr)] overflow-hidden md:grid-rows-1",
            showSectionNav
              ? "md:grid-cols-[220px_minmax(0,1fr)]"
              : "md:grid-cols-1",
          )}
        >
          {showSectionNav ? (
            <aside
              className={cn(
                "flex flex-col border-b border-border/70 bg-muted/20 motion-safe:transition-all motion-safe:duration-200 motion-safe:ease-out md:border-b-0 md:border-r",
                isLoaded
                  ? "opacity-100 translate-x-0"
                  : "opacity-0 -translate-x-2",
              )}
            >
              <nav
                aria-label="Settings sections"
                className="flex gap-1 overflow-x-auto px-3 py-3 md:flex-1 md:flex-col md:overflow-y-auto md:pt-1"
              >
                {visibleSections.map((entry) => (
                  <SettingsSectionButton
                    active={entry.value === section}
                    isLoaded={isLoaded}
                    key={entry.value}
                    onSelect={onSectionChange}
                    section={entry}
                  />
                ))}
              </nav>
              {appVersion ? (
                <p className="hidden px-3 pb-3 text-xs text-muted-foreground/60 md:block">
                  v{appVersion}
                </p>
              ) : null}
            </aside>
          ) : null}

          <section className="flex min-h-0 flex-col overflow-y-auto px-4 pt-4 sm:px-6">
            <div
              className="mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4"
              data-testid={`settings-panel-${section}`}
            >
              {renderSettingsSection(section, {
                currentPubkey,
                fallbackDisplayName,
                isUpdatingDesktopNotifications,
                notificationErrorMessage,
                notificationPermission,
                notificationSettings,
                onSetDesktopNotificationsEnabled,
                onSetHomeBadgeEnabled,
                onSetMentionNotificationsEnabled,
                onSetNeedsActionNotificationsEnabled,
                onSetSoundEnabled,
              })}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
