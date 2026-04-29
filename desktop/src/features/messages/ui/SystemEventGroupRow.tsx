import * as React from "react";
import { ChevronRight } from "lucide-react";

import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import {
  type UserProfileLookup,
  resolveUserLabel,
} from "@/features/profile/lib/identity";
import {
  parseSystemMessagePayload,
  type SystemMessagePayload,
} from "@/features/messages/lib/describeSystemEvent";
import { iconForSystemEventGroup } from "@/features/messages/lib/systemEventIcons";
import { cn } from "@/shared/lib/cn";
import { SystemMessageRow } from "./SystemMessageRow";

// ---------------------------------------------------------------------------
// Summary builder
// ---------------------------------------------------------------------------

/** Resolve an actor pubkey to a display name. */
function resolveActorName(
  pubkey: string | undefined,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string {
  if (!pubkey) return "Someone";
  return resolveUserLabel({ pubkey, currentPubkey, profiles });
}

/** Describe a single action type + count as a fragment (no actor prefix). */
function describeAction(type: string, count: number): string | null {
  switch (type) {
    case "member_joined_self":
      return count === 1
        ? "joined the channel"
        : `joined the channel (×${count})`;
    case "member_joined":
      return `added ${count} member${count === 1 ? "" : "s"}`;
    case "member_left":
      return count === 1 ? "left the channel" : `left the channel (×${count})`;
    case "member_removed":
      return `removed ${count} member${count === 1 ? "" : "s"}`;
    case "topic_changed":
      return `changed the topic`;
    case "purpose_changed":
      return `changed the purpose`;
    case "channel_created":
      return "created this channel";
    default:
      return null;
  }
}

/**
 * Build a summary grouped by actor.
 *
 * Single actor, one action:  "tho added 5 members"
 * Single actor, mixed:       "tho added 3 members, removed 2 members"
 * Multi actor (semicolons):  "tho added 5 members; wes added 2 members"
 * Self-join:                 "tho joined the channel"
 */
function buildSummary(
  payloads: SystemMessagePayload[],
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
  _personaLookup?: Map<string, string>,
): string {
  // Group counts by actor → type.
  const actorTypes = new Map<string, Map<string, number>>();
  // Preserve insertion order of actors.
  const actorOrder: string[] = [];

  for (const p of payloads) {
    const actorKey = p.actor ?? "__unknown__";
    let typeMap = actorTypes.get(actorKey);
    if (!typeMap) {
      typeMap = new Map();
      actorTypes.set(actorKey, typeMap);
      actorOrder.push(actorKey);
    }
    // Distinguish self-joins ("joined") from adds ("added N members").
    const type =
      p.type === "member_joined" && p.actor === p.target
        ? "member_joined_self"
        : p.type;
    typeMap.set(type, (typeMap.get(type) ?? 0) + 1);
  }

  const clauses: string[] = [];

  for (const actorKey of actorOrder) {
    const name = resolveActorName(
      actorKey === "__unknown__" ? undefined : actorKey,
      currentPubkey,
      profiles,
    );
    const typeMap = actorTypes.get(actorKey);
    if (!typeMap) continue;
    const actions: string[] = [];

    for (const [type, count] of typeMap) {
      const desc = describeAction(type, count);
      if (desc) actions.push(desc);
    }

    if (actions.length === 0) continue;

    // First action gets the actor name; subsequent actions for the same actor
    // omit the name to read naturally: "tho added 3 members, removed 2 members"
    clauses.push(`${name} ${actions.join(", ")}`);
  }

  return clauses.length > 0
    ? clauses.join("; ")
    : `${payloads.length} system event${payloads.length === 1 ? "" : "s"}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SystemEventGroupRow({
  entries,
  currentPubkey,
  personaLookup,
  profiles,
}: {
  entries: MainTimelineEntry[];
  currentPubkey?: string;
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
}) {
  const [expanded, setExpanded] = React.useState(false);

  const payloads = React.useMemo(
    () =>
      entries
        .map((e) => parseSystemMessagePayload(e.message.body))
        .filter((p): p is SystemMessagePayload => p !== null),
    [entries],
  );

  const summary = React.useMemo(
    () => buildSummary(payloads, currentPubkey, profiles, personaLookup),
    [payloads, currentPubkey, profiles, personaLookup],
  );

  const GroupIcon = React.useMemo(
    () => iconForSystemEventGroup(payloads),
    [payloads],
  );

  const groupId = React.useId();
  const panelId = `${groupId}-panel`;

  return (
    <div data-testid="system-event-group">
      {/* Collapsed summary row */}
      <button
        aria-controls={panelId}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1 text-left transition-colors hover:bg-muted/50"
        data-testid="system-event-group-toggle"
        onClick={() => setExpanded((prev) => !prev)}
        type="button"
      >
        <div className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-muted">
          <GroupIcon className="h-3 w-3 text-muted-foreground" />
        </div>
        <p className="flex-1 text-xs text-muted-foreground">{summary}</p>
        <ChevronRight
          className={cn(
            "h-3.5 w-3.5 shrink-0 text-muted-foreground/60 transition-transform duration-150",
            expanded && "rotate-90",
          )}
        />
      </button>

      {/* Expanded children */}
      {expanded ? (
        <section
          className="ml-4 border-l border-border/50 pl-2"
          data-testid="system-event-group-children"
          id={panelId}
        >
          {entries.map((entry) => (
            <SystemMessageRow
              key={entry.message.id}
              body={entry.message.body}
              createdAt={entry.message.createdAt}
              currentPubkey={currentPubkey}
              personaLookup={personaLookup}
              profiles={profiles}
              time={entry.message.time}
            />
          ))}
        </section>
      ) : null}
    </div>
  );
}
