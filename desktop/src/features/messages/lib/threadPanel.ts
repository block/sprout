import type { TimelineMessage } from "@/features/messages/types";

type ThreadPanelData = {
  threadHead: TimelineMessage | null;
  totalReplyCount: number;
  visibleReplies: MainTimelineEntry[];
  replyTargetMessage: TimelineMessage | null;
};

export type TimelineThreadSummaryParticipant = {
  id: string;
  author: string;
  avatarUrl: string | null;
};

export type TimelineThreadSummary = {
  threadHeadId: string;
  replyCount: number;
  participants: TimelineThreadSummaryParticipant[];
};

export type MainTimelineEntry = {
  message: TimelineMessage;
  summary: TimelineThreadSummary | null;
};

function normalizeHeadMessage(message: TimelineMessage): TimelineMessage {
  return {
    ...message,
    depth: 0,
  };
}

function normalizeInlineReplyMessage(
  message: TimelineMessage,
  depth: number,
): TimelineMessage {
  return {
    ...message,
    depth,
  };
}

function buildSummaryParticipants(
  replies: TimelineMessage[],
): TimelineThreadSummaryParticipant[] {
  const recentUniqueParticipants = new Map<
    string,
    TimelineThreadSummaryParticipant
  >();

  for (let index = replies.length - 1; index >= 0; index -= 1) {
    const reply = replies[index];
    const participantKey = reply.pubkey ?? reply.id;
    if (recentUniqueParticipants.has(participantKey)) {
      continue;
    }

    recentUniqueParticipants.set(participantKey, {
      id: participantKey,
      author: reply.author,
      avatarUrl: reply.avatarUrl ?? null,
    });

    if (recentUniqueParticipants.size >= 3) {
      break;
    }
  }

  return [...recentUniqueParticipants.values()].reverse();
}

function buildDirectChildrenByParentId(messages: TimelineMessage[]) {
  const childrenByParentId = new Map<string, TimelineMessage[]>();

  for (const message of messages) {
    if (!message.parentId) {
      continue;
    }

    const children = childrenByParentId.get(message.parentId) ?? [];
    children.push(message);
    childrenByParentId.set(message.parentId, children);
  }

  return childrenByParentId;
}

function buildSummaryForDirectReplies(
  messageId: string,
  directChildrenByParentId: Map<string, TimelineMessage[]>,
): TimelineThreadSummary | null {
  const directReplies = directChildrenByParentId.get(messageId) ?? [];
  if (directReplies.length === 0) {
    return null;
  }

  return {
    threadHeadId: messageId,
    replyCount: directReplies.length,
    participants: buildSummaryParticipants(directReplies),
  };
}

function appendExpandedReplies(params: {
  entries: MainTimelineEntry[];
  parentId: string;
  depth: number;
  directChildrenByParentId: Map<string, TimelineMessage[]>;
  expandedReplyIds: ReadonlySet<string>;
}) {
  const {
    entries,
    parentId,
    depth,
    directChildrenByParentId,
    expandedReplyIds,
  } = params;
  const directReplies = directChildrenByParentId.get(parentId) ?? [];

  for (const reply of directReplies) {
    entries.push({
      message: normalizeInlineReplyMessage(reply, depth),
      summary: buildSummaryForDirectReplies(reply.id, directChildrenByParentId),
    });

    if (expandedReplyIds.has(reply.id)) {
      appendExpandedReplies({
        entries,
        parentId: reply.id,
        depth: depth + 1,
        directChildrenByParentId,
        expandedReplyIds,
      });
    }
  }
}

function buildVisibleThreadReplies(params: {
  openThreadHeadId: string;
  directChildrenByParentId: Map<string, TimelineMessage[]>;
  expandedReplyIds: ReadonlySet<string>;
}) {
  const { openThreadHeadId, directChildrenByParentId, expandedReplyIds } =
    params;
  const entries: MainTimelineEntry[] = [];

  appendExpandedReplies({
    entries,
    parentId: openThreadHeadId,
    depth: 1,
    directChildrenByParentId,
    expandedReplyIds,
  });

  return entries;
}

export function buildMainTimelineEntries(
  messages: TimelineMessage[],
): MainTimelineEntry[] {
  const directChildrenByParentId = buildDirectChildrenByParentId(messages);

  return messages
    .filter((message) => message.parentId == null)
    .map((message) => {
      return {
        message,
        summary: buildSummaryForDirectReplies(
          message.id,
          directChildrenByParentId,
        ),
      };
    });
}

export function buildThreadPanelData(
  messages: TimelineMessage[],
  openThreadHeadId: string | null,
  threadReplyTargetId: string | null,
  expandedReplyIds: ReadonlySet<string>,
): ThreadPanelData {
  if (!openThreadHeadId) {
    return {
      threadHead: null,
      totalReplyCount: 0,
      visibleReplies: [],
      replyTargetMessage: null,
    };
  }

  const messageById = new Map(messages.map((message) => [message.id, message]));
  const threadHead = messageById.get(openThreadHeadId) ?? null;

  if (!threadHead) {
    return {
      threadHead: null,
      totalReplyCount: 0,
      visibleReplies: [],
      replyTargetMessage: null,
    };
  }

  const directChildrenByParentId = buildDirectChildrenByParentId(messages);
  const normalizedThreadHead = normalizeHeadMessage(threadHead);
  const visibleReplies = buildVisibleThreadReplies({
    openThreadHeadId,
    directChildrenByParentId,
    expandedReplyIds,
  });

  const replyTargetInBranch =
    threadReplyTargetId === threadHead.id
      ? normalizedThreadHead
      : (messageById.get(threadReplyTargetId ?? "") ?? null);

  return {
    threadHead: normalizedThreadHead,
    totalReplyCount: directChildrenByParentId.get(openThreadHeadId)?.length ?? 0,
    visibleReplies,
    replyTargetMessage: replyTargetInBranch ?? normalizedThreadHead,
  };
}
