const PUBKEY_HEX_RE = /^[0-9a-f]{64}$/i;

function normalizePubkey(pubkey: string) {
  return pubkey.toLowerCase();
}

function getTaggedPubkey(
  tags: string[][],
  tagName: string,
  options?: {
    firstTagOnly?: boolean;
  },
) {
  const candidates = options?.firstTagOnly ? tags.slice(0, 1) : tags;

  for (const tag of candidates) {
    const taggedPubkey = tag[0] === tagName ? tag[1]?.toLowerCase() : null;
    if (taggedPubkey && PUBKEY_HEX_RE.test(taggedPubkey)) {
      return taggedPubkey;
    }
  }

  return null;
}

export function resolveEventAuthorPubkey(input: {
  pubkey: string;
  tags: string[][];
  preferActorTag?: boolean;
  requireChannelTagForPTags?: boolean;
}) {
  const {
    preferActorTag = false,
    pubkey,
    requireChannelTagForPTags = false,
    tags,
  } = input;

  if (preferActorTag) {
    const actorPubkey = getTaggedPubkey(tags, "actor");
    if (actorPubkey) {
      return actorPubkey;
    }
  }

  const canUseAttributedPTag =
    !requireChannelTagForPTags || tags.some((tag) => tag[0] === "h");
  if (canUseAttributedPTag) {
    const attributedPubkey = getTaggedPubkey(tags, "p", { firstTagOnly: true });
    if (attributedPubkey) {
      return attributedPubkey;
    }
  }

  return normalizePubkey(pubkey);
}
