import { Play, Square, UserMinus } from "lucide-react";

import { MembersSidebarIconButton } from "./MembersSidebarIconButton";

type MembersSidebarAgentControlsProps = {
  canBulkRemove: boolean;
  canBulkRespawn: boolean;
  canBulkStop: boolean;
  disabled: boolean;
  onRemoveAll: () => void;
  onRespawnAll: () => void;
  onStopAll: () => void;
};

export function MembersSidebarAgentControls({
  canBulkRemove,
  canBulkRespawn,
  canBulkStop,
  disabled,
  onRemoveAll,
  onRespawnAll,
  onStopAll,
}: MembersSidebarAgentControlsProps) {
  return (
    <div
      className="ml-auto flex flex-wrap items-center gap-1"
      data-testid="members-sidebar-agent-controls"
    >
      <MembersSidebarIconButton
        actionLabel="Spawn or respawn all managed bots"
        data-testid="members-sidebar-respawn-all"
        disabled={disabled || !canBulkRespawn}
        icon={<Play className="h-4 w-4" />}
        onClick={onRespawnAll}
        variant="default"
      />
      <MembersSidebarIconButton
        actionLabel="Stop or shut down all managed bots"
        data-testid="members-sidebar-stop-all"
        disabled={disabled || !canBulkStop}
        icon={<Square className="h-4 w-4" />}
        onClick={onStopAll}
        variant="outline"
      />
      {canBulkRemove ? (
        <MembersSidebarIconButton
          actionLabel="Remove all managed bots from this channel"
          className="text-muted-foreground hover:text-foreground"
          data-testid="members-sidebar-remove-all"
          disabled={disabled}
          icon={<UserMinus className="h-4 w-4" />}
          onClick={onRemoveAll}
          variant="ghost"
        />
      ) : null}
    </div>
  );
}
