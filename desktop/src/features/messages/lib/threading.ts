import type { RelayEvent } from "@/shared/api/types";

export type ThreadReference = {
  parentId: string | null;
  rootId: string | null;
};

function getEventTags(tags: string[][]) {
  return tags.filter((tag) => tag[0] === "e" && typeof tag[1] === "string");
}

export function getChannelIdFromTags(tags: string[][]) {
  return tags.find((tag) => tag[0] === "h")?.[1] ?? null;
}

export function getThreadReference(tags: string[][]): ThreadReference {
  const eventTags = getEventTags(tags);

  if (eventTags.length === 0) {
    return {
      parentId: null,
      rootId: null,
    };
  }

  const rootTag = eventTags.find((tag) => tag[3] === "root");
  const replyTag =
    [...eventTags].reverse().find((tag) => tag[3] === "reply") ?? null;

  if (!replyTag) {
    return {
      parentId: null,
      rootId: null,
    };
  }

  const parentId = replyTag[1] ?? null;

  return {
    parentId,
    rootId: rootTag?.[1] ?? parentId,
  };
}

export function buildReplyTags(
  channelId: string,
  authorPubkey: string,
  parentEventId: string,
  rootEventId: string,
) {
  const tags: string[][] = [
    ["p", authorPubkey],
    ["h", channelId],
  ];

  if (parentEventId === rootEventId) {
    tags.push(["e", rootEventId, "", "reply"]);
    return tags;
  }

  tags.push(["e", rootEventId, "", "root"]);
  tags.push(["e", parentEventId, "", "reply"]);
  return tags;
}

export function resolveReplyRootId(
  parentEventId: string,
  events: RelayEvent[],
) {
  const parent = events.find((event) => event.id === parentEventId);
  if (!parent) {
    return parentEventId;
  }

  const thread = getThreadReference(parent.tags);
  return thread.rootId ?? parent.id;
}
