import { ArrowRightLeft } from "lucide-react";

import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { MessageTimestamp } from "./MessageTimestamp";

type SystemMessagePayload = {
  type: string;
  actor?: string;
  target?: string;
  topic?: string;
  purpose?: string;
};

function resolveLabel(
  pubkey: string | undefined,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string {
  if (!pubkey) {
    return "Someone";
  }
  return resolveUserLabel({ pubkey, currentPubkey, profiles });
}

function resolvePersonaSuffix(
  pubkey: string | undefined,
  personaLookup: Map<string, string> | undefined,
): string {
  if (!pubkey || !personaLookup) return "";
  const personaName = personaLookup.get(pubkey.toLowerCase());
  return personaName ? ` (${personaName})` : "";
}

function describeSystemEvent(
  payload: SystemMessagePayload,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
  personaLookup?: Map<string, string>,
): string | null {
  const actor = resolveLabel(payload.actor, currentPubkey, profiles);

  switch (payload.type) {
    case "member_joined": {
      const target = resolveLabel(payload.target, currentPubkey, profiles);
      const personaSuffix = resolvePersonaSuffix(payload.target, personaLookup);
      if (payload.actor === payload.target) {
        return `${actor}${personaSuffix} joined the channel`;
      }
      return `${actor} added ${target}${personaSuffix} to the channel`;
    }
    case "member_left": {
      return `${actor} left the channel`;
    }
    case "member_removed": {
      const target = resolveLabel(payload.target, currentPubkey, profiles);
      return `${actor} removed ${target} from the channel`;
    }
    case "topic_changed":
      return `${actor} changed the topic to "${payload.topic}"`;
    case "purpose_changed":
      return `${actor} changed the purpose to "${payload.purpose}"`;
    case "channel_created":
      return `${actor} created this channel`;
    default:
      return null;
  }
}

export function SystemMessageRow({
  body,
  createdAt,
  time,
  currentPubkey,
  profiles,
  personaLookup,
}: {
  body: string;
  createdAt: number;
  time: string;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
}) {
  let payload: SystemMessagePayload;
  try {
    payload = JSON.parse(body);
  } catch {
    return null;
  }

  const description = describeSystemEvent(
    payload,
    currentPubkey,
    profiles,
    personaLookup,
  );
  if (!description) {
    return null;
  }

  return (
    <div
      className="flex items-center gap-2.5 px-2 py-1"
      data-testid="system-message-row"
    >
      <div className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-muted">
        <ArrowRightLeft className="h-3 w-3 text-muted-foreground" />
      </div>
      <p className="text-xs text-muted-foreground">{description}</p>
      <span className="ml-auto text-xs text-muted-foreground/60">
        <MessageTimestamp createdAt={createdAt} time={time} />
      </span>
    </div>
  );
}
