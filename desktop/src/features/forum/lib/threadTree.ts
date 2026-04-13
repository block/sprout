import type { ThreadReply } from "@/shared/api/types";

export type ThreadNode = ThreadReply & { children: ThreadNode[] };

/**
 * Groups flat thread replies into a tree by NIP-10 parent (`parentEventId`),
 * attaching orphans to `rootPostId`.
 */
export function buildReplyTree(
  replies: ThreadReply[],
  rootPostId: string,
): ThreadNode[] {
  const byParent = new Map<string, ThreadReply[]>();
  for (const reply of replies) {
    const pid = reply.parentEventId ?? rootPostId;
    const list = byParent.get(pid) ?? [];
    list.push(reply);
    byParent.set(pid, list);
  }

  function nest(parentId: string): ThreadNode[] {
    return (byParent.get(parentId) ?? []).map((reply) => ({
      ...reply,
      children: nest(reply.eventId),
    }));
  }

  return nest(rootPostId);
}

export function findNodeInTree(
  nodes: ThreadNode[],
  id: string,
): ThreadNode | null {
  for (const n of nodes) {
    if (n.eventId === id) {
      return n;
    }
    const inner = findNodeInTree(n.children, id);
    if (inner) {
      return inner;
    }
  }
  return null;
}
