import type { TimelineMessage } from "@/features/messages/types";

export type CollapsedThreadPreview = {
  participants: Array<{
    avatarUrl: string | null | undefined;
    id: string;
    label: string;
  }>;
  replyCount: number;
};

export function buildCollapsedThreadTimeline(
  messages: TimelineMessage[],
  collapseDepthAt = 2,
) {
  const messageById = new Map(messages.map((message) => [message.id, message]));
  const visibleMessages: TimelineMessage[] = [];
  const hiddenRepliesByAnchor = new Map<string, TimelineMessage[]>();

  for (const message of messages) {
    if (message.depth < collapseDepthAt) {
      visibleMessages.push(message);
      continue;
    }

    let anchorId = message.parentId ?? null;
    while (anchorId) {
      const ancestor = messageById.get(anchorId);
      if (!ancestor) {
        anchorId = null;
        break;
      }
      if (ancestor.depth < collapseDepthAt) {
        const group = hiddenRepliesByAnchor.get(ancestor.id) ?? [];
        group.push(message);
        hiddenRepliesByAnchor.set(ancestor.id, group);
        break;
      }
      anchorId = ancestor.parentId ?? null;
    }

    if (!anchorId) {
      visibleMessages.push(message);
    }
  }

  const summaryByMessageId = new Map<string, CollapsedThreadPreview>();
  for (const [messageId, hiddenReplies] of hiddenRepliesByAnchor) {
    const participants = new Map<
      string,
      { avatarUrl: string | null | undefined; id: string; label: string }
    >();

    for (let index = hiddenReplies.length - 1; index >= 0; index -= 1) {
      const reply = hiddenReplies[index];
      const key =
        reply.pubkey?.toLowerCase() ?? `author:${reply.author.toLowerCase()}`;
      if (!participants.has(key)) {
        participants.set(key, {
          avatarUrl: reply.avatarUrl,
          id: key,
          label: reply.author,
        });
      }
      if (participants.size >= 3) {
        break;
      }
    }

    summaryByMessageId.set(messageId, {
      participants: [...participants.values()],
      replyCount: hiddenReplies.length,
    });
  }

  return { summaryByMessageId, visibleMessages };
}
