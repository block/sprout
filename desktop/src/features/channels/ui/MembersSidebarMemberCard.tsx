import { Ellipsis, Play, RotateCcw, Square, Trash2 } from "lucide-react";

import {
  getManagedAgentPrimaryActionLabel,
  isManagedAgentActive,
} from "@/features/agents/lib/managedAgentControlActions";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { getPresenceLabel } from "@/features/presence/lib/presence";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import type {
  ChannelMember,
  ManagedAgent,
  PresenceStatus,
} from "@/shared/api/types";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

type MembersSidebarMemberCardProps = {
  canRemoveMember: boolean;
  isActionPending: boolean;
  isArchived: boolean;
  managedAgent?: ManagedAgent;
  member: ChannelMember;
  memberIsBot: boolean;
  memberLabel: string;
  onManagedAgentAction: (agent: ManagedAgent) => void;
  onRemoveMember: (member: ChannelMember) => void;
  presenceStatus?: PresenceStatus | null;
  profileAvatarUrl?: string | null;
};

function formatRoleLabel(member: ChannelMember, memberIsBot: boolean) {
  if (memberIsBot) {
    return "Bot";
  }

  return `${member.role[0]?.toUpperCase() ?? ""}${member.role.slice(1)}`;
}

function formatManagedAgentStatus(agent: ManagedAgent) {
  switch (agent.status) {
    case "running":
      return "Running";
    case "stopped":
      return "Stopped";
    case "deployed":
      return "Deployed";
    case "not_deployed":
      return "Not deployed";
  }
}

export function MembersSidebarMemberCard({
  canRemoveMember,
  isActionPending,
  isArchived,
  managedAgent,
  member,
  memberIsBot,
  memberLabel,
  onManagedAgentAction,
  onRemoveMember,
  presenceStatus,
  profileAvatarUrl,
}: MembersSidebarMemberCardProps) {
  const roleLabel = formatRoleLabel(member, memberIsBot);
  const disabled = isActionPending || isArchived;
  const hasActions = memberIsBot
    ? Boolean(managedAgent) || canRemoveMember
    : canRemoveMember;

  return (
    <div
      className="group flex items-center justify-between gap-3 rounded-lg px-3 py-2 transition-colors hover:bg-muted/40"
      data-testid={`sidebar-member-${member.pubkey}`}
    >
      <div className="flex min-w-0 items-center gap-3">
        <ProfileAvatar
          avatarUrl={profileAvatarUrl ?? null}
          className="h-9 w-9 rounded-full text-[11px] shadow-none"
          iconClassName="h-4 w-4"
          label={memberLabel}
        />
        <div className="min-w-0 space-y-0.5">
          <p className="truncate text-sm font-medium leading-5">
            {memberLabel}
          </p>
          <div
            className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground"
            data-testid={`sidebar-member-presence-${member.pubkey}`}
          >
            {presenceStatus ? (
              <>
                <PresenceDot className="h-2 w-2" status={presenceStatus} />
                <span>{getPresenceLabel(presenceStatus)}</span>
                <span aria-hidden="true">&middot;</span>
              </>
            ) : null}
            <span>{roleLabel}</span>
            {managedAgent ? (
              <>
                <span aria-hidden="true">&middot;</span>
                <span
                  className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground"
                  data-testid={`sidebar-managed-agent-status-${member.pubkey}`}
                >
                  {formatManagedAgentStatus(managedAgent)}
                </span>
              </>
            ) : null}
          </div>
        </div>
      </div>
      {hasActions ? (
        <MemberActionsMenu
          canRemoveMember={canRemoveMember}
          disabled={disabled}
          managedAgent={managedAgent}
          member={member}
          memberIsBot={memberIsBot}
          onManagedAgentAction={onManagedAgentAction}
          onRemoveMember={onRemoveMember}
        />
      ) : null}
    </div>
  );
}

function MemberActionsMenu({
  canRemoveMember,
  disabled,
  managedAgent,
  member,
  memberIsBot,
  onManagedAgentAction,
  onRemoveMember,
}: {
  canRemoveMember: boolean;
  disabled: boolean;
  managedAgent?: ManagedAgent;
  member: ChannelMember;
  memberIsBot: boolean;
  onManagedAgentAction: (agent: ManagedAgent) => void;
  onRemoveMember: (member: ChannelMember) => void;
}) {
  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <button
          className="invisible flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground group-hover:visible hover:bg-muted hover:text-foreground data-[state=open]:visible"
          data-testid={`sidebar-member-menu-${member.pubkey}`}
          type="button"
        >
          <Ellipsis className="h-4 w-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        {memberIsBot && managedAgent ? (
          <>
            <DropdownMenuItem
              data-testid={`sidebar-agent-action-${member.pubkey}`}
              disabled={disabled}
              onClick={() => onManagedAgentAction(managedAgent)}
            >
              {getManagedAgentActionIcon(managedAgent)}
              {getManagedAgentPrimaryActionLabel(managedAgent)}
            </DropdownMenuItem>
            {canRemoveMember ? <DropdownMenuSeparator /> : null}
          </>
        ) : null}
        {canRemoveMember ? (
          <DropdownMenuItem
            className="text-destructive focus:text-destructive"
            data-testid={`sidebar-remove-member-${member.pubkey}`}
            disabled={disabled}
            onClick={() => onRemoveMember(member)}
          >
            <Trash2 className="h-4 w-4" />
            Remove from channel
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function getManagedAgentActionIcon(agent: ManagedAgent) {
  if (isManagedAgentActive(agent)) {
    return <Square className="h-4 w-4" />;
  }

  if (agent.backend.type === "local" && agent.status === "stopped") {
    return <RotateCcw className="h-4 w-4" />;
  }

  return <Play className="h-4 w-4" />;
}
