import type { TimelineMessage } from "@/features/messages/types";

type ThreadBranch = {
  headMessage: TimelineMessage | null;
  messageIds: Set<string>;
  messages: TimelineMessage[];
};

/**
 * Collect the visible branch for a selected thread head.
 *
 * The underlying NIP-10 root may point to an older message, but the UI thread
 * panel should anchor to the specific message the user chose to discuss and
 * then show only descendants from that branch.
 */
export function collectThreadBranch(
  messages: TimelineMessage[],
  branchHeadId: string | null,
): ThreadBranch {
  if (!branchHeadId) {
    return {
      headMessage: null,
      messageIds: new Set<string>(),
      messages: [],
    };
  }

  const headMessage =
    messages.find((message) => message.id === branchHeadId) ?? null;
  if (!headMessage) {
    return {
      headMessage: null,
      messageIds: new Set<string>(),
      messages: [],
    };
  }

  const childrenByParent = new Map<string, string[]>();
  for (const message of messages) {
    if (!message.parentId) {
      continue;
    }
    const children = childrenByParent.get(message.parentId) ?? [];
    children.push(message.id);
    childrenByParent.set(message.parentId, children);
  }

  const branchIds = new Set<string>([branchHeadId]);
  for (const message of messages) {
    if (message.branchHeadId === branchHeadId) {
      branchIds.add(message.id);
    }
  }
  const stack = [branchHeadId];
  while (stack.length > 0) {
    const parentId = stack.pop();
    if (!parentId) {
      continue;
    }
    const children = childrenByParent.get(parentId) ?? [];
    for (const childId of children) {
      if (branchIds.has(childId)) {
        continue;
      }
      branchIds.add(childId);
      stack.push(childId);
    }
  }

  const depthOffset = headMessage.depth + 1;
  const branchMessages = messages
    .filter(
      (message) => message.id !== branchHeadId && branchIds.has(message.id),
    )
    .map((message) => ({
      ...message,
      // Keep normal replies in the drilled branch visually flat.
      // Only explicitly drilled nested branches get their own branchHeadId and collapse link.
      depth:
        message.branchHeadId === branchHeadId
          ? 0
          : Math.max(0, message.depth - depthOffset),
    }));

  return {
    headMessage,
    messageIds: branchIds,
    messages: branchMessages,
  };
}
