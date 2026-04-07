import type { TimelineMessage } from "@/features/messages/types";

/**
 * Where to show "N replies" for nested thread activity: walk up from the nested
 * message's parent until we hit a **main-timeline** message. Prefer a **direct
 * reply to the thread root** (e.g. the agent's first response) over the root
 * post itself, so hints attach to the Q&A anchor users reply under.
 */
export function threadHintAnchorIdForNestedMessage(
  nested: TimelineMessage,
  messageById: Map<string, TimelineMessage>,
  mainMessageIds: Set<string>,
): string {
  let pid: string | null | undefined = nested.parentId;
  let rootCandidate: string | null = null;

  while (pid) {
    const msg = messageById.get(pid);
    if (!msg) {
      break;
    }

    if (mainMessageIds.has(pid)) {
      const isDirectReplyToThreadRoot = Boolean(
        msg.parentId && msg.rootId && msg.parentId === msg.rootId,
      );
      if (isDirectReplyToThreadRoot) {
        return pid;
      }
      if (!msg.parentId) {
        rootCandidate = pid;
      }
    }
    pid = msg.parentId;
  }

  return rootCandidate ?? nested.rootId ?? nested.id;
}
