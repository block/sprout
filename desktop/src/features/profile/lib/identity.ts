import type { UserProfileSummary } from "@/shared/api/types";

export type UserProfileLookup = Record<string, UserProfileSummary>;

export function truncatePubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-4)}`;
}

function normalizePubkey(pubkey: string) {
  return pubkey.toLowerCase();
}

function getResolvedProfile(
  pubkey: string,
  profiles: UserProfileLookup | undefined,
) {
  if (!profiles) {
    return null;
  }

  return profiles[normalizePubkey(pubkey)] ?? null;
}

export function resolveUserLabel(input: {
  pubkey: string;
  currentPubkey?: string;
  fallbackName?: string | null;
  profiles?: UserProfileLookup;
  preferResolvedSelfLabel?: boolean;
}) {
  const {
    currentPubkey,
    fallbackName,
    preferResolvedSelfLabel = false,
    profiles,
    pubkey,
  } = input;

  if (
    typeof currentPubkey === "string" &&
    normalizePubkey(currentPubkey) === normalizePubkey(pubkey)
  ) {
    if (!preferResolvedSelfLabel) {
      return "You";
    }
  }

  const profile = getResolvedProfile(pubkey, profiles);
  const displayName = profile?.displayName?.trim();
  if (displayName) {
    return displayName;
  }

  const nip05Handle = profile?.nip05Handle?.trim();
  if (nip05Handle) {
    return nip05Handle;
  }

  const safeFallback = fallbackName?.trim();
  if (safeFallback) {
    return safeFallback;
  }

  return truncatePubkey(pubkey);
}

export function resolveUserSecondaryLabel(input: {
  pubkey: string;
  profiles?: UserProfileLookup;
}) {
  const profile = getResolvedProfile(input.pubkey, input.profiles);
  const displayName = profile?.displayName?.trim();
  const nip05Handle = profile?.nip05Handle?.trim();

  if (displayName && nip05Handle) {
    return nip05Handle;
  }

  return null;
}
