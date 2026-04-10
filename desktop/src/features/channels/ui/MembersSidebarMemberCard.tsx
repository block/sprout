import { Play, RotateCcw, Square, X } from "lucide-react";

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
import { Button } from "@/shared/ui/button";
import { MembersSidebarIconButton } from "./MembersSidebarIconButton";

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
  const actionLabel = managedAgent
    ? getManagedAgentPrimaryActionLabel(managedAgent)
    : null;

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
                <span>Managed locally</span>
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
      {memberIsBot ? (
        <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          {managedAgent ? (
            <MembersSidebarIconButton
              actionLabel={actionLabel ?? "Manage bot"}
              className="text-muted-foreground hover:text-foreground"
              data-testid={`sidebar-agent-action-${member.pubkey}`}
              disabled={isActionPending || isArchived}
              icon={getManagedAgentActionIcon(managedAgent)}
              onClick={() => {
                onManagedAgentAction(managedAgent);
              }}
              variant="ghost"
            />
          ) : null}
          {canRemoveMember ? (
            <MembersSidebarIconButton
              actionLabel={`Remove ${memberLabel} from channel`}
              className="text-muted-foreground hover:text-destructive"
              data-testid={`sidebar-remove-member-${member.pubkey}`}
              disabled={isActionPending || isArchived}
              icon={<X className="h-3.5 w-3.5" />}
              onClick={() => {
                onRemoveMember(member);
              }}
              variant="ghost"
            />
          ) : null}
        </div>
      ) : canRemoveMember ? (
        <div className="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          <Button
            className="h-7 rounded-md px-2 text-xs text-muted-foreground hover:text-foreground"
            data-testid={`sidebar-remove-member-${member.pubkey}`}
            disabled={isActionPending || isArchived}
            onClick={() => {
              onRemoveMember(member);
            }}
            size="sm"
            type="button"
            variant="ghost"
          >
            Remove
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function getManagedAgentActionIcon(agent: ManagedAgent) {
  if (isManagedAgentActive(agent)) {
    return <Square className="h-3.5 w-3.5" />;
  }

  if (agent.backend.type === "local" && agent.status === "stopped") {
    return <RotateCcw className="h-3.5 w-3.5" />;
  }

  return <Play className="h-3.5 w-3.5" />;
}
