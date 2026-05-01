import * as React from "react";
import { Activity } from "lucide-react";

import {
  useUserNotesQuery,
  useUserProfileQuery,
} from "@/features/profile/hooks";
import {
  useRelayAgentsQuery,
  useManagedAgentsQuery,
} from "@/features/agents/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { formatRelativeTime } from "@/features/forum/lib/time";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { useAgentSession } from "@/shared/context/AgentSessionContext";

import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { BotIdenticon } from "@/features/messages/ui/BotIdenticon";

type UserProfilePopoverProps = {
  children: React.ReactNode;
  pubkey: string;
  /** When set to "bot", a BotIdenticon badge renders next to the display name. */
  role?: string;
  /** Value used to generate the BotIdenticon glyph (typically the author name). */
  botIdenticonValue?: string;
};

const RUNTIME_LABELS: Record<string, string> = {
  goose: "Goose",
  "claude-code": "Claude Code",
  "codex-acp": "Codex",
  aider: "Aider",
};

function runtimeLabel(command: string): string {
  return RUNTIME_LABELS[command] ?? command;
}

function InfoBadge({ children }: { children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center rounded-full bg-muted/50 px-2 py-0.5 text-xs text-muted-foreground">
      {children}
    </span>
  );
}

function truncatePubkey(pubkey: string) {
  if (pubkey.length <= 16) {
    return pubkey;
  }

  return `${pubkey.slice(0, 8)}…${pubkey.slice(-8)}`;
}

export function UserProfilePopover({
  children,
  pubkey,
  role,
  botIdenticonValue,
}: UserProfilePopoverProps) {
  const [open, setOpen] = React.useState(false);
  const profileQuery = useUserProfileQuery(open ? pubkey : undefined);
  const notesQuery = useUserNotesQuery(open ? pubkey : undefined, {
    limit: 1,
  });
  const relayAgentsQuery = useRelayAgentsQuery({
    enabled: open && role === "bot",
  });
  const managedAgentsQuery = useManagedAgentsQuery({
    enabled: open && role === "bot",
  });
  const presenceQuery = usePresenceQuery(open ? [pubkey] : [], {
    enabled: open,
  });

  const { onOpenAgentSession } = useAgentSession();
  const relayAgent = relayAgentsQuery.data?.find((a) => a.pubkey === pubkey);
  const managedAgent = managedAgentsQuery.data?.find(
    (a) => a.pubkey === pubkey,
  );
  const canViewActivity =
    role === "bot" &&
    managedAgent?.backend.type === "local" &&
    Boolean(onOpenAgentSession);
  const profile = profileQuery.data;
  const latestNote = (notesQuery.data?.notes ?? [])[0] ?? null;
  const presenceStatus = presenceQuery.data?.[pubkey.toLowerCase()];

  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" className="w-80" side="top" sideOffset={8}>
        <div className="flex flex-col gap-3">
          <div className="flex items-start gap-3">
            {profile?.avatarUrl ? (
              <img
                alt={profile.displayName ?? "User avatar"}
                className="h-10 w-10 shrink-0 rounded-xl object-cover shadow-sm"
                referrerPolicy="no-referrer"
                src={rewriteRelayUrl(profile.avatarUrl)}
              />
            ) : (
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-secondary text-xs font-semibold text-secondary-foreground shadow-sm">
                {(profile?.displayName ?? pubkey.slice(0, 2))
                  .slice(0, 2)
                  .toUpperCase()}
              </div>
            )}

            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <p className="truncate text-sm font-semibold">
                  {profile?.displayName ?? truncatePubkey(pubkey)}
                </p>
                {role === "bot" && botIdenticonValue ? (
                  <BotIdenticon
                    value={botIdenticonValue}
                    size={20}
                    className="shrink-0 rounded"
                  />
                ) : null}
              </div>
              {profile?.nip05Handle ? (
                <p className="truncate text-xs text-muted-foreground">
                  {profile.nip05Handle}
                </p>
              ) : null}
              {profile?.displayName ? (
                <p className="truncate font-mono text-[10px] text-muted-foreground/50">
                  {truncatePubkey(pubkey)}
                </p>
              ) : null}
            </div>

            {presenceStatus ? <PresenceBadge status={presenceStatus} /> : null}
          </div>

          {!notesQuery.isLoading && !notesQuery.isError && latestNote ? (
            <div
              className="rounded-lg bg-muted/30 px-3 py-2"
              data-testid="user-profile-latest-note"
            >
              <p className="line-clamp-2 text-xs text-foreground">
                {latestNote.content}
              </p>
              <p className="mt-1 text-[10px] text-muted-foreground/70">
                {formatRelativeTime(latestNote.createdAt)}
              </p>
            </div>
          ) : null}

          {role === "bot" && (managedAgent || relayAgent) ? (
            <div className="flex flex-wrap gap-1.5">
              {managedAgent?.agentCommand ? (
                <InfoBadge>{runtimeLabel(managedAgent.agentCommand)}</InfoBadge>
              ) : relayAgent?.agentType ? (
                <InfoBadge>{runtimeLabel(relayAgent.agentType)}</InfoBadge>
              ) : null}
              {managedAgent?.model ? (
                <InfoBadge>{managedAgent.model}</InfoBadge>
              ) : null}
              {managedAgent?.acpCommand ? (
                <InfoBadge>ACP: {managedAgent.acpCommand}</InfoBadge>
              ) : null}
            </div>
          ) : null}

          {profile?.about ? (
            <p className="text-xs leading-relaxed text-muted-foreground">
              {profile.about}
            </p>
          ) : null}

          {canViewActivity ? (
            <button
              className="flex w-full items-center gap-2 rounded-lg border border-border/60 px-3 py-2 text-left text-xs font-medium text-foreground transition-colors hover:bg-muted/50"
              data-testid={`user-profile-view-activity-${pubkey}`}
              onClick={() => {
                setOpen(false);
                onOpenAgentSession?.(pubkey);
              }}
              type="button"
            >
              <Activity className="h-3.5 w-3.5 text-muted-foreground" />
              View activity log
            </button>
          ) : null}

          {notesQuery.isError ? (
            <p className="text-xs text-muted-foreground">
              Recent notes are unavailable right now.
            </p>
          ) : null}
        </div>
      </PopoverContent>
    </Popover>
  );
}
