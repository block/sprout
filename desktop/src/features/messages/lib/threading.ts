import type { RelayEvent } from "@/shared/api/types";

export type ThreadReference = {
  parentId: string | null;
  rootId: string | null;
};

const HEX_64_RE = /^[0-9a-f]{64}$/i;

function getEventTags(tags: string[][]) {
  return tags.filter((tag) => tag[0] === "e" && typeof tag[1] === "string");
}

export function getChannelIdFromTags(tags: string[][]) {
  return tags.find((tag) => tag[0] === "h")?.[1] ?? null;
}

/** NIP-10 `e` reply marker on kind 20002 — which composer surface the typer is using. */
export function getTypingReplyParentFromTags(
  tags: string[][] | undefined,
): string | null {
  if (!tags) {
    return null;
  }
  for (const tag of tags) {
    if (tag[0] === "e" && tag[3] === "reply" && tag[1]) {
      return tag[1];
    }
  }
  return null;
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

export function getThreadBranchHeadFromTags(
  tags: string[][] | undefined,
): string | null {
  if (!tags) {
    return null;
  }

  const branchHeadId = tags.find(
    (tag) => tag[0] === "sprout" && tag[1] === "thread_branch_head",
  )?.[2];

  if (!branchHeadId || !HEX_64_RE.test(branchHeadId)) {
    return null;
  }

  return branchHeadId.toLowerCase();
}

export function buildThreadBranchHeadTag(
  branchHeadId: string,
): string[] | null {
  if (!HEX_64_RE.test(branchHeadId)) {
    return null;
  }

  return ["sprout", "thread_branch_head", branchHeadId.toLowerCase()];
}

/**
 * Best-effort client-side normalization of mention pubkeys: lowercase, deduplicate, skip self.
 * The relay performs authoritative validation (hex format, 64-char length, cap of 50)
 * on top of the same normalization — this helper keeps optimistic UI tags consistent.
 */
export function normalizeMentionPubkeys(
  mentionPubkeys: string[],
  selfPubkey: string,
): string[] {
  const selfLower = selfPubkey.toLowerCase();
  const seen = new Set<string>([selfLower]);
  const result: string[] = [];
  for (const pk of mentionPubkeys) {
    const lower = pk.toLowerCase();
    if (seen.has(lower)) {
      continue;
    }
    seen.add(lower);
    result.push(lower);
  }
  return result;
}

export function buildReplyTags(
  channelId: string,
  authorPubkey: string,
  parentEventId: string,
  rootEventId: string,
  mentionPubkeys: string[] = [],
) {
  const tags: string[][] = [
    ["p", authorPubkey],
    ["h", channelId],
  ];

  // Add p-tags for mentioned users so mention-filtered subscriptions
  // (e.g. ACP agent harness) receive the reply event.
  // Best-effort normalization — relay performs authoritative validation.
  for (const pubkey of normalizeMentionPubkeys(mentionPubkeys, authorPubkey)) {
    tags.push(["p", pubkey]);
  }

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
