import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { resolveUserLabel } from "@/features/profile/lib/identity";

export type SystemMessagePayload = {
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

/**
 * Produce a human-readable description for a single system event payload.
 * Returns `null` for unknown event types.
 */
export function describeSystemEvent(
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

/**
 * Try to parse a system message body into a payload. Returns `null` on failure.
 */
export function parseSystemMessagePayload(
  body: string,
): SystemMessagePayload | null {
  try {
    return JSON.parse(body) as SystemMessagePayload;
  } catch {
    return null;
  }
}
