import { nip44DecryptSelf } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";

// ── Storage keys ─────────────────────────────────────────────────────────────

export const CLIENT_ID_STORAGE_KEY = "sprout.read-state-client-id.v1";
export const SLOT_ID_STORAGE_KEY = "sprout.read-state-slot-id.v1";
export const SYNC_ENABLED_STORAGE_KEY = "sprout.read-state-sync-enabled.v1";
export const CACHE_STORAGE_KEY = "sprout.channel-read-state.v1";

// ── Types ────────────────────────────────────────────────────────────────────

export type ReadStateBlob = {
  v: 1;
  client_id: string;
  contexts: Record<string, number>;
};

export type DecodedBlob = {
  blob: ReadStateBlob;
  event: RelayEvent;
};

// ── Random / ID helpers ─────────────────────────────────────────────────────

export function randomHex(length: number): string {
  const bytes = new Uint8Array(length / 2);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function getOrCreateStorageValue(key: string): string {
  const existing = localStorage.getItem(key);
  if (existing && existing.length > 0) {
    return existing;
  }
  const value = randomHex(32);
  localStorage.setItem(key, value);
  return value;
}

// ── Sync-enabled storage ────────────────────────────────────────────────────

export function readSyncEnabled(): boolean {
  const stored = localStorage.getItem(SYNC_ENABLED_STORAGE_KEY);
  // NIP-RS: MUST NOT publish without explicit user opt-in — default OFF
  return stored === "true";
}

export function writeSyncEnabled(enabled: boolean): void {
  localStorage.setItem(SYNC_ENABLED_STORAGE_KEY, String(enabled));
}

// ── Cached read state ───────────────────────────────────────────────────────

/** Read the localStorage cache of merged read state (unix timestamps). */
export function readCachedReadState(): Record<string, number> {
  try {
    const raw = localStorage.getItem(CACHE_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};

    // The old format stored ISO strings; the new format stores unix seconds.
    // Support both: if a value is a string, parse it to unix seconds.
    const result: Record<string, number> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof value === "number" && Number.isFinite(value)) {
        result[key] = value;
      } else if (typeof value === "string") {
        const ms = Date.parse(value);
        if (!Number.isNaN(ms)) {
          result[key] = Math.floor(ms / 1000);
        }
      }
    }
    return result;
  } catch {
    return {};
  }
}

/** Write merged read state to localStorage cache. */
export function writeCachedReadState(state: Record<string, number>): void {
  localStorage.setItem(CACHE_STORAGE_KEY, JSON.stringify(state));
}

// ── Validation per NIP-RS Content Validation ─────────────────────────────────

export function validateDTag(event: RelayEvent): string | null {
  const dTags = event.tags.filter((tag) => tag[0] === "d" && tag.length >= 2);
  if (dTags.length !== 1) return null;
  const dTag = dTags[0];
  const dValue = dTag?.[1];
  if (!dValue) return null;
  if (!dValue.startsWith("read-state:")) return null;
  const slotId = dValue.slice("read-state:".length);
  if (slotId.length === 0 || slotId.length > 64) return null;
  // ASCII check
  for (let i = 0; i < slotId.length; i++) {
    if (slotId.charCodeAt(i) > 127) return null;
  }
  return dValue;
}

export function validateTTag(event: RelayEvent): boolean {
  const tTags = event.tags.filter(
    (tag) => tag[0] === "t" && tag[1] === "read-state",
  );
  return tTags.length === 1;
}

export async function decryptAndValidateBlob(
  event: RelayEvent,
): Promise<DecodedBlob | null> {
  // Validate d tag
  if (!validateDTag(event)) return null;

  // Validate t tag
  if (!validateTTag(event)) return null;

  // Decrypt
  let plaintext: string;
  try {
    plaintext = await nip44DecryptSelf(event.content);
  } catch {
    return null;
  }

  // Parse JSON
  let parsed: unknown;
  try {
    parsed = JSON.parse(plaintext);
  } catch {
    return null;
  }

  if (!parsed || typeof parsed !== "object") return null;
  const obj = parsed as Record<string, unknown>;

  // Validate v
  if (!("v" in obj) || typeof obj.v !== "number" || !Number.isInteger(obj.v)) {
    return null;
  }
  if (obj.v !== 1) return null; // Unknown version — ignore

  // Validate client_id
  if (
    !("client_id" in obj) ||
    typeof obj.client_id !== "string" ||
    obj.client_id.length < 1 ||
    obj.client_id.length > 64
  ) {
    return null;
  }

  // Validate contexts
  if (
    !("contexts" in obj) ||
    typeof obj.contexts !== "object" ||
    obj.contexts === null ||
    Array.isArray(obj.contexts)
  ) {
    return null;
  }

  const rawContexts = obj.contexts as Record<string, unknown>;

  // Reject if > 10,000 entries
  if (Object.keys(rawContexts).length > 10_000) return null;

  // Validate individual entries
  const contexts: Record<string, number> = {};
  for (const [contextId, timestamp] of Object.entries(rawContexts)) {
    // Context ID must not exceed 256 bytes
    if (new TextEncoder().encode(contextId).length > 256) continue;
    // Timestamp must be integer in 0–4294967295
    if (
      typeof timestamp !== "number" ||
      !Number.isInteger(timestamp) ||
      timestamp < 0 ||
      timestamp > 4294967295
    ) {
      continue;
    }
    contexts[contextId] = timestamp;
  }

  return {
    blob: {
      v: 1,
      client_id: obj.client_id as string,
      contexts,
    },
    event,
  };
}

// ── Merge logic (CvRDT max-register) ────────────────────────────────────────

export function mergeContexts(
  base: Record<string, number>,
  incoming: Record<string, number>,
): Record<string, number> {
  const result = { ...base };
  for (const [ctx, ts] of Object.entries(incoming)) {
    result[ctx] = Math.max(result[ctx] ?? 0, ts);
  }
  return result;
}

export function contextsEqual(
  a: Record<string, number>,
  b: Record<string, number>,
): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}
