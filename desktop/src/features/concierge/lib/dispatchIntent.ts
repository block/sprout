import type { DispatchIntent } from "@/features/concierge/types";

/**
 * Dispatch protocol: the Concierge agent proposes actions as fenced
 * ```dispatch blocks containing JSON `{ "agent", "channel", "instruction" }`.
 * Nothing is sent until the human approves the rendered card — the card is
 * the safety boundary, not the prompt.
 */
const DISPATCH_FENCE = /```dispatch\s*\n([\s\S]*?)```/g;

export type ParsedDispatch = {
  /** Intents found in the message, in document order. */
  intents: DispatchIntent[];
  /** Message content with the dispatch fences removed. */
  cleanedContent: string;
};

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * Extract dispatch intents from an agent message. Malformed blocks are
 * dropped (fail-closed: an unparseable proposal must never become a card the
 * user can approve). Intent ids are `<messageId>:<index>` so settled state
 * can be keyed stably across re-renders and reloads.
 */
export function parseDispatchIntents(
  messageId: string,
  content: string,
): ParsedDispatch {
  const intents: DispatchIntent[] = [];
  let index = 0;
  const cleanedContent = content
    .replace(DISPATCH_FENCE, (_match, body: string) => {
      try {
        const parsed = JSON.parse(body) as Record<string, unknown>;
        if (
          isNonEmptyString(parsed.agent) &&
          isNonEmptyString(parsed.channel) &&
          isNonEmptyString(parsed.instruction)
        ) {
          intents.push({
            id: `${messageId}:${index}`,
            agent: parsed.agent.trim().replace(/^@/, ""),
            channel: parsed.channel.trim().replace(/^#/, ""),
            instruction: parsed.instruction.trim(),
            status: "pending",
          });
        }
      } catch {
        // Malformed JSON — drop the block silently.
      }
      index += 1;
      return "";
    })
    .trim();

  return { intents, cleanedContent };
}

// ── Settled-state persistence ─────────────────────────────────────────────────
// Approved/dismissed decisions are keyed by intent id in localStorage so a
// reload never resurrects an already-settled card as approvable.

export type SettledDispatchMap = Record<string, "approved" | "dismissed">;

export function dispatchStorageKey(selfPubkey: string): string {
  return `sprout-concierge-dispatch.v1:${selfPubkey.trim().toLowerCase()}`;
}

export function readSettledDispatches(raw: string | null): SettledDispatchMap {
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const out: SettledDispatchMap = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (value === "approved" || value === "dismissed") out[key] = value;
    }
    return out;
  } catch {
    return {};
  }
}

export function applySettledStatus(
  intent: DispatchIntent,
  settled: SettledDispatchMap,
): DispatchIntent {
  const status = settled[intent.id];
  return status ? { ...intent, status } : intent;
}
