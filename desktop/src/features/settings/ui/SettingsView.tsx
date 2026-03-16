import { getCurrentWindow } from "@tauri-apps/api/window";
import { X } from "lucide-react";
import type * as React from "react";

import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
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
  onClose: () => void;
  onSectionChange: (section: SettingsSection) => void;
  section: SettingsSection;
};

function handleSettingsHeaderPointerDown(event: React.PointerEvent) {
  if (event.button !== 0) {
    return;
  }

  const target = event.target as HTMLElement;
  if (target.closest('button, a, input, textarea, [role="button"]')) {
    return;
  }

  event.preventDefault();
  getCurrentWindow().startDragging();
}

function SettingsSectionButton({
  active,
  onSelect,
  section,
}: {
  active: boolean;
  onSelect: (section: SettingsSection) => void;
  section: (typeof settingsSections)[number];
}) {
  const Icon = section.icon;

  return (
    <button
      aria-pressed={active}
      className={cn(
        "group inline-flex min-w-fit items-center gap-2 rounded-lg border px-3 py-2 text-sm font-medium whitespace-nowrap transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring lg:w-full lg:justify-start",
        active
          ? "border-border bg-background text-foreground shadow-sm"
          : "border-transparent bg-transparent text-muted-foreground hover:bg-background/70 hover:text-foreground",
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
  isPresenceLoading,
  isUpdatingPresence,
  onClose,
  onSectionChange,
  onSetPresence,
  presenceError,
  presenceStatus,
  section,
}: SettingsViewProps) {
  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background"
      data-testid="settings-view"
    >
      <header
        className="flex items-start justify-between gap-4 border-b border-border/80 bg-background px-4 pb-4 pt-8 sm:px-6"
        onPointerDown={handleSettingsHeaderPointerDown}
      >
        <div className="min-w-0 pt-0.5">
          <h1
            className="text-lg font-semibold tracking-tight"
            data-testid="settings-title"
          >
            Settings
          </h1>
          <p className="text-sm text-muted-foreground">
            Manage your relay identity, desktop preferences, and local access
            tokens.
          </p>
        </div>

        <Button
          aria-label="Close settings"
          className="shrink-0 text-muted-foreground hover:text-foreground"
          data-testid="settings-close"
          onClick={onClose}
          size="icon"
          title="Close settings"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </header>

      <div className="grid min-h-0 flex-1 grid-rows-[auto_minmax(0,1fr)] overflow-hidden lg:grid-cols-[260px_minmax(0,1fr)] lg:grid-rows-1">
        <aside className="border-b border-border/70 bg-muted/20 lg:border-b-0 lg:border-r">
          <nav
            aria-label="Settings sections"
            className="flex gap-2 overflow-x-auto px-3 py-4 lg:flex-col lg:overflow-y-auto"
          >
            {settingsSections.map((entry) => (
              <SettingsSectionButton
                active={entry.value === section}
                key={entry.value}
                onSelect={onSectionChange}
                section={entry}
              />
            ))}
          </nav>
        </aside>

        <section className="min-h-0 overflow-y-auto px-4 py-4 sm:px-6">
          <div
            className="mx-auto flex w-full max-w-4xl flex-col gap-4"
            data-testid={`settings-panel-${section}`}
          >
            {renderSettingsSection(section, {
              currentPubkey,
              fallbackDisplayName,
              isPresenceLoading,
              isUpdatingPresence,
              onSetPresence,
              presenceError,
              presenceStatus,
            })}
          </div>
        </section>
      </div>
    </div>
  );
}
