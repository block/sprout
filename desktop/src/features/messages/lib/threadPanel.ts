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

function normalizeBranchReply(message: TimelineMessage): TimelineMessage {
  return {
    ...message,
    // Thread-panel replies render flat like Slack's side thread view.
    depth: 0,
  };
}

function buildSummaryParticipants(
  replies: TimelineMessage[],
): TimelineThreadSummaryParticipant[] {
  const recentUniqueParticipants = new Map<string, TimelineThreadSummaryParticipant>();

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

export function buildMainTimelineEntries(
  messages: TimelineMessage[],
): MainTimelineEntry[] {
  const directChildrenByParentId = buildDirectChildrenByParentId(messages);

  return messages
    .filter((message) => message.parentId == null)
    .map((message) => {
      return {
        message,
        summary: buildSummaryForDirectReplies(message.id, directChildrenByParentId),
      };
    });
}

export function buildThreadPanelData(
  messages: TimelineMessage[],
  openThreadHeadId: string | null,
  threadReplyTargetId: string | null,
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
  const directReplies = (directChildrenByParentId.get(openThreadHeadId) ?? []).map(
    (message) => normalizeBranchReply(message),
  );
  const visibleReplies = directReplies.map((message) => ({
    message,
    summary: buildSummaryForDirectReplies(message.id, directChildrenByParentId),
  }));

  const replyTargetInBranch =
    threadReplyTargetId === threadHead.id
      ? normalizedThreadHead
      : messageById.get(threadReplyTargetId ?? "") ?? null;

  return {
    threadHead: normalizedThreadHead,
    totalReplyCount: directReplies.length,
    visibleReplies,
    replyTargetMessage: replyTargetInBranch ?? normalizedThreadHead,
  };
}
