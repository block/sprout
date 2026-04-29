import type { UserProfileLookup } from "@/features/profile/lib/identity";
import {
  describeSystemEvent,
  parseSystemMessagePayload,
} from "@/features/messages/lib/describeSystemEvent";
import { iconForSystemEvent } from "@/features/messages/lib/systemEventIcons";
import { MessageTimestamp } from "./MessageTimestamp";

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
  const payload = parseSystemMessagePayload(body);
  if (!payload) {
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

  const Icon = iconForSystemEvent(payload.type);

  return (
    <div
      className="flex items-center gap-2.5 px-2 py-1"
      data-testid="system-message-row"
    >
      <div className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-muted">
        <Icon className="h-3 w-3 text-muted-foreground" />
      </div>
      <p className="text-xs text-muted-foreground">{description}</p>
      <span className="ml-auto text-xs text-muted-foreground/60">
        <MessageTimestamp createdAt={createdAt} time={time} />
      </span>
    </div>
  );
}
