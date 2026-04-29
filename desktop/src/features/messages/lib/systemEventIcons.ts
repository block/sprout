import type { LucideIcon } from "lucide-react";
import {
  AlignLeft,
  ArrowRightLeft,
  Hash,
  LogOut,
  Plus,
  UserMinus,
  UserPlus,
} from "lucide-react";

import type { SystemMessagePayload } from "./describeSystemEvent";

const iconByType: Record<string, LucideIcon> = {
  member_joined: UserPlus,
  member_left: LogOut,
  member_removed: UserMinus,
  topic_changed: Hash,
  purpose_changed: AlignLeft,
  channel_created: Plus,
};

/** Return the lucide icon component for a single system event type. */
export function iconForSystemEvent(type: string): LucideIcon {
  return iconByType[type] ?? ArrowRightLeft;
}

/**
 * Pick the best icon for a group of system events.
 * Uses the most frequent event type; falls back to `ArrowRightLeft`.
 */
export function iconForSystemEventGroup(
  payloads: SystemMessagePayload[],
): LucideIcon {
  if (payloads.length === 0) return ArrowRightLeft;

  const counts = new Map<string, number>();
  for (const p of payloads) {
    counts.set(p.type, (counts.get(p.type) ?? 0) + 1);
  }

  let dominant = payloads[0].type;
  let max = 0;
  for (const [type, count] of counts) {
    if (count > max) {
      max = count;
      dominant = type;
    }
  }

  return iconByType[dominant] ?? ArrowRightLeft;
}
