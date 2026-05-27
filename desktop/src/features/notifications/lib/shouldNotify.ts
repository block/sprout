import type { RelayEvent } from "@/shared/api/types";
import { getThreadReference } from "@/features/messages/lib/threading";

export function shouldNotifyForEvent(
  event: RelayEvent,
  _currentPubkey: string,
  participatedRootIds: ReadonlySet<string>,
  followedRootIds: ReadonlySet<string>,
): boolean {
  const { parentId, rootId } = getThreadReference(event.tags);

  if (parentId === null) {
    return true;
  }

  if (event.tags.some((tag) => tag[0] === "broadcast" && tag[1] === "1")) {
    return true;
  }

  if (rootId !== null && participatedRootIds.has(rootId)) {
    return true;
  }

  if (rootId !== null && followedRootIds.has(rootId)) {
    return true;
  }

  return false;
}
