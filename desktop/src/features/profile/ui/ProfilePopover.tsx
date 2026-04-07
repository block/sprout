import type * as React from "react";
import { Settings } from "lucide-react";

import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  PresenceDot,
  PresenceBadge,
} from "@/features/presence/ui/PresenceBadge";
import type { PresenceStatus } from "@/shared/api/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProfilePopoverProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  displayName: string;
  nip05?: string | null;
  avatarUrl: string | null;
  currentStatus: PresenceStatus;
  isStatusPending?: boolean;
  onSetStatus: (status: PresenceStatus) => void;
  onOpenSettings: () => void;
  children: React.ReactNode;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MENU_ITEM_CLASS =
  "flex w-full items-center gap-3 px-3 py-2.5 rounded-lg text-left hover:bg-accent cursor-pointer transition-colors";

const ALL_STATUSES: PresenceStatus[] = ["online", "away", "offline"];

const STATUS_ACTION_LABELS: Record<PresenceStatus, string> = {
  online: "Set yourself as online",
  away: "Set yourself as away",
  offline: "Set yourself as offline",
};

// ---------------------------------------------------------------------------
// ProfilePopover
// ---------------------------------------------------------------------------

export function ProfilePopover({
  open,
  onOpenChange,
  displayName,
  nip05,
  avatarUrl,
  currentStatus,
  isStatusPending,
  onSetStatus,
  onOpenSettings,
  children,
}: ProfilePopoverProps) {
  const otherStatuses = ALL_STATUSES.filter((s) => s !== currentStatus);
  const isMac =
    typeof navigator !== "undefined" &&
    /Mac|iPod|iPhone|iPad/.test(navigator.userAgent);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>

      <PopoverContent
        side="top"
        align="start"
        sideOffset={8}
        className="w-[280px] rounded-xl border border-border bg-popover p-0 shadow-lg"
        data-testid="profile-popover"
      >
        <div aria-label="Profile menu" role="menu">
          {/* ── Identity block ─────────────────────────────────── */}
          <div className="flex items-center gap-3 px-4 py-3">
            <div className="relative shrink-0">
              <ProfileAvatar
                avatarUrl={avatarUrl}
                className="h-10 w-10 rounded-2xl text-sm"
                iconClassName="h-5 w-5"
                label={displayName}
              />
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold text-popover-foreground">
                {displayName}
              </p>
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                {nip05 ? <span className="truncate">@{nip05}</span> : null}
                {nip05 ? <span aria-hidden="true">·</span> : null}
                <PresenceBadge
                  className="border-0 bg-transparent px-0 py-0 text-xs"
                  data-testid="profile-popover-current-status"
                  status={currentStatus}
                />
              </div>
            </div>
          </div>

          <hr className="my-1 h-px border-0 bg-border" />

          {/* ── Status options ─────────────────────────────────── */}
          <div className="px-1.5 py-1">
            {otherStatuses.map((status) => (
              <button
                key={status}
                className={MENU_ITEM_CLASS}
                data-testid={`profile-popover-status-${status}`}
                disabled={isStatusPending}
                onClick={() => {
                  onSetStatus(status);
                  onOpenChange(false);
                }}
                role="menuitem"
                type="button"
              >
                <PresenceDot className="h-2.5 w-2.5" status={status} />
                <span className="text-sm text-popover-foreground">
                  {STATUS_ACTION_LABELS[status]}
                </span>
              </button>
            ))}
          </div>

          <hr className="my-1 h-px border-0 bg-border" />

          {/* ── Settings ───────────────────────────────────────── */}
          <div className="px-1.5 py-1">
            <button
              className={MENU_ITEM_CLASS}
              data-testid="profile-popover-settings"
              onClick={() => {
                onOpenChange(false);
                window.requestAnimationFrame(() => {
                  onOpenSettings();
                });
              }}
              role="menuitem"
              type="button"
            >
              <Settings className="h-4 w-4 text-muted-foreground" />
              <span className="flex-1 text-sm text-popover-foreground">
                Settings
              </span>
              <kbd className="text-xs text-muted-foreground">
                {isMac ? "⌘," : "Ctrl+,"}
              </kbd>
            </button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
