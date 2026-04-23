import * as React from "react";

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
import { Markdown } from "@/shared/ui/markdown";
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
  const [showAllNotes, setShowAllNotes] = React.useState(false);
  const profileQuery = useUserProfileQuery(open ? pubkey : undefined);
  const notesQuery = useUserNotesQuery(open ? pubkey : undefined, {
    limit: showAllNotes ? 20 : 3,
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

  const relayAgent = relayAgentsQuery.data?.find((a) => a.pubkey === pubkey);
  const managedAgent = managedAgentsQuery.data?.find(
    (a) => a.pubkey === pubkey,
  );
  const profile = profileQuery.data;
  const notes = notesQuery.data?.notes ?? [];
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
            </div>

            {presenceStatus ? <PresenceBadge status={presenceStatus} /> : null}
          </div>

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

          <p className="truncate font-mono text-[10px] text-muted-foreground/60">
            {truncatePubkey(pubkey)}
          </p>

          {notesQuery.isLoading ? (
            <div
              className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground"
              data-testid="user-profile-notes-loading"
            >
              Loading recent notes…
            </div>
          ) : null}

          {!notesQuery.isLoading && notes.length > 0 ? (
            <div
              className="border-t border-border/60 pt-3"
              data-testid="user-profile-notes"
            >
              <div className="mb-2 flex items-center justify-between">
                <p className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Recent Notes
                </p>
                {notes.length >= 3 ? (
                  <button
                    className="text-[11px] text-primary hover:underline"
                    onClick={() => setShowAllNotes(!showAllNotes)}
                    type="button"
                  >
                    {showAllNotes ? "Show less" : "View all"}
                  </button>
                ) : null}
              </div>
              <div
                className={`space-y-2 ${showAllNotes ? "max-h-64 overflow-y-auto" : ""}`}
              >
                {notes.map((note) => (
                  <article
                    className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2"
                    data-testid="user-profile-note"
                    key={note.id}
                  >
                    <p className="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground/80">
                      {formatRelativeTime(note.createdAt)}
                    </p>
                    <Markdown
                      className="max-w-none text-xs text-foreground"
                      content={note.content}
                      tight
                    />
                  </article>
                ))}
              </div>
            </div>
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
