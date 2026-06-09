/**
 * Per-user Concierge selection: which managed agent gets the special Home
 * placement. Keyed by the agent's pubkey (rename-proof) and stored per
 * identity in localStorage. Local-only for v1 — no relay sync.
 */

const STORAGE_KEY_PREFIX = "sprout-concierge.v1";

/** Same-tab change notification — localStorage `storage` events only fire in
 *  OTHER tabs, so writers dispatch this for live UI (e.g. the Home launcher). */
export const SELECTION_CHANGED_EVENT = "sprout-concierge-selection-changed";

export type ConciergeSelection = {
  agentPubkey: string;
  updatedAt: number;
};

export function selectionStorageKey(selfPubkey: string): string {
  return `${STORAGE_KEY_PREFIX}:${selfPubkey}`;
}

/** Validate an unknown payload into a selection, or null. Pure. */
export function parseSelection(json: unknown): ConciergeSelection | null {
  if (typeof json !== "object" || json === null) return null;
  const obj = json as Record<string, unknown>;
  if (typeof obj.agentPubkey !== "string" || obj.agentPubkey.length === 0) {
    return null;
  }
  if (typeof obj.updatedAt !== "number" || !Number.isFinite(obj.updatedAt)) {
    return null;
  }
  return { agentPubkey: obj.agentPubkey, updatedAt: obj.updatedAt };
}

export function readConciergeSelection(
  selfPubkey: string,
): ConciergeSelection | null {
  try {
    const raw = window.localStorage.getItem(selectionStorageKey(selfPubkey));
    if (!raw) return null;
    return parseSelection(JSON.parse(raw));
  } catch {
    return null;
  }
}

export function writeConciergeSelection(
  selfPubkey: string,
  agentPubkey: string,
): boolean {
  const selection: ConciergeSelection = {
    agentPubkey,
    updatedAt: Math.floor(Date.now() / 1000),
  };
  try {
    window.localStorage.setItem(
      selectionStorageKey(selfPubkey),
      JSON.stringify(selection),
    );
    window.dispatchEvent(new Event(SELECTION_CHANGED_EVENT));
    return true;
  } catch {
    return false;
  }
}

export function clearConciergeSelection(selfPubkey: string): void {
  try {
    window.localStorage.removeItem(selectionStorageKey(selfPubkey));
    window.dispatchEvent(new Event(SELECTION_CHANGED_EVENT));
  } catch {
    /* storage unavailable — nothing to clear */
  }
}
